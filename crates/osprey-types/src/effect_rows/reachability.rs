//! Effect row reachability.
use super::{
    expression_name, Analyzer, BTreeSet, CallableEnv, Expr, HashSet, Index, Stmt, Summary,
};

/// A summary's requirements as bare `(effect, operation)` pairs. Multiplicity
/// is a property of the DECLARATION, so the instance arguments a row entry
/// carries are irrelevant to every rule in that axis. Implements [MULTI-REPLAY-CHECK].
pub(super) fn operation_pairs(summary: &Summary) -> BTreeSet<(String, String)> {
    summary
        .required
        .iter()
        .map(|requirement| (requirement.effect.clone(), requirement.operation.clone()))
        .collect()
}

/// The first operation of `effect` that the handled expression performs inside
/// a `spawn` — through the functions it calls as well as in its own syntax,
/// because `spawn ask(a)` is almost always one call away from the `handle`.
///
/// A resuming handler serializes one suspend-to-resume round trip per perform
/// ([EFFECTS-FIBER-PERFORM]); a second resumption of a continuation spanning a
/// spawned fiber has no order to belong to. Implements [MULTI-REPLAY-FIBER].
pub(super) fn fibered_operation(
    analyzer: &Analyzer<'_>,
    effect: &str,
    body: &Expr,
    scope: &[String],
    env: &CallableEnv,
) -> Option<String> {
    let mut operations = BTreeSet::new();
    for (region, region_scope) in reachable_bodies(analyzer.index, body, scope) {
        spawned_bodies(region, &mut |spawned| {
            operations.extend(
                operation_pairs(&analyzer.expression(spawned, region_scope, env))
                    .into_iter()
                    .filter(|(row_effect, _)| row_effect == effect)
                    .map(|(_, operation)| operation),
            );
        });
    }
    operations.into_iter().next()
}

/// The handled expression and the body of every function it can reach through
/// a statically named call, each paired with the scope its names resolve in.
///
/// Conservative on purpose: any mention of a function's name counts as reaching
/// it, since a name passed as a value is a call the row engine must assume
/// happens. Over-reaching costs a rejection the plan already fails closed on;
/// under-reaching would miss the fiber boundary this rule exists to find.
pub(super) fn reachable_bodies<'a>(
    index: &'a Index<'_>,
    body: &'a Expr,
    scope: &'a [String],
) -> Vec<(&'a Expr, &'a [String])> {
    let mut regions: Vec<(&Expr, &[String])> = vec![(body, scope)];
    let mut visited = HashSet::new();
    let mut next = 0;
    while let Some((region, region_scope)) = regions.get(next).copied() {
        next += 1;
        let mut names = Vec::new();
        named_references(region, &mut |name| names.push(name.to_string()));
        for id in names
            .iter()
            .filter_map(|name| index.resolve(region_scope, name))
        {
            if visited.insert(id) {
                if let Some(function) = index.functions.get(id) {
                    regions.push((function.body, &function.scope));
                }
            }
        }
    }
    regions
}

/// Apply `visit` to every identifier and path mentioned in `expression`.
pub(super) fn named_references<'a>(expression: &'a Expr, visit: &mut impl FnMut(&'a str)) {
    if let Some(name) = expression_name(expression) {
        visit(name);
    }
    walk_children(expression, |child| named_references(child, visit));
}

/// Apply `visit` to the body of every `spawn` inside `expression`, including
/// those below a nested handler — a fiber spawned there still crosses this
/// region's continuation.
pub(super) fn spawned_bodies<'a>(expression: &'a Expr, visit: &mut impl FnMut(&'a Expr)) {
    if let Expr::Spawn(spawned) = expression {
        visit(spawned);
    }
    walk_children(expression, |child| spawned_bodies(child, visit));
}

pub(super) fn walk_children<'a>(expression: &'a Expr, mut visit: impl FnMut(&'a Expr)) {
    osprey_ast::AstNode::Expression(expression).for_each_child(|node| match node {
        osprey_ast::AstNode::Expression(child) => visit(child),
        osprey_ast::AstNode::Statement(
            Stmt::Let { value, .. } | Stmt::Assignment { value, .. } | Stmt::Expr { value, .. },
        ) => visit(value),
        osprey_ast::AstNode::Statement(_) => {}
    });
}
