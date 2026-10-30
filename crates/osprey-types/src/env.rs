//! The typing environment and the two operations that make HM polymorphic:
//! `instantiate` (fresh-rename a scheme's bound vars at each use) and
//! `generalize` (quantify a let-binding over the vars not free in the
//! environment).

use crate::ctx::InferCtx;
use crate::ty::{Scheme, Type, VarId};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

mod builtin_bindings;

/// One call's signature, deferred obligations, and ordered declared binders.
pub(crate) struct AppliedSignature {
    pub ty: Type,
    pub obligations: Vec<(String, Type)>,
    pub params: Vec<Type>,
    pub bindings: HashMap<VarId, Type>,
}

/// One scope's own bindings. A `None` or `false` entry records that this scope
/// removed or shadowed a name, so a lookup stops there instead of reaching an
/// outer scope's binding.
#[derive(Debug, Clone, Default)]
struct Frame {
    vars: HashMap<String, Option<Scheme>>,
    /// Ordered declaration binders, cleared whenever a value shadows a name.
    type_params: HashMap<String, Option<Vec<Type>>>,
    /// Names declared `mut` — the only bindings handler-arm assignment may target.
    mutables: HashMap<String, bool>,
    /// Builtin identity and runtime obligations, cleared by shadowing.
    builtins: HashMap<String, Option<builtin_bindings::Builtin>>,
}

/// Maps names to their type schemes, with value semantics: bindings added to a
/// child scope (lambda bodies, match arms) never leak out, and a scope never
/// sees what its parent binds after the child was made.
///
/// A scope shares its parent instead of copying it: inference opens a child
/// for every lambda, arm and block, and copying an environment holding every
/// builtin and top-level scheme made each one cost the whole environment.
/// A scope writes only its own frame.
#[derive(Debug, Clone, Default)]
pub(crate) struct TypeEnv {
    local: Arc<Frame>,
    outer: Option<Arc<TypeEnv>>,
    depth: usize,
}

/// Past this many nested scopes a child is flattened, so lookups stay short
/// however a pass chains them.
const MAX_DEPTH: usize = 64;

impl TypeEnv {
    pub(crate) fn new() -> TypeEnv {
        TypeEnv::default()
    }

    /// Every frame, innermost first.
    fn frames(&self) -> impl Iterator<Item = &Frame> {
        std::iter::successors(Some(self), |env| env.outer.as_deref()).map(|env| &*env.local)
    }

    /// The innermost frame's entry for a name, whatever it records.
    fn lookup<'a, T>(&'a self, pick: impl Fn(&'a Frame) -> Option<&'a T>) -> Option<&'a T> {
        self.frames().find_map(pick)
    }

    /// This scope's own frame, for writing. A frame another scope still shares
    /// is not copied: this scope becomes a child of its own snapshot instead,
    /// so the write costs one empty frame however large the shared one is. A
    /// scope that many such writes have deepened is flattened once, here, so
    /// the scopes opened from it stay shallow.
    fn frame(&mut self) -> &mut Frame {
        if Arc::get_mut(&mut self.local).is_none() {
            *self = if self.depth < MAX_DEPTH {
                self.child()
            } else {
                self.flattened()
            };
        }
        Arc::make_mut(&mut self.local)
    }

    pub(crate) fn get(&self, name: &str) -> Option<&Scheme> {
        self.lookup(|frame| frame.vars.get(name))
            .and_then(Option::as_ref)
    }

    fn type_params(&self, name: &str) -> Option<&Vec<Type>> {
        self.lookup(|frame| frame.type_params.get(name))
            .and_then(Option::as_ref)
    }

    pub(crate) fn insert(&mut self, name: impl Into<String>, scheme: Scheme) {
        let name = name.into();
        // A fresh binding shadows any outer `mut` of the same name.
        if self.is_mutable(&name) {
            let _ = self.frame().mutables.insert(name.clone(), false);
        }
        self.shadow_declaration(&name);
        let _ = self.frame().vars.insert(name, Some(scheme));
    }

    /// Bind a `mut` declaration — the one binding form handler arms may assign.
    pub(crate) fn insert_mutable(&mut self, name: impl Into<String>, scheme: Scheme) {
        let name = name.into();
        self.shadow_declaration(&name);
        let frame = self.frame();
        let _ = frame.vars.insert(name.clone(), Some(scheme));
        let _ = frame.mutables.insert(name, true);
    }

    pub(crate) fn is_mutable(&self, name: &str) -> bool {
        self.lookup(|frame| frame.mutables.get(name))
            .is_some_and(|mutable| *mutable)
    }

    /// The currently bound names. Snapshotted on the freshly built builtin
    /// environment to detect redefinition of built-in functions.
    pub(crate) fn bound_names(&self) -> HashSet<String> {
        let mut decided = HashSet::new();
        let mut bound = HashSet::new();
        for frame in self.frames() {
            for (name, scheme) in &frame.vars {
                if decided.insert(name.as_str()) && scheme.is_some() {
                    let _ = bound.insert(name.clone());
                }
            }
        }
        bound
    }

    pub(crate) fn remove(&mut self, name: &str) {
        self.shadow_declaration(name);
        if self.get(name).is_some() {
            let _ = self.frame().vars.insert(name.to_owned(), None);
        }
    }

    /// Attach declaration-site binders to this exact lexical binding.
    pub(crate) fn declare_type_params(&mut self, name: &str, params: Vec<Type>) {
        let _ = self
            .frame()
            .type_params
            .insert(name.to_owned(), Some(params));
    }

    /// The declaration-site binders exactly as declared — NOT re-instantiated
    /// against the current scheme. A body check quantifies the very variables
    /// the signature exported, which a substituted copy would orphan.
    pub(crate) fn declared_params(&self, name: &str) -> Vec<Type> {
        self.type_params(name).cloned().unwrap_or_default()
    }

    /// Instantiate the signature and its declared binders with one substitution.
    pub(crate) fn applied(&self, ctx: &mut InferCtx, name: &str) -> Option<AppliedSignature> {
        let scheme = self.get(name)?;
        let map = scheme.vars.iter().map(|v| (*v, ctx.fresh())).collect();
        let params = self
            .type_params(name)
            .into_iter()
            .flatten()
            .map(|ty| subst_vars(&ctx.apply(ty), &map))
            .collect();
        let obligations = subst_obligations(&scheme.obligations, &map);
        Some(AppliedSignature {
            ty: subst_vars(&scheme.ty, &map),
            obligations,
            params,
            bindings: map,
        })
    }

    /// A fresh child scope: bindings added to the child don't leak.
    pub(crate) fn child(&self) -> TypeEnv {
        let outer = if self.depth < MAX_DEPTH {
            self.clone()
        } else {
            self.flattened()
        };
        TypeEnv {
            local: Arc::default(),
            depth: outer.depth + 1,
            outer: Some(Arc::new(outer)),
        }
    }

    /// One frame holding exactly what this scope sees.
    fn flattened(&self) -> TypeEnv {
        let mut merged = Frame::default();
        for frame in self.frames() {
            merge_missing(&mut merged.vars, &frame.vars);
            merge_missing(&mut merged.type_params, &frame.type_params);
            merge_missing(&mut merged.mutables, &frame.mutables);
            merge_missing(&mut merged.builtins, &frame.builtins);
        }
        TypeEnv {
            local: Arc::new(merged),
            outer: None,
            depth: 0,
        }
    }

    /// The free variables of the whole environment — the vars `generalize` must
    /// *not* quantify, because an outer scope may still constrain them. A
    /// binding an inner scope shadows or removed is not visible, so it does
    /// not count.
    pub(crate) fn free_vars(&self, ctx: &mut InferCtx) -> BTreeSet<VarId> {
        let mut out = BTreeSet::new();
        let mut decided: HashSet<&str> = HashSet::new();
        let mut frames = self.frames().peekable();
        while let Some(frame) = frames.next() {
            let innermost_decides = frames.peek().is_some();
            for (name, scheme) in &frame.vars {
                let visible = if innermost_decides {
                    decided.insert(name.as_str())
                } else {
                    !decided.contains(name.as_str())
                };
                if let (true, Some(scheme)) = (visible, scheme) {
                    scheme_free_vars(ctx, scheme, &mut out);
                }
            }
        }
        out
    }
}

/// A scheme's free variables: those of its type and obligations it does not
/// quantify.
fn scheme_free_vars(ctx: &mut InferCtx, scheme: &Scheme, out: &mut BTreeSet<VarId>) {
    #[cfg(test)]
    tests::record_environment_visit(scheme);
    let mut fv = BTreeSet::new();
    ctx.free_vars(&scheme.ty, &mut fv);
    for (_, obligation) in &scheme.obligations {
        ctx.free_vars(obligation, &mut fv);
    }
    for q in &scheme.vars {
        let _ = fv.remove(q);
    }
    out.extend(fv);
}

/// Keep the first visible binding, including an explicit shadowing marker.
fn merge_missing<T: Clone>(target: &mut HashMap<String, T>, source: &HashMap<String, T>) {
    for (name, value) in source {
        let _ = target.entry(name.clone()).or_insert_with(|| value.clone());
    }
}

/// A scheme's built-in obligations restated against the substitution `map` —
/// the same rewrite every instantiation site performs on its fresh variables.
fn subst_obligations(
    obligations: &[(String, Type)],
    map: &HashMap<VarId, Type>,
) -> Vec<(String, Type)> {
    obligations
        .iter()
        .map(|(name, ty)| (name.clone(), subst_vars(ty, map)))
        .collect()
}

/// Instantiate a scheme: replace each quantified variable with a fresh one.
pub(crate) fn instantiate(ctx: &mut InferCtx, scheme: &Scheme) -> Type {
    instantiated(ctx, scheme).0
}

/// [`instantiate`], also re-stating the scheme's built-in obligations against
/// this site's fresh variables. The caller records them so the site is checked
/// against the concrete types it supplies — a generalized wrapper cannot
/// launder an ineligible element type past a constrained built-in
/// ([`Scheme::obligations`]).
pub(crate) fn instantiated(ctx: &mut InferCtx, scheme: &Scheme) -> (Type, Vec<(String, Type)>) {
    if scheme.vars.is_empty() {
        return (scheme.ty.clone(), scheme.obligations.clone());
    }
    let map: HashMap<VarId, Type> = scheme.vars.iter().map(|v| (*v, ctx.fresh())).collect();
    let obligations = subst_obligations(&scheme.obligations, &map);
    (subst_vars(&scheme.ty, &map), obligations)
}

/// Generalize a type over the variables free in it but not in the environment.
pub(crate) fn generalize(ctx: &mut InferCtx, env: &TypeEnv, ty: &Type) -> Scheme {
    let ty = ctx.apply(ty);
    let mut ty_fv = BTreeSet::new();
    ctx.free_vars(&ty, &mut ty_fv);
    let vars = if ty_fv.is_empty() {
        Vec::new()
    } else {
        let env_fv = env.free_vars(ctx);
        ty_fv.difference(&env_fv).copied().collect()
    };
    Scheme {
        vars,
        ty,
        obligations: Vec::new(),
    }
}

pub(crate) fn subst_vars(t: &Type, map: &HashMap<VarId, Type>) -> Type {
    crate::ty::map_type_vars(t, &mut |v| map.get(&v).cloned().unwrap_or(Type::Var(v)))
}
#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    reason = "test assertions: an out-of-bounds index is a test failure, not a production panic"
)]
pub(crate) mod tests {
    use super::*;
    use crate::unify::unify;

    std::thread_local! {
        static ENVIRONMENT_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
        pub(crate) static MARKER_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    pub(crate) const ENVIRONMENT_MARKER: &str = "EnvScanMarker";

    pub(super) fn record_environment_visit(scheme: &Scheme) {
        ENVIRONMENT_VISITS.with(|visits| visits.set(visits.get() + 1));
        if matches!(&scheme.ty, Type::Record { name, .. } | Type::Con { name, .. }
            if name == ENVIRONMENT_MARKER)
        {
            MARKER_VISITS.with(|visits| visits.set(visits.get() + 1));
        }
    }

    #[test]
    fn closed_types_generalize_without_scanning_unrelated_bindings() {
        let mut ctx = InferCtx::new();
        let mut env = TypeEnv::new();
        for index in 0..512 {
            env.insert(format!("global{index}"), Scheme::mono(Type::int()));
        }
        let ty = Type::fun(vec![Type::int()], Type::string());
        ENVIRONMENT_VISITS.with(|visits| visits.set(0));
        let scheme = generalize(&mut ctx, &env, &ty);
        assert_eq!(scheme.ty, ty);
        assert_eq!(scheme.vars, Vec::new());
        assert_eq!(scheme.obligations, Vec::new());
        let visits = ENVIRONMENT_VISITS.with(std::cell::Cell::get);
        assert_eq!(
            visits, 0,
            "a closed type cannot quantify variables from unrelated bindings"
        );
    }

    #[test]
    fn instantiation_gives_fresh_independent_vars() {
        // id : forall t0. (t0) -> t0
        let mut ctx = InferCtx::new();
        let scheme = Scheme::poly(vec![0], Type::fun(vec![Type::Var(0)], Type::Var(0)));

        // Two uses must not share a variable: id(1) and id("x") both type-check.
        let a = instantiate(&mut ctx, &scheme);
        let b = instantiate(&mut ctx, &scheme);
        if let (Type::Fun { params: pa, .. }, Type::Fun { params: pb, .. }) = (&a, &b) {
            unify(&mut ctx, &pa[0], &Type::int()).unwrap();
            unify(&mut ctx, &pb[0], &Type::string()).unwrap();
        } else {
            panic!("expected function types");
        }
    }

    #[test]
    fn instantiate_substitutes_into_records_and_unions() {
        let mut ctx = InferCtx::new();
        // forall t0. Union "U" { Record "R" { x: t0 }, t0 }
        let body = Type::Union {
            name: "U".into(),
            variants: vec![
                Type::Record {
                    name: "R".into(),
                    fields: [("x".to_string(), Type::Var(0))].into_iter().collect(),
                },
                Type::Var(0),
            ],
        };
        let inst = instantiate(&mut ctx, &Scheme::poly(vec![0], body));
        // The single quantified var is renamed to one consistent fresh var that
        // appears in both the record field and the bare variant.
        if let Type::Union { variants, .. } = &inst {
            assert!(matches!(variants[1], Type::Var(_)));
            if let Type::Record { fields, .. } = &variants[0] {
                assert_eq!(fields["x"], variants[1]);
            } else {
                panic!("expected record variant");
            }
        } else {
            panic!("expected union");
        }
    }

    #[test]
    fn a_child_scope_neither_leaks_nor_sees_later_parent_bindings() {
        let mut parent = TypeEnv::new();
        parent.insert_mutable("count", Scheme::mono(Type::int()));
        parent.declare_type_params("count", vec![Type::Var(7)]);
        parent.mark_runtime_builtin("count");
        let mut child = parent.child();
        child.insert("count", Scheme::mono(Type::string()));
        child.insert("local", Scheme::mono(Type::bool()));
        parent.insert("later", Scheme::mono(Type::int()));
        assert_eq!(
            child.get("count").map(|s| s.ty.clone()),
            Some(Type::string())
        );
        assert!(!child.is_mutable("count") && !child.is_runtime_builtin("count"));
        assert!(child.type_params("count").is_none() && child.get("later").is_none());
        assert_eq!(parent.get("count").map(|s| s.ty.clone()), Some(Type::int()));
        assert!(parent.is_mutable("count") && parent.is_runtime_builtin("count"));
        assert!(parent.type_params("count").is_some() && parent.get("local").is_none());
    }

    #[test]
    fn removing_or_shadowing_hides_an_outer_binding_from_every_query() {
        let mut ctx = InferCtx::new();
        let mut parent = TypeEnv::new();
        parent.insert("pinned", Scheme::mono(Type::Var(0)));
        parent.insert("gone", Scheme::mono(Type::Var(1)));
        let mut child = parent.child();
        child.insert("pinned", Scheme::mono(Type::int()));
        child.remove("gone");
        assert!(child.get("gone").is_none() && parent.get("gone").is_some());
        assert_eq!(
            child.bound_names(),
            ["pinned".to_owned()].into_iter().collect()
        );
        assert!(child.free_vars(&mut ctx).is_empty());
        assert_eq!(parent.free_vars(&mut ctx), [0, 1].into_iter().collect());
    }

    #[test]
    fn deep_nesting_flattens_without_changing_what_a_scope_sees() {
        let mut env = TypeEnv::new();
        env.insert("base", Scheme::mono(Type::int()));
        for depth in 0..(MAX_DEPTH * 2) {
            env = env.child();
            env.insert(format!("v{depth}"), Scheme::mono(Type::bool()));
        }
        assert!(env.depth <= MAX_DEPTH + 1);
        assert!(env.get("base").is_some() && env.get("v0").is_some());
        assert_eq!(env.bound_names().len(), MAX_DEPTH * 2 + 1);
    }

    #[test]
    fn a_scope_written_under_many_snapshots_stays_shallow() {
        let mut env = TypeEnv::new();
        let mut snapshots = Vec::new();
        for index in 0..(MAX_DEPTH * 3) {
            snapshots.push(env.clone());
            env.insert(format!("v{index}"), Scheme::mono(Type::int()));
        }
        assert!(env.depth <= MAX_DEPTH && env.child().depth <= MAX_DEPTH + 1);
        assert_eq!(env.bound_names().len(), MAX_DEPTH * 3);
        assert!(snapshots
            .iter()
            .enumerate()
            .all(|(i, s)| s.bound_names().len() == i));
    }

    #[test]
    fn generalize_skips_env_bound_vars() {
        let mut ctx = InferCtx::new();
        let mut env = TypeEnv::new();
        // An outer binding pins t0, so a let over (t0 -> t1) may only quantify t1.
        env.insert("outer", Scheme::mono(Type::Var(0)));
        let scheme = generalize(&mut ctx, &env, &Type::fun(vec![Type::Var(0)], Type::Var(1)));
        assert_eq!(scheme.vars, vec![1]);
    }
}
