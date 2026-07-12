//! The inference context: fresh-variable supply plus the substitution that
//! backs unification, stored as an index-addressed arena (`Vec<Option<Type>>`)
//! — the textbook union-find layout. `prune` compresses paths as it resolves,
//! so a chain of variable bindings is walked once and then answers in one hop.

use crate::ty::{Type, VarId};
use osprey_ast::Variance;
use std::collections::{BTreeSet, HashMap};

/// Holds every type variable's binding. Variable ids are indices into `subst`.
#[derive(Debug, Default)]
pub struct InferCtx {
    subst: Vec<Option<Type>>,
    /// Type-constructor name → declared per-parameter variance, consulted by
    /// assignability so `Source<out T>` matches covariantly. Implements
    /// [TYPE-VARIANCE-ASSIGN].
    variances: HashMap<String, Vec<Variance>>,
}

impl InferCtx {
    /// Create an empty context with no allocated type variables.
    pub fn new() -> InferCtx {
        InferCtx::default()
    }

    /// Register a type constructor's declared per-parameter variance.
    pub fn set_variance(&mut self, name: impl Into<String>, variances: Vec<Variance>) {
        let _ = self.variances.insert(name.into(), variances);
    }

    /// The declared per-parameter variance of a type constructor, if any.
    #[must_use]
    pub fn variance_of(&self, name: &str) -> Option<&[Variance]> {
        self.variances.get(name).map(Vec::as_slice)
    }

    /// Allocate a fresh, unbound type variable.
    pub fn fresh(&mut self) -> Type {
        let id = VarId::try_from(self.subst.len()).unwrap_or(VarId::MAX);
        self.subst.push(None);
        Type::Var(id)
    }

    /// Follow a variable to its representative, compressing the path. Only the
    /// outermost variable is resolved — nested types are left intact (use
    /// [`InferCtx::apply`] for a deep walk).
    pub fn prune(&mut self, t: &Type) -> Type {
        if let Type::Var(id) = t {
            let idx = usize::try_from(*id).unwrap_or(usize::MAX);
            if let Some(bound) = self.subst.get(idx).and_then(Option::clone) {
                let pruned = self.prune(&bound);
                if let Some(slot) = self.subst.get_mut(idx) {
                    *slot = Some(pruned.clone());
                }
                return pruned;
            }
        }
        t.clone()
    }

    /// Bind a variable to a type. The caller guarantees the occurs-check passed.
    pub fn bind(&mut self, id: VarId, t: Type) {
        let idx = usize::try_from(id).unwrap_or(usize::MAX);
        if let Some(slot) = self.subst.get_mut(idx) {
            *slot = Some(t);
        }
    }

    /// The occurs check: does variable `id` appear anywhere in `t`? Prevents
    /// the construction of infinite types like `t0 ~ List<t0>`.
    pub fn occurs(&mut self, id: VarId, t: &Type) -> bool {
        let t = self.prune(t);
        match &t {
            Type::Var(v) => *v == id,
            Type::Fun { params, ret } => {
                params.iter().any(|p| self.occurs(id, p)) || self.occurs(id, ret)
            }
            Type::Con { args, .. } => args.iter().any(|a| self.occurs(id, a)),
            Type::Record { fields, .. } => fields.values().any(|v| self.occurs(id, v)),
            Type::Union { variants, .. } => variants.iter().any(|v| self.occurs(id, v)),
        }
    }

    /// Fully resolve `t` against the current substitution. The occurs-check
    /// keeps the substitution acyclic, so this terminates.
    pub fn apply(&mut self, t: &Type) -> Type {
        let t = self.prune(t);
        match &t {
            Type::Var(_) => t,
            Type::Con { name, args } => Type::Con {
                name: name.clone(),
                args: args.iter().map(|a| self.apply(a)).collect(),
            },
            Type::Fun { params, ret } => Type::Fun {
                params: params.iter().map(|p| self.apply(p)).collect(),
                ret: Box::new(self.apply(ret)),
            },
            Type::Record { name, fields } => Type::Record {
                name: name.clone(),
                fields: fields
                    .iter()
                    .map(|(k, v)| (k.clone(), self.apply(v)))
                    .collect(),
            },
            Type::Union { name, variants } => Type::Union {
                name: name.clone(),
                variants: variants.iter().map(|v| self.apply(v)).collect(),
            },
        }
    }

    /// Collect the free (unbound) variables of `t` into `out`.
    pub fn free_vars(&mut self, t: &Type, out: &mut BTreeSet<VarId>) {
        let t = self.prune(t);
        match &t {
            Type::Var(v) => {
                let _ = out.insert(*v);
            }
            Type::Fun { params, ret } => {
                for p in params {
                    self.free_vars(p, out);
                }
                self.free_vars(ret, out);
            }
            Type::Con { args, .. } => {
                for a in args {
                    self.free_vars(a, out);
                }
            }
            Type::Record { fields, .. } => {
                for v in fields.values() {
                    self.free_vars(v, out);
                }
            }
            Type::Union { variants, .. } => {
                for v in variants {
                    self.free_vars(v, out);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_vars_are_distinct_and_unbound() {
        let mut c = InferCtx::new();
        let a = c.fresh();
        let b = c.fresh();
        assert_ne!(a, b);
        assert_eq!(c.prune(&a), a);
    }

    #[test]
    fn prune_follows_and_compresses_chains() {
        let mut c = InferCtx::new();
        let (a, b) = (c.fresh(), c.fresh());
        if let (Type::Var(ia), Type::Var(_ib)) = (&a, &b) {
            c.bind(*ia, b.clone());
            let bb = if let Type::Var(ib) = &b { *ib } else { 0 };
            c.bind(bb, Type::int());
        }
        assert_eq!(c.prune(&a), Type::int());
    }

    #[test]
    fn occurs_check_detects_cycles() {
        let mut c = InferCtx::new();
        let a = c.fresh();
        let id = if let Type::Var(id) = a { id } else { 0 };
        assert!(c.occurs(id, &Type::list(a.clone())));
        assert!(!c.occurs(id, &Type::list(Type::int())));
    }

    fn rec(field: Type) -> Type {
        Type::Record {
            name: "R".into(),
            fields: [("x".to_string(), field)].into_iter().collect(),
        }
    }
    fn uni(variant: Type) -> Type {
        Type::Union {
            name: "U".into(),
            variants: vec![variant],
        }
    }

    #[test]
    fn occurs_walks_records_unions_and_functions() {
        let mut c = InferCtx::new();
        let v = c.fresh();
        let id = if let Type::Var(id) = v { id } else { 0 };
        assert!(c.occurs(id, &rec(v.clone())));
        assert!(c.occurs(id, &uni(v.clone())));
        // The variable in a function's parameter / return is found too.
        assert!(c.occurs(id, &Type::fun(vec![v.clone()], Type::int())));
        assert!(c.occurs(id, &Type::fun(vec![Type::int()], v.clone())));
    }

    #[test]
    fn apply_and_free_vars_descend_records_and_unions() {
        let mut c = InferCtx::new();
        let v = c.fresh();
        let id = if let Type::Var(id) = v { id } else { 0 };
        c.bind(id, Type::int());
        // `apply` rebuilds records/unions with the binding resolved.
        assert_eq!(c.apply(&rec(v.clone())), rec(Type::int()));
        assert_eq!(c.apply(&uni(v.clone())), uni(Type::int()));
        // `free_vars` reaches into record fields and union variants.
        let w = c.fresh();
        let mut fv = BTreeSet::new();
        c.free_vars(&rec(w.clone()), &mut fv);
        c.free_vars(&uni(w.clone()), &mut fv);
        let wid = if let Type::Var(wid) = w { wid } else { 0 };
        assert!(fv.contains(&wid));
    }
}
