//! The order a statement list's functions are checked in: callees before
//! callers, so each is generalized before a caller uses it, while a recursive
//! group keeps its source order. Implements [TYPE-BINDING-ORDER].
use super::{BTreeSet, HashMap, Stmt};

/// One statement list's functions, the dependencies of each, and the recursive
/// group each belongs to.
pub(super) struct BindingOrder<'a> {
    statements: Vec<&'a Stmt>,
    /// Each statement's function dependencies, ordered by name.
    edges: Vec<Vec<usize>>,
    /// Latest binding read by each statement, if it reads a local binding.
    latest_binding: Vec<Option<usize>>,
    /// Each statement's strongly connected component in the call graph.
    groups: Vec<usize>,
    started: Vec<bool>,
}

impl<'a> BindingOrder<'a> {
    pub(super) fn new(statements: Vec<&'a Stmt>) -> Self {
        let mut functions = HashMap::new();
        let mut lets = HashMap::new();
        for (index, statement) in statements.iter().enumerate() {
            match statement {
                Stmt::Function { name, .. } => {
                    let _ = functions.insert(name.as_str(), index);
                }
                Stmt::Let { name, .. } => {
                    let _ = lets.insert(name.as_str(), index);
                }
                _ => {}
            }
        }
        let (edges, latest_binding): (Vec<Vec<usize>>, Vec<Option<usize>>) = statements
            .iter()
            .map(|statement| reads(statement))
            .map(|names| {
                let callees = names
                    .iter()
                    .filter_map(|name| functions.get(name.as_str()).copied())
                    .collect();
                let latest = names
                    .iter()
                    .filter_map(|name| lets.get(name.as_str()).copied())
                    .max();
                (callees, latest)
            })
            .unzip();
        Self {
            groups: Groups::of(&edges),
            edges,
            latest_binding,
            started: vec![false; statements.len()],
            statements,
        }
    }

    /// Mark `index` started, answering its statement unless it already was.
    pub(super) fn start(&mut self, index: usize) -> Option<&'a Stmt> {
        let started = self.started.get_mut(index)?;
        if std::mem::replace(started, true) {
            return None;
        }
        self.statements.get(index).copied()
    }

    /// The functions `index` calls that can be checked before it: unstarted,
    /// outside its recursive group, and reading no `let` at or after `cursor`,
    /// which has not run.
    pub(super) fn callees(&self, index: usize, cursor: usize) -> Vec<usize> {
        let group = self.groups.get(index);
        self.edges
            .get(index)
            .into_iter()
            .flatten()
            .copied()
            .filter(|&callee| self.groups.get(callee) != group && self.ready(callee, cursor))
            .collect()
    }

    fn ready(&self, callee: usize, cursor: usize) -> bool {
        let unstarted = self.started.get(callee) == Some(&false);
        unstarted
            && self
                .latest_binding
                .get(callee)
                .is_some_and(|bound| bound.is_none_or(|at| at < cursor))
    }
}

/// The free names a function's body reads, less its own parameters.
fn reads(statement: &Stmt) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    if let Stmt::Function {
        parameters, body, ..
    } = statement
    {
        osprey_ast::freevars::free_idents(body, &mut names);
        for parameter in parameters {
            let _ = names.remove(&parameter.name);
        }
    }
    names
}

/// Tarjan's strongly connected components over the call graph.
struct Groups<'e> {
    edges: &'e [Vec<usize>],
    index: Vec<Option<usize>>,
    low: Vec<usize>,
    on_stack: Vec<bool>,
    stack: Vec<usize>,
    group: Vec<usize>,
    next: usize,
}

impl<'e> Groups<'e> {
    /// Each node's component id.
    fn of(edges: &'e [Vec<usize>]) -> Vec<usize> {
        let count = edges.len();
        let mut groups = Self {
            edges,
            index: vec![None; count],
            low: vec![0; count],
            on_stack: vec![false; count],
            stack: Vec::new(),
            group: (0..count).collect(),
            next: 0,
        };
        for node in 0..count {
            if groups.index.get(node) == Some(&None) {
                groups.visit(node);
            }
        }
        groups.group
    }

    fn visit(&mut self, node: usize) {
        self.enter(node);
        let edges = self.edges;
        for &next in edges.get(node).into_iter().flatten() {
            match self.index.get(next).copied().flatten() {
                None => {
                    self.visit(next);
                    self.lower(node, self.low.get(next).copied());
                }
                Some(seen) if self.on_stack.get(next) == Some(&true) => {
                    self.lower(node, Some(seen));
                }
                Some(_) => {}
            }
        }
        if self.low.get(node) == self.index.get(node).copied().flatten().as_ref() {
            self.close(node);
        }
    }

    fn enter(&mut self, node: usize) {
        if let (Some(index), Some(low), Some(on)) = (
            self.index.get_mut(node),
            self.low.get_mut(node),
            self.on_stack.get_mut(node),
        ) {
            *index = Some(self.next);
            *low = self.next;
            *on = true;
        }
        self.next = self.next.saturating_add(1);
        self.stack.push(node);
    }

    fn lower(&mut self, node: usize, to: Option<usize>) {
        if let (Some(low), Some(to)) = (self.low.get_mut(node), to) {
            *low = (*low).min(to);
        }
    }

    /// Pop `root`'s component, naming every member after the root.
    fn close(&mut self, root: usize) {
        while let Some(member) = self.stack.pop() {
            if let (Some(on), Some(group)) =
                (self.on_stack.get_mut(member), self.group.get_mut(member))
            {
                *on = false;
                *group = root;
            }
            if member == root {
                break;
            }
        }
    }
}
