//! The typing environment and the two operations that make HM polymorphic:
//! `instantiate` (fresh-rename a scheme's bound vars at each use) and
//! `generalize` (quantify a let-binding over the vars not free in the
//! environment).

use crate::ctx::InferCtx;
use crate::ty::{Scheme, Type, VarId};
use std::collections::{BTreeSet, HashMap, HashSet};

/// Maps names to their type schemes. Cloned to form child scopes (lambda
/// bodies, match arms) — value semantics, so child bindings never leak out.
#[derive(Debug, Clone, Default)]
pub struct TypeEnv {
    vars: HashMap<String, Scheme>,
    /// Names declared `mut` — the only bindings handler-arm assignment may target.
    mutables: HashSet<String>,
}

impl TypeEnv {
    pub fn new() -> TypeEnv {
        TypeEnv::default()
    }

    pub fn get(&self, name: &str) -> Option<&Scheme> {
        self.vars.get(name)
    }

    pub fn insert(&mut self, name: impl Into<String>, scheme: Scheme) {
        let name = name.into();
        // A fresh binding shadows any outer `mut` of the same name.
        let _ = self.mutables.remove(&name);
        let _ = self.vars.insert(name, scheme);
    }

    /// Bind a `mut` declaration — the one binding form handler arms may assign.
    pub fn insert_mutable(&mut self, name: impl Into<String>, scheme: Scheme) {
        let name = name.into();
        let _ = self.vars.insert(name.clone(), scheme);
        let _ = self.mutables.insert(name);
    }

    pub fn is_mutable(&self, name: &str) -> bool {
        self.mutables.contains(name)
    }

    /// The currently bound names. Snapshotted on the freshly built builtin
    /// environment to detect redefinition of built-in functions.
    pub fn bound_names(&self) -> HashSet<String> {
        self.vars.keys().cloned().collect()
    }

    pub fn remove(&mut self, name: &str) {
        let _ = self.vars.remove(name);
    }

    /// A fresh child scope (a clone — bindings added to the child don't leak).
    pub fn child(&self) -> TypeEnv {
        self.clone()
    }

    /// The free variables of the whole environment — the vars `generalize` must
    /// *not* quantify, because an outer scope may still constrain them.
    pub fn free_vars(&self, ctx: &mut InferCtx) -> BTreeSet<VarId> {
        let mut out = BTreeSet::new();
        for scheme in self.vars.values() {
            let mut fv = BTreeSet::new();
            ctx.free_vars(&scheme.ty, &mut fv);
            for q in &scheme.vars {
                let _ = fv.remove(q);
            }
            out.extend(fv);
        }
        out
    }
}

/// Instantiate a scheme: replace each quantified variable with a fresh one.
pub fn instantiate(ctx: &mut InferCtx, scheme: &Scheme) -> Type {
    if scheme.vars.is_empty() {
        return scheme.ty.clone();
    }
    let map: HashMap<VarId, Type> = scheme.vars.iter().map(|v| (*v, ctx.fresh())).collect();
    subst_vars(&scheme.ty, &map)
}

/// Generalize a type over the variables free in it but not in the environment.
pub fn generalize(ctx: &mut InferCtx, env: &TypeEnv, ty: &Type) -> Scheme {
    let ty = ctx.apply(ty);
    let env_fv = env.free_vars(ctx);
    let mut ty_fv = BTreeSet::new();
    ctx.free_vars(&ty, &mut ty_fv);
    let vars: Vec<VarId> = ty_fv.difference(&env_fv).copied().collect();
    Scheme { vars, ty }
}

fn subst_vars(t: &Type, map: &HashMap<VarId, Type>) -> Type {
    match t {
        Type::Var(v) => map.get(v).cloned().unwrap_or_else(|| t.clone()),
        Type::Con { name, args } => Type::Con {
            name: name.clone(),
            args: args.iter().map(|a| subst_vars(a, map)).collect(),
        },
        Type::Fun { params, ret } => Type::Fun {
            params: params.iter().map(|p| subst_vars(p, map)).collect(),
            ret: Box::new(subst_vars(ret, map)),
        },
        Type::Record { name, fields } => Type::Record {
            name: name.clone(),
            fields: fields
                .iter()
                .map(|(k, v)| (k.clone(), subst_vars(v, map)))
                .collect(),
        },
        Type::Union { name, variants } => Type::Union {
            name: name.clone(),
            variants: variants.iter().map(|v| subst_vars(v, map)).collect(),
        },
    }
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    reason = "test assertions: an out-of-bounds index is a test failure, not a production panic"
)]
mod tests {
    use super::*;
    use crate::unify::unify;

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
    fn generalize_skips_env_bound_vars() {
        let mut ctx = InferCtx::new();
        let mut env = TypeEnv::new();
        // An outer binding pins t0, so a let over (t0 -> t1) may only quantify t1.
        env.insert("outer", Scheme::mono(Type::Var(0)));
        let scheme = generalize(&mut ctx, &env, &Type::fun(vec![Type::Var(0)], Type::Var(1)));
        assert_eq!(scheme.vars, vec![1]);
    }
}
