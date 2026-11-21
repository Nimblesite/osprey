//! Whether Osprey's inference alone gives a definition its Lean type, so the
//! generated module can leave the `name : type` header off ([MATH-CODE-EMIT-ELEGANT]).
//!
//! Every value of the fragment is `Float` or `Bool`. A parameter, a `let` name
//! or the result is pinned once it meets something whose type is fixed: a
//! float or boolean literal, a `Float` builtin, `floatDiv`, a boolean
//! operator, or a definition of the module (whose generated type is its Lean
//! type). Arithmetic and comparison join their operands. Anything left joined
//! to nothing fixed would be generalised, so its header stays.

use crate::model::{Def, Expr, Op, Tail, Ty};

const FLOAT: usize = 0;
const BOOL: usize = 1;

/// The fewest parameters to annotate inline, first to last, so the body fixes every other
/// parameter and the result; `None` when the result stays open and only a header will do.
pub(crate) fn annotated(def: &Def, earlier: &[Def]) -> Option<Vec<usize>> {
    let mut chosen = Vec::new();
    loop {
        let open = open(def, earlier, &chosen);
        match open.first() {
            None => return Some(chosen),
            Some(&index) if index < def.params.len() => chosen.push(index),
            Some(_) => return None,
        }
    }
}

/// The parameters, then the result (at index `params.len()`), still unfixed when `chosen`
/// parameters carry their Lean type.
fn open(def: &Def, earlier: &[Def], chosen: &[usize]) -> Vec<usize> {
    let mut pins = Pins {
        roots: vec![FLOAT, BOOL],
        names: Vec::new(),
        earlier,
    };
    let params: Vec<usize> = def
        .params
        .iter()
        .map(|(name, _)| pins.local(name))
        .collect();
    for (index, (node, (_, ty))) in params.iter().zip(&def.params).enumerate() {
        if chosen.contains(&index) {
            pins.join(*node, of(*ty));
        }
    }
    let result = pins.tail(&def.body);
    let nodes = params.into_iter().chain([result]);
    nodes
        .enumerate()
        .filter(|(_, node)| !pins.fixed(*node))
        .map(|(index, _)| index)
        .collect()
}

struct Pins<'a> {
    roots: Vec<usize>,
    names: Vec<(String, usize)>,
    earlier: &'a [Def],
}

impl Pins<'_> {
    fn fresh(&mut self) -> usize {
        self.roots.push(self.roots.len());
        self.roots.len() - 1
    }

    fn local(&mut self, name: &str) -> usize {
        let node = self.fresh();
        self.names.push((name.to_owned(), node));
        node
    }

    fn root(&self, mut node: usize) -> usize {
        while let Some(&parent) = self.roots.get(node).filter(|parent| **parent != node) {
            node = parent;
        }
        node
    }

    /// Join two nodes; a fixed type is always the root it joins under.
    fn join(&mut self, a: usize, b: usize) {
        let (a, b) = (self.root(a), self.root(b));
        let (root, child) = if b <= BOOL { (b, a) } else { (a, b) };
        if let Some(slot) = self.roots.get_mut(child) {
            *slot = root;
        }
    }

    /// Join two nodes and name what they now are.
    fn joined(&mut self, a: usize, b: usize) -> usize {
        self.join(a, b);
        self.root(a)
    }

    fn fixed(&self, node: usize) -> bool {
        self.root(node) <= BOOL
    }

    fn tail(&mut self, tail: &Tail) -> usize {
        match tail {
            Tail::Expr(term) => self.expr(term),
            Tail::If(test, yes, no) => {
                let test = self.expr(test);
                self.join(test, BOOL);
                let (yes, no) = (self.tail(yes), self.tail(no));
                self.joined(yes, no)
            }
            Tail::Let(name, value, body) => {
                let value = self.tail(value);
                let node = self.local(name);
                self.join(node, value);
                let result = self.tail(body);
                self.names.truncate(self.names.len().saturating_sub(1));
                result
            }
        }
    }

    fn expr(&mut self, term: &Expr) -> usize {
        match term {
            Expr::Num(_) => FLOAT,
            Expr::Bool(_) => BOOL,
            Expr::Name(name) => self.named(name),
            Expr::Call(callee, arguments) => self.call(callee, arguments),
            Expr::Not(operand) => {
                let operand = self.expr(operand);
                self.joined(operand, BOOL)
            }
            Expr::Binary(op, left, right) => self.binary(*op, left, right),
        }
    }

    fn named(&mut self, name: &str) -> usize {
        match self.names.iter().rev().find(|(local, _)| local == name) {
            Some((_, node)) => *node,
            None => self.fresh(),
        }
    }

    fn binary(&mut self, op: Op, left: &Expr, right: &Expr) -> usize {
        let (left, right) = (self.expr(left), self.expr(right));
        match op {
            Op::Or | Op::And => {
                self.join(left, BOOL);
                self.joined(right, BOOL)
            }
            Op::Div => {
                self.join(left, FLOAT);
                self.joined(right, FLOAT)
            }
            Op::Lt | Op::Le | Op::Gt | Op::Ge | Op::Eq | Op::Ne => {
                self.join(left, right);
                BOOL
            }
            Op::Add | Op::Sub | Op::Mul => self.joined(left, right),
        }
    }

    /// A definition takes and returns its Lean types; every builtin takes and returns `Float`,
    /// except `abs`, which Osprey also defines on `int`.
    fn call(&mut self, callee: &str, arguments: &[Expr]) -> usize {
        let nodes: Vec<usize> = arguments
            .iter()
            .map(|argument| self.expr(argument))
            .collect();
        if let Some(def) = self.earlier.iter().find(|def| def.name == callee) {
            for (node, (_, ty)) in nodes.iter().zip(&def.params) {
                self.join(*node, of(*ty));
            }
            return of(def.result);
        }
        if callee == "abs" {
            return nodes.first().copied().unwrap_or(FLOAT);
        }
        for node in nodes {
            self.join(node, FLOAT);
        }
        FLOAT
    }
}

fn of(ty: Ty) -> usize {
    match ty {
        Ty::Float => FLOAT,
        Ty::Bool => BOOL,
    }
}
