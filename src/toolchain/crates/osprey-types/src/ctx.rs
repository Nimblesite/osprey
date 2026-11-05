//! The inference context: fresh-variable supply plus the substitution that
//! backs unification, stored as an index-addressed arena (`Vec<Option<Type>>`)
//! — the textbook union-find layout. `prune` compresses paths as it resolves,
//! so a chain of variable bindings is walked once and then answers in one hop.

use crate::error::TypeError;
use crate::numeric::Numeric;
use crate::ty::{Type, VarId};
use osprey_ast::{TypeExpr, Variance};
use std::collections::{BTreeSet, HashMap};

type RecordTemplate = (Vec<String>, Vec<(String, String)>);

/// Holds every type variable's binding. Variable ids are indices into `subst`.
#[derive(Debug, Default)]
pub(crate) struct InferCtx {
    subst: Vec<Option<Type>>,
    /// Type-constructor name → declared per-parameter variance, consulted by
    /// assignability so `Source<out T>` matches covariantly. Implements
    /// [TYPE-VARIANCE-ASSIGN].
    variances: HashMap<String, Vec<Variance>>,
    /// Nominal record layouts, before instantiating their declaration binders.
    records: HashMap<String, RecordTemplate>,
    /// Opaque alias → its binders and representation [MODULES-OPAQUE-TYPES].
    equations: HashMap<String, (Vec<String>, TypeExpr)>,
    /// The declaration being checked. An opaque alias is its representation
    /// exactly where this sits inside the alias's owning module.
    site: String,
    /// Open numeric literals: a variable a literal's type rests on, with the
    /// type it defaults to. Implements [TYPE-NUMERIC-LITERAL].
    literals: HashMap<VarId, Numeric>,
}

impl InferCtx {
    /// Create an empty context with no allocated type variables.
    pub(crate) fn new() -> InferCtx {
        InferCtx::default()
    }

    /// Register a type constructor's declared per-parameter variance.
    pub(crate) fn set_variance(&mut self, name: impl Into<String>, variances: Vec<Variance>) {
        let _ = self.variances.insert(name.into(), variances);
    }

    /// The declared per-parameter variance of a type constructor, if any.
    #[must_use]
    pub(crate) fn variance_of(&self, name: &str) -> Option<&[Variance]> {
        self.variances.get(name).map(Vec::as_slice)
    }

    /// Register a record's generic field template [TYPE-GENERICS-DECL].
    pub(crate) fn set_record(
        &mut self,
        name: String,
        params: Vec<String>,
        fields: Vec<(String, String)>,
    ) {
        let _ = self.records.insert(name, (params, fields));
    }

    /// Resolve a nominal record application to its instantiated fields.
    pub(crate) fn record_fields(
        &self,
        name: &str,
        args: &[Type],
    ) -> Option<std::collections::BTreeMap<String, Type>> {
        let (params, fields) = self.records.get(name)?;
        if params.len() != args.len() {
            return None;
        }
        let binder = params.iter().cloned().zip(args.iter().cloned()).collect();
        Some(
            fields
                .iter()
                .map(|(field, ty)| {
                    (
                        field.clone(),
                        crate::convert::type_name_to_type(ty, &binder),
                    )
                })
                .collect(),
        )
    }

    pub(crate) fn record_preserves_parameters(&self, name: &str) -> bool {
        let Some((parameters, _)) = self.records.get(name) else {
            return false;
        };
        let arguments: Option<Vec<_>> = (0..parameters.len())
            .map(|index| u32::try_from(index).ok().map(Type::Var))
            .collect();
        let Some(arguments) = arguments else {
            return false;
        };
        let Some(fields) = self.record_fields(name, &arguments) else {
            return false;
        };
        crate::info::fields_carry_parameters(fields.values(), parameters.len())
    }

    /// The one record that declares `field`, instantiated at fresh type
    /// arguments; `None` when no record or several declare it.
    pub(crate) fn sole_record_declaring(&mut self, field: &str) -> Option<Type> {
        let mut owners = self
            .records
            .iter()
            .filter(|(_, (_, fields))| fields.iter().any(|(name, _)| name == field));
        let (name, (params, _)) = owners.next()?;
        if owners.next().is_some() {
            return None;
        }
        let (name, arity) = (name.clone(), params.len());
        let args: Vec<Type> = (0..arity).map(|_| self.fresh()).collect();
        let fields = self.record_fields(&name, &args)?;
        Some(if args.is_empty() {
            Type::Record { name, fields }
        } else {
            Type::con(name, args)
        })
    }

    /// Declare an opaque alias: a nominal type that only the declarations of
    /// its owning module may read through [MODULES-OPAQUE-TYPES].
    pub(crate) fn set_equation(&mut self, name: String, params: Vec<String>, alias: TypeExpr) {
        let _ = self.equations.insert(name, (params, alias));
    }

    /// Move to the declaration `site`, answering the one it replaces.
    pub(crate) fn enter(&mut self, site: String) -> String {
        std::mem::replace(&mut self.site, site)
    }

    /// Whether the current declaration reads through any opaque alias at all.
    pub(crate) fn sees_through_aliases(&self) -> bool {
        self.equations
            .keys()
            .any(|alias| osprey_ast::symbol::encloses(alias, &self.site))
    }

    /// What `t` stands for when it is an opaque alias `site` may read through.
    fn representation(&self, t: &Type, site: &str) -> Option<Type> {
        let Type::Con { name, args } = t else {
            return None;
        };
        let (params, alias) = self.equations.get(name)?;
        (params.len() == args.len() && osprey_ast::symbol::encloses(name, site)).then(|| {
            let binder = params.iter().cloned().zip(args.iter().cloned()).collect();
            crate::convert::type_expr_to_type(alias, &binder)
        })
    }

    /// `t` with every opaque alias `site` may read through replaced, outermost
    /// first, by its representation.
    #[must_use]
    pub(crate) fn exposed_at(&self, t: Type, site: &str) -> Type {
        match self.representation(&t, site) {
            Some(representation) => self.exposed_at(representation, site),
            None => t,
        }
    }

    /// [`Self::prune`], then [`Self::exposed_at`] the current declaration:
    /// what a rule that inspects a type's shape must look at.
    pub(crate) fn expose(&mut self, t: &Type) -> Type {
        let pruned = self.prune(t);
        self.exposed_at(pruned, &self.site)
    }

    /// The mismatch of `a` and `b`. When one is an opaque alias, the error
    /// names the module that keeps its representation: the author can read
    /// `type T = int` there and would otherwise learn only that `int` is not
    /// `T`.
    pub(crate) fn mismatch(&mut self, a: &Type, b: &Type) -> TypeError {
        let (a, b) = (&self.displayed(a), &self.displayed(b));
        let error = TypeError::mismatch(a, b);
        let sealed = [a, b].into_iter().find_map(|t| match t {
            Type::Con { name, .. } if self.equations.contains_key(name) => {
                osprey_ast::symbol::parent(name).map(|owner| (name, owner))
            }
            _ => None,
        });
        match sealed {
            Some((alias, owner)) => TypeError::new(format!(
                "{}; `{alias}` is opaque outside module `{owner}`",
                error.message
            )),
            None => error,
        }
    }

    /// Prune both sides of a unification and read through an opaque alias the
    /// current declaration may see through, unless the other side is that same
    /// alias. A variable is left to bind the alias itself, so a value keeps the
    /// abstract type its annotation gave it.
    pub(crate) fn seen_through(&mut self, a: &Type, b: &Type) -> (Type, Type) {
        let (a, b) = (self.prune(a), self.prune(b));
        let settled = matches!((&a, &b), (Type::Var(_), _) | (_, Type::Var(_)))
            || matches!((&a, &b), (Type::Con { name: left, .. }, Type::Con { name: right, .. }) if left == right);
        if settled || self.equations.is_empty() {
            return (a, b);
        }
        (
            self.exposed_at(a, &self.site),
            self.exposed_at(b, &self.site),
        )
    }

    /// A variable for a numeric literal whose context may still pick its type
    /// among `default`'s family ([TYPE-NUMERIC-LITERAL]).
    pub(crate) fn fresh_literal(&mut self, default: Numeric) -> Type {
        let var = self.fresh();
        if let Type::Var(id) = var {
            let _ = self.literals.insert(id, default);
        }
        var
    }

    /// The default of a variable that is an open numeric literal.
    pub(crate) fn literal_default(&self, id: VarId) -> Option<Numeric> {
        self.literals.get(&id).copied()
    }

    /// Whether `t` is still an open numeric literal.
    pub(crate) fn is_open_literal(&mut self, t: &Type) -> bool {
        matches!(self.prune(t), Type::Var(id) if self.literals.contains_key(&id))
    }

    /// Whether arithmetic must settle this open float literal to `float` now
    /// [TYPE-NUMERIC-LITERAL].
    pub(crate) fn is_open_float_literal(&mut self, t: &Type) -> bool {
        matches!(self.prune(t), Type::Var(id) if self.literal_default(id).is_some_and(Numeric::is_float))
    }

    /// Fix to its default every open literal free in `t`: a binding's type is
    /// decided at the binding, so nothing quantifies over a literal.
    pub(crate) fn settle_literals_in(&mut self, t: &Type) {
        if self.literals.is_empty() {
            return;
        }
        let mut free = BTreeSet::new();
        let applied = self.apply(t);
        self.free_vars(&applied, &mut free);
        for id in free {
            self.settle_literal(id);
        }
    }

    /// [`Self::expose`] for a rule that must act on the shape now: a literal
    /// has nothing further to wait for, so it takes its default here.
    pub(crate) fn expose_settled(&mut self, t: &Type) -> Type {
        self.settle_head(t);
        self.expose(t)
    }

    /// Default an outer literal without exposing aliases or walking its fields.
    pub(crate) fn settle_head(&mut self, t: &Type) {
        if !matches!(t, Type::Var(_)) {
            return;
        }
        if let Type::Var(id) = self.prune(t) {
            self.settle_literal(id);
        }
    }

    /// `t` as a diagnostic names it: an open literal reads as its default, the
    /// type it will have unless something decides otherwise.
    pub(crate) fn displayed(&mut self, t: &Type) -> Type {
        let applied = self.apply(t);
        crate::ty::map_type_vars(&applied, &mut |id| match self.literal_default(id) {
            Some(default) => Type::prim(default.name()),
            None => Type::Var(id),
        })
    }

    /// Fix to its default every literal nothing has decided.
    pub(crate) fn settle_all_literals(&mut self) {
        let open: Vec<VarId> = self.literals.keys().copied().collect();
        for id in open {
            self.settle_literal(id);
        }
    }

    fn settle_literal(&mut self, id: VarId) {
        if let Some(default) = self.literals.get(&id).copied() {
            if matches!(self.prune(&Type::Var(id)), Type::Var(root) if root == id) {
                self.bind(id, Type::prim(default.name()));
            }
        }
    }

    /// Allocate a fresh, unbound type variable.
    pub(crate) fn fresh(&mut self) -> Type {
        let id = VarId::try_from(self.subst.len()).unwrap_or(VarId::MAX);
        self.subst.push(None);
        Type::Var(id)
    }

    /// Follow a variable to its representative, compressing the path. Only the
    /// outermost variable is resolved — nested types are left intact (use
    /// [`InferCtx::apply`] for a deep walk).
    pub(crate) fn prune(&mut self, t: &Type) -> Type {
        let Type::Var(id) = t else {
            return t.clone();
        };
        let idx = usize::try_from(*id).unwrap_or(usize::MAX);
        let Some(bound) = self.subst.get(idx).and_then(Option::as_ref) else {
            return t.clone();
        };
        let Type::Var(next) = bound else {
            return bound.clone();
        };
        let pruned = self.prune(&Type::Var(*next));
        self.bind(*id, pruned.clone());
        pruned
    }

    /// How many variables are currently BOUND. A fixpoint loop compares this
    /// across passes to tell "that pass learned something" from "nothing left
    /// to learn" without diffing the whole substitution
    /// ([`crate::check::Checker::resolve_deferred_arithmetic`]).
    #[must_use]
    pub(crate) fn bound_count(&self) -> usize {
        self.subst.iter().filter(|slot| slot.is_some()).count()
    }

    /// Bind a variable to a type. The caller guarantees the occurs-check passed.
    pub(crate) fn bind(&mut self, id: VarId, t: Type) {
        let idx = usize::try_from(id).unwrap_or(usize::MAX);
        if let Some(slot) = self.subst.get_mut(idx) {
            *slot = Some(t);
        }
    }

    /// The occurs check: does variable `id` appear anywhere in `t`? Prevents
    /// the construction of infinite types like `t0 ~ List<t0>`.
    pub(crate) fn occurs(&mut self, id: VarId, t: &Type) -> bool {
        match t {
            Type::Var(v) => match self.lend(*v) {
                Some((slot, bound)) => {
                    let found = self.occurs(id, &bound);
                    self.restore(slot, bound);
                    found
                }
                None => *v == id,
            },
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
    pub(crate) fn apply(&mut self, t: &Type) -> Type {
        crate::ty::map_type_vars(t, &mut |v| match self.lend(v) {
            Some((slot, bound)) => {
                let resolved = self.apply(&bound);
                self.restore(slot, bound);
                resolved
            }
            None => Type::Var(v),
        })
    }

    /// Collect the free (unbound) variables of `t` into `out`.
    pub(crate) fn free_vars(&mut self, t: &Type, out: &mut BTreeSet<VarId>) {
        match t {
            Type::Var(v) => match self.lend(*v) {
                Some((slot, bound)) => {
                    self.free_vars(&bound, out);
                    self.restore(slot, bound);
                }
                None => {
                    let _ = out.insert(*v);
                }
            },
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

    /// Take variable `id`'s binding out of the substitution so a deep walk can
    /// read it in place instead of copying it at every node, as [`Self::prune`]
    /// does; [`Self::restore`] puts it back. The occurs-check keeps the
    /// substitution acyclic, so a walk never reaches the slot it borrowed.
    fn lend(&mut self, id: VarId) -> Option<(usize, Type)> {
        let slot = usize::try_from(id).ok()?;
        let bound = self.subst.get_mut(slot)?.take()?;
        Some((slot, bound))
    }

    fn restore(&mut self, slot: usize, bound: Type) {
        if let Some(entry) = self.subst.get_mut(slot) {
            *entry = Some(bound);
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
