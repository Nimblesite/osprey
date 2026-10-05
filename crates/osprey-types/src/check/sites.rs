//! Type checking: sites.
use super::{HashMap, HashSet, InferCtx, Position, Type};

/// The code generator's erased view assigns every declared generic parameter
/// the uniform boxed type-variable representation.
pub(super) fn erased_type_params(type_params: &[String]) -> HashMap<String, Type> {
    type_params
        .iter()
        .enumerate()
        .map(|(index, parameter)| (parameter.clone(), Type::Var(erased_var(index))))
        .collect()
}
/// The variable id standing for the type parameter at `index`. Numbering the
/// erasure by POSITION is what lets the backend read a generic record's real
/// field types back out of an instantiation: `Envelope<Map<…>, int>.metadata`
/// is declared `U`, and only the parameter's position says that `U` is the
/// second argument. Implements [TYPE-GENERICS-DECL].
#[must_use]
pub fn erased_var(index: usize) -> u32 {
    u32::try_from(index).unwrap_or(u32::MAX)
}
/// Resolve one operation signature against the final substitution.
pub(super) fn resolve_op(ctx: &mut InferCtx, op: &crate::info::OpType) -> crate::info::OpType {
    crate::info::OpType {
        params: op.params.iter().map(|t| ctx.apply(t)).collect(),
        ret: ctx.apply(&op.ret),
        mode: op.mode,
    }
}
/// A type is resolved once it mentions no inference variable anywhere — the
/// exact complement of [`crate::ty::has_type_var`], which owns the traversal.
pub(super) fn type_is_resolved(ty: &Type) -> bool {
    !crate::ty::has_type_var(ty)
}
pub(super) fn collect_site_candidates<S: PartialEq>(
    entries: impl Iterator<Item = ((u32, u32), S)>,
) -> HashMap<(u32, u32), Vec<S>> {
    let mut out = HashMap::new();
    for (key, candidate) in entries {
        push_site_candidate(&mut out, key, candidate);
    }
    out
}
pub(super) fn push_site_candidate<S: PartialEq>(
    sites: &mut HashMap<(u32, u32), Vec<S>>,
    key: (u32, u32),
    candidate: S,
) {
    let candidates = sites.entry(key).or_default();
    if !candidates.contains(&candidate) {
        candidates.push(candidate);
    }
}
/// Collect position-keyed effect sites, DROPPING any key that appears with
/// two different resolutions. String-interpolation fragments are re-parsed
/// with fragment-relative positions, so two performs in different fragments
/// can share a `(line, column)` key — publishing either one would hand
/// codegen the wrong signature. Without an entry the backend degrades to the
/// unmangled, fully-boxed path, which fails loudly (unhandled effect) rather
/// than confusing types. Implements [EFFECTS-GENERIC-INSTANTIATION].
pub(super) fn dedupe_sites<S: PartialEq>(
    entries: impl Iterator<Item = ((u32, u32), S)>,
) -> HashMap<(u32, u32), S> {
    let mut out: HashMap<(u32, u32), S> = HashMap::new();
    let mut conflicted: HashSet<(u32, u32)> = HashSet::new();
    for (key, site) in entries {
        if conflicted.contains(&key) {
            continue;
        }
        match out.get(&key) {
            Some(existing) if *existing != site => {
                let _ = out.remove(&key);
                let _ = conflicted.insert(key);
            }
            _ => {
                let _ = out.insert(key, site);
            }
        }
    }
    out
}
/// Resolve a list of source-position-keyed types against the final
/// substitution, keying the published map by `(line, column)`.
pub(super) fn resolve_positioned(
    ctx: &mut InferCtx,
    tys: &[(Position, Type)],
) -> HashMap<(u32, u32), Type> {
    tys.iter()
        .map(|(pos, ty)| ((pos.line, pos.column), ctx.apply(ty)))
        .collect()
}
