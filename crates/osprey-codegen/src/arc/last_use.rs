//! Scope-aware last-use drops.
use super::{release_entries, Codegen};
use osprey_ast::{Expr, Stmt};

/// Drop owners introduced in the current lexical scope when that scope's
/// continuation no longer references them (M6 last-use precision, TR Fig. 6).
/// Runs only at function ledger depth: statement/loop frames cannot establish
/// enclosing liveness. The lexical gate is equally important: a block inside
/// one match arm must not remove an outer owner from the global ledger, because
/// sibling arms bypass that path-local drop.
pub(crate) fn release_dead_after<S: std::borrow::Borrow<Stmt>>(
    cg: &mut Codegen,
    rest: &[S],
    value: Option<&Expr>,
) {
    if cg.arc.frames.len() != 1 {
        return;
    }
    let mut live = std::collections::BTreeSet::new();
    osprey_ast::freevars::free_idents_of_stmts(rest, value, &mut live);
    // An inline lambda the continuation can still apply keeps what it closes
    // over alive, though the continuation never names those captures.
    live.extend(crate::closure::reach(cg, &live));
    live.extend(crate::iter::stage_aliases(cg));
    release_dead(cg, &live);
}

fn release_dead(cg: &mut Codegen, live: &std::collections::BTreeSet<String>) {
    let mut dead = Vec::new();
    let scope = cg.scope_id();
    if let Some(frame) = cg.arc.frames.first_mut() {
        let mut kept = Vec::new();
        for e in frame.drain(..) {
            match &e.name {
                Some(n) if e.binding_scope == scope && !live.contains(n) => dead.push(e),
                _ => kept.push(e),
            }
        }
        *frame = kept;
    }
    release_entries(cg, &dead);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arc::{bind_owned, own};
    use crate::llty::{LType, Value};

    fn owner_names(cg: &Codegen) -> Vec<&str> {
        cg.arc
            .frames
            .first()
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.name.as_deref())
            .collect()
    }

    fn bind_owner(cg: &mut Codegen, name: &str) {
        let owner = Value::new(format!("%{name}"), LType::Ptr);
        own(cg, &owner);
        bind_owned(cg, name, &owner);
    }

    #[test]
    fn last_use_only_drains_the_current_lexical_scope() {
        let mut cg = Codegen::new();
        cg.begin_function("scope_arc", None);
        bind_owner(&mut cg, "outer");
        cg.push_scope();
        bind_owner(&mut cg, "inner");

        release_dead(&mut cg, &std::collections::BTreeSet::new());

        assert_eq!(owner_names(&cg), ["outer"]);
    }

    #[test]
    fn sibling_scope_cannot_drain_another_arms_owner() {
        let mut cg = Codegen::new();
        cg.begin_function("branch_arc", None);
        cg.push_scope();
        bind_owner(&mut cg, "arm_owner");
        cg.pop_scope();
        cg.push_scope();

        release_dead(&mut cg, &std::collections::BTreeSet::new());

        assert_eq!(owner_names(&cg), ["arm_owner"]);
    }
}
