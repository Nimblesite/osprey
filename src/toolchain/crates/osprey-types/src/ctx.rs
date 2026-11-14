//! The inference context: fresh-variable supply plus the substitution that
//! backs unification, stored as an index-addressed arena (`Vec<Option<Type>>`)
//! — the textbook union-find layout. `prune` compresses paths as it resolves,
//! so a chain of variable bindings is walked once and then answers in one hop.

use crate::error::TypeError;
use crate::numeric::Numeric;
use crate::ty::{Type, VarId};
use osprey_ast::{TypeExpr, Variance};
use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};

#[cfg(test)]
mod tests;

#[cfg(test)]
std::thread_local! {
    pub(crate) static FREE_VAR_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

type RecordTemplate = (Vec<String>, Vec<(String, String)>);

/// Holds every type variable's binding. Variable ids are indices into `subst`.
#[derive(Debug, Default)]
pub(crate) struct InferCtx {
    subst: Vec<Option<Type>>,
    bound: usize,
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
    /// Numeric literal defaults indexed by variable ID, through the last literal.
    /// Implements [TYPE-NUMERIC-LITERAL].
    literals: Vec<Option<Numeric>>,
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
        let (params, fields) = self.record_template(name, args.len())?;
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

    fn record_template(&self, name: &str, arity: usize) -> Option<&RecordTemplate> {
        self.records
            .get(name)
            .filter(|(params, _)| params.len() == arity)
    }

    /// A missing field is distinct from an unknown record or incorrect arity.
    pub(crate) fn record_field(
        &self,
        name: &str,
        args: &[Type],
        field: &str,
    ) -> Result<Option<Type>, ()> {
        let (params, fields) = self.record_template(name, args.len()).ok_or(())?;
        Ok(fields
            .iter()
            .rev()
            .find(|(name, _)| name == field)
            .map(|(_, ty)| {
                let binder = params.iter().cloned().zip(args.iter().cloned()).collect();
                crate::convert::type_name_to_type(ty, &binder)
            }))
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
        self.expose_type(Cow::Owned(t), site).into_owned()
    }

    fn expose_type<'a>(&self, t: Cow<'a, Type>, site: &str) -> Cow<'a, Type> {
        match self.representation(&t, site) {
            Some(representation) => self.expose_type(Cow::Owned(representation), site),
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
    pub(crate) fn seen_through<'a>(
        &mut self,
        a: &'a Type,
        b: &'a Type,
    ) -> (Cow<'a, Type>, Cow<'a, Type>) {
        let (a, b) = (self.prune_type(a), self.prune_type(b));
        let pair = (a.as_ref(), b.as_ref());
        let settled = matches!(pair, (Type::Var(_), _) | (_, Type::Var(_)))
            || matches!(pair, (Type::Con { name: left, .. }, Type::Con { name: right, .. }) if left == right);
        if settled || self.equations.is_empty() {
            return (a, b);
        }
        (
            self.expose_type(a, &self.site),
            self.expose_type(b, &self.site),
        )
    }

    /// A variable for a numeric literal whose context may still pick its type
    /// among `default`'s family ([TYPE-NUMERIC-LITERAL]).
    pub(crate) fn fresh_literal(&mut self, default: Numeric) -> Type {
        let var = self.fresh();
        self.literals.resize(self.subst.len() - 1, None);
        self.literals.push(Some(default));
        var
    }

    /// The default of a variable that is an open numeric literal.
    pub(crate) fn literal_default(&self, id: VarId) -> Option<Numeric> {
        self.literals
            .get(usize::try_from(id).ok()?)
            .copied()
            .flatten()
    }

    /// Whether `t` is still an open numeric literal.
    pub(crate) fn is_open_literal(&mut self, t: &Type) -> bool {
        matches!(self.prune(t), Type::Var(id) if self.literal_default(id).is_some())
    }

    /// Whether arithmetic must settle this open float literal to `float` now
    /// [TYPE-NUMERIC-LITERAL].
    pub(crate) fn is_open_float_literal(&mut self, t: &Type) -> bool {
        matches!(self.prune(t), Type::Var(id) if self.literal_default(id).is_some_and(Numeric::is_float))
    }

    /// Return the free variables in `t` after fixing its open numeric literals
    /// to their defaults. Each default is concrete, so it adds no new variables.
    pub(crate) fn free_vars_after_defaulting(&mut self, t: &Type) -> BTreeSet<VarId> {
        let mut free = BTreeSet::new();
        self.free_vars(t, &mut free);
        if !self.literals.is_empty() {
            free.retain(|id| {
                let Some(default) = self.literal_default(*id) else {
                    return true;
                };
                self.bind(*id, Type::prim(default.name()));
                false
            });
        }
        free
    }

    /// [`Self::expose`] for a rule that must act on the shape now: a literal
    /// has nothing further to wait for, so it takes its default here.
    pub(crate) fn expose_settled(&mut self, t: &Type) -> Type {
        self.settle_head(t);
        self.expose(t)
    }

    /// Default an outer literal without exposing aliases or walking its fields.
    pub(crate) fn settle_head(&mut self, t: &Type) {
        if let Type::Var(id) = t {
            let root = self.compress(*id);
            self.settle_literal(root);
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
        for (slot, default) in self.subst.iter_mut().zip(&self.literals) {
            if let (None, Some(default)) = (&*slot, default) {
                *slot = Some(Type::prim(default.name()));
                self.bound += 1;
            }
        }
    }

    fn settle_literal(&mut self, id: VarId) {
        if let Some(default) = self.literal_default(id) {
            if self.binding(id).is_none() {
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
        self.prune_type(t).into_owned()
    }

    fn prune_type<'a>(&mut self, t: &'a Type) -> Cow<'a, Type> {
        let Type::Var(id) = t else {
            return Cow::Borrowed(t);
        };
        let root = self.compress(*id);
        match self.binding(root) {
            Some(bound) => Cow::Owned(bound.clone()),
            None if root == *id => Cow::Borrowed(t),
            None => Cow::Owned(Type::Var(root)),
        }
    }

    /// Point aliases at one representative so only that slot owns the payload.
    fn compress(&mut self, id: VarId) -> VarId {
        let mut root = id;
        while let Some(&Type::Var(next)) = self.binding(root) {
            root = next;
        }
        let mut alias = id;
        while alias != root {
            let Some(&Type::Var(next)) = self.binding(alias) else {
                break;
            };
            if next == root {
                break;
            }
            self.bind(alias, Type::Var(root));
            alias = next;
        }
        root
    }

    /// How many variables are currently BOUND. A fixpoint loop compares this
    /// across passes to tell "that pass learned something" from "nothing left
    /// to learn" without diffing the whole substitution
    /// ([`crate::check::Checker::resolve_deferred_arithmetic`]).
    #[must_use]
    pub(crate) fn bound_count(&self) -> usize {
        self.bound
    }

    /// Bind a variable to a type. The caller guarantees the occurs-check passed.
    pub(crate) fn bind(&mut self, id: VarId, t: Type) {
        let idx = usize::try_from(id).unwrap_or(usize::MAX);
        if let Some(slot) = self.subst.get_mut(idx) {
            self.bound += usize::from(slot.is_none());
            *slot = Some(t);
        }
    }

    /// The occurs check: does variable `id` appear anywhere in `t`? Prevents
    /// the construction of infinite types like `t0 ~ List<t0>`.
    pub(crate) fn occurs(&self, id: VarId, t: &Type) -> bool {
        match t {
            Type::Var(v) => self
                .binding(*v)
                .map_or(*v == id, |bound| self.occurs(id, bound)),
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
    pub(crate) fn apply(&self, t: &Type) -> Type {
        let mut ty = t.clone();
        self.apply_in_place(&mut ty);
        ty
    }

    /// Resolve an owned type without rebuilding its existing storage.
    pub(crate) fn apply_in_place(&self, ty: &mut Type) {
        match ty {
            Type::Var(id) => {
                let mut bound = self.binding(*id);
                while let Some(Type::Var(next)) = bound {
                    *id = *next;
                    bound = self.binding(*next);
                }
                if let Some(resolved) = bound {
                    *ty = self.apply(resolved);
                }
            }
            Type::Fun { params, ret } => {
                for ty in params {
                    self.apply_in_place(ty);
                }
                self.apply_in_place(ret);
            }
            Type::Con { args, .. } | Type::Union { variants: args, .. } => {
                for ty in args {
                    self.apply_in_place(ty);
                }
            }
            Type::Record { fields, .. } => {
                for ty in fields.values_mut() {
                    self.apply_in_place(ty);
                }
            }
        }
    }

    /// Collect the free (unbound) variables of `t` into `out`.
    pub(crate) fn free_vars(&self, t: &Type, out: &mut BTreeSet<VarId>) {
        #[cfg(test)]
        FREE_VAR_VISITS.with(|count| count.set(count.get() + 1));
        match t {
            Type::Var(v) => match self.binding(*v) {
                Some(bound) => self.free_vars(bound, out),
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

    fn binding(&self, id: VarId) -> Option<&Type> {
        self.subst.get(usize::try_from(id).ok()?)?.as_ref()
    }
}
