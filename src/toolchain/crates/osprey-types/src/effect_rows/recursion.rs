//! Effect row recursion: which functions lie on a cycle of named references.
use super::{reachability::named_references, BTreeSet, Index};

/// The named-reference graph with each function's strongly connected
/// component: the order a fixed point settles functions in, callees first.
pub(super) struct Callees {
    edges: Vec<Vec<usize>>,
    components: Vec<Vec<usize>>,
    component_of: Vec<usize>,
}

impl Callees {
    pub(super) fn of(index: &Index<'_>) -> Callees {
        let edges = reference_edges(index);
        let components = components(&edges);
        let mut component_of = vec![0; edges.len()];
        for (component, members) in components.iter().enumerate() {
            for member in members {
                if let Some(slot) = component_of.get_mut(*member) {
                    *slot = component;
                }
            }
        }
        Callees {
            edges,
            components,
            component_of,
        }
    }

    /// Every function on a reference cycle: a strongly connected component of
    /// more than one function, or a function that names itself. A function off
    /// every cycle is a finite function of its callees' returns and settles once
    /// they do; only a function on a cycle can feed its own return back.
    ///
    /// Conservative on purpose, like [`super::reachability::reachable_bodies`]:
    /// any mention of a function's name counts as an edge, so a cycle is never
    /// missed.
    pub(super) fn recursive(&self) -> BTreeSet<usize> {
        let cycles = self.components.iter().filter(|c| is_cycle(&self.edges, c));
        cycles.flatten().copied().collect()
    }

    /// Whether `id` still waits on a `dirty` callee outside its own component:
    /// off every cycle a function is a finite function of its callees, so
    /// modelling it before they settle only schedules it to be modelled again.
    pub(super) fn waits(&self, id: usize, dirty: &BTreeSet<usize>) -> bool {
        let own = self.component_of.get(id);
        self.edges.get(id).is_some_and(|callees| {
            callees
                .iter()
                .any(|callee| dirty.contains(callee) && self.component_of.get(*callee) != own)
        })
    }
}

fn is_cycle(edges: &[Vec<usize>], component: &[usize]) -> bool {
    match component {
        [only] => refers_to(edges, *only, *only),
        _ => true,
    }
}

fn reference_edges(index: &Index<'_>) -> Vec<Vec<usize>> {
    let edges = index.functions.iter().map(|function| {
        let mut callees = Vec::new();
        named_references(function.body, &mut |name| {
            callees.extend(index.resolve(&function.scope, name));
        });
        callees
    });
    edges.collect()
}

fn refers_to(edges: &[Vec<usize>], from: usize, to: usize) -> bool {
    edges.get(from).is_some_and(|callees| callees.contains(&to))
}

/// Kosaraju: finish order on the reference graph, then one component per
/// unassigned root on the reversed graph in reverse finish order.
fn components(edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let mut visited = vec![false; edges.len()];
    let mut order = Vec::with_capacity(edges.len());
    for root in 0..edges.len() {
        let _ = depth_first(edges, root, &mut visited, &mut order);
    }
    let reversed = reversed(edges);
    let mut assigned = vec![false; edges.len()];
    let roots = order.into_iter().rev();
    roots
        .map(|root| depth_first(&reversed, root, &mut assigned, &mut Vec::new()))
        .filter(|component| !component.is_empty())
        .collect()
}

/// Visit everything reachable from `root` not yet marked, appending each node
/// to `finished` once its successors are done; returns the nodes visited.
fn depth_first(
    edges: &[Vec<usize>],
    root: usize,
    marked: &mut [bool],
    finished: &mut Vec<usize>,
) -> Vec<usize> {
    let mut visited = Vec::new();
    let mut stack = Vec::new();
    enter(marked, root, &mut stack, &mut visited);
    while let Some(&(node, next)) = stack.last() {
        if let Some(top) = stack.last_mut() {
            top.1 = next + 1;
        }
        if let Some(child) = edges.get(node).and_then(|callees| callees.get(next)) {
            enter(marked, *child, &mut stack, &mut visited);
        } else {
            finished.push(node);
            let _ = stack.pop();
        }
    }
    visited
}

fn enter(
    marked: &mut [bool],
    id: usize,
    stack: &mut Vec<(usize, usize)>,
    visited: &mut Vec<usize>,
) {
    if mark(marked, id) {
        stack.push((id, 0));
        visited.push(id);
    }
}

fn mark(marked: &mut [bool], id: usize) -> bool {
    marked
        .get_mut(id)
        .is_some_and(|seen| !std::mem::replace(seen, true))
}

fn reversed(edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let mut reversed = vec![Vec::new(); edges.len()];
    for (from, callees) in edges.iter().enumerate() {
        for to in callees {
            if let Some(callers) = reversed.get_mut(*to) {
                callers.push(from);
            }
        }
    }
    reversed
}

#[cfg(test)]
#[path = "recursion_tests.rs"]
mod tests;
