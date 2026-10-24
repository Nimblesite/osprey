//! Resolved type information published for the code generator. Type inference
//! runs to completion; the resulting concrete signatures, record layouts and
//! union tags are then frozen into the plain, substitution-free tables below
//! so the backend can drive codegen off real types instead of guessing `i64`.

use crate::ty::{names, Type, VarId};
use std::collections::HashMap;

/// Closed-program requirements on full application, separate from written row bounds.
/// Unknown callback effects are never represented as an empty proven row.
/// Implements [LSP-EFFECT-REQUIREMENTS].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EffectRequirements {
    /// Sorted operation identities, including generic effect instances.
    pub operations: Vec<String>,
    /// Host operations tracked separately from algebraic operations.
    pub runtime_builtins: Vec<String>,
    /// Symbolic callback uses or unresolved dynamic calls remain in the summary.
    pub unresolved_callbacks: bool,
    /// Some operation instances retain unsolved type arguments.
    pub unresolved_instances: bool,
}

/// The declared shape of a record/variant constructor: ordered `(field, type)`
/// pairs written as type names (`int`, `string`, `Point`, …) plus its owning
/// type. Field type strings are kept verbatim so the backend maps them to its
/// own LLVM type lattice.
#[derive(Debug, Clone)]
pub struct CtorLayout {
    /// The type this constructor builds (`Point` for a record, `Shape` for a
    /// union variant).
    pub owner: String,
    /// Whether the owner is a single-variant record (vs. a union variant).
    pub owner_is_record: bool,
    /// The owner's declared type parameters (`["T"]` for `Generic<T>`), so the
    /// backend can tell a generic field (`data: T`) — whose concrete LLVM type
    /// is fixed per construction — from a nominal one (`origin: Point`).
    pub type_params: Vec<String>,
    /// Ordered `(field name, resolved field type)` — a generic field
    /// (`data: T`) resolves to a [`Type::Var`], which the backend lowers to its
    /// uniform boxed representation.
    pub fields: Vec<(String, Type)>,
}

/// One effect operation's resolved signature.
#[derive(Debug, Clone, PartialEq)]
pub struct OpType {
    /// Parameter types, in declaration order.
    pub params: Vec<Type>,
    /// The operation's return type.
    pub ret: Type,
    /// The DECLARED mode. A value operation's arm returns `ret`; a `control`
    /// operation's arm returns the handler's answer and owns the continuation.
    /// Carried on the resolved signature so the checker and the backend read
    /// one source of truth. Implements [EFFECTS-HANDLER-ARMS].
    pub mode: osprey_ast::OperationMode,
}

/// Everything the code generator needs from inference: per-function signatures
/// (fully resolved against the final substitution), constructor layouts and
/// union memberships.
#[derive(Debug, Clone, Default)]
pub struct ProgramTypes {
    /// Checked signal metadata at source callee identities, before discharge.
    pub(crate) signal_queries: HashMap<usize, EffectRequirements>,
    /// Function/extern name → (resolved parameter types, resolved return type).
    pub functions: HashMap<String, (Vec<Type>, Type)>,
    /// Qualified function name → the same requirements used by entry checking.
    pub function_effects: HashMap<String, EffectRequirements>,
    /// Constructor name → its record/variant layout.
    pub ctors: HashMap<String, CtorLayout>,
    /// Union type name → ordered variant constructor names (tag order).
    pub unions: HashMap<String, Vec<String>>,
    /// Effect name → operation name → resolved signature.
    pub effects: HashMap<String, HashMap<String, OpType>>,
    /// Lambda source position → its resolved function type, so the backend
    /// lowers every lambda from inferred types, not guesses.
    pub(crate) lambdas: SiteTable,
    /// `let` binding source position → its resolved type, so editor hover can
    /// show the type of an unannotated binding. Implements [LSP-HOVER-VARIABLES]
    pub(crate) lets: SiteTable,
    /// Each list literal's resolved `List<T>`, keyed by its source position.
    /// The backend reads it when the literal itself cannot supply an element
    /// representation — i.e. when it is empty.
    pub(crate) lists: SiteTable,
    /// `perform` site position `(line, column)` → the operation signature and
    /// effect type arguments instantiated at that site, so the backend can
    /// box/unbox erased generic operation slots against the site's concrete
    /// types and key the runtime handler lookup by instantiation. Implements
    /// [EFFECTS-GENERIC-INSTANTIATION].
    pub performs: HashMap<(u32, u32), PerformSite>,
    /// `handle` site position `(line, column)` → the effect type arguments
    /// and each operation's signature instantiated at that site (the
    /// handler-arm view of the same instantiation data). Implements
    /// [EFFECTS-GENERIC-INSTANTIATION].
    pub handler_ops: HashMap<(u32, u32), HandlerSite>,
    /// `perform`/`handler`/`mask` sites naming a capability: position → the
    /// capability's effect. The capability's runtime identity is the value
    /// bound to the written name. Implements [EFFECTS-INSTANCE].
    pub capabilities: HashMap<(u32, u32), String>,
    /// Explicit call substitutions, applied while specializing generic bodies.
    pub applications: HashMap<(u32, u32), HashMap<VarId, Type>>,
    /// Binder identity is part of the contract even when its spelling is omitted.
    pub(crate) declared_params: HashMap<String, HashMap<String, Type>>,
    pub(crate) call_bindings: HashMap<usize, HashMap<VarId, Type>>,
    pub(crate) methods: crate::methods::Targets,
    /// Generalized source-binding constraints compared by annotation diagnostics.
    pub(crate) obligations: HashMap<String, Vec<(String, Type)>>,
}

/// One `perform` site's resolved instantiation.
#[derive(Debug, Clone, PartialEq)]
pub struct PerformSite {
    /// The performed operation's instantiated signature.
    pub op: OpType,
    /// The effect's resolved type arguments at this site.
    pub effect_args: Vec<Type>,
}

/// One `handle` site's resolved instantiation.
#[derive(Debug, Clone, PartialEq)]
pub struct HandlerSite {
    /// The effect's resolved type arguments at this site.
    pub effect_args: Vec<Type>,
    /// Operation name → its instantiated signature.
    pub ops: HashMap<String, OpType>,
}

impl ProgramTypes {
    /// Instantiate a record field against the actual generic type arguments.
    #[must_use]
    pub fn field_type(&self, record: &Type, field: &str) -> Option<Type> {
        match record {
            Type::Record { fields, .. } => fields.get(field).cloned(),
            Type::Con { name, args } => {
                let layout = self.ctors.get(name)?;
                let (_, ty) = layout.fields.iter().find(|(name, _)| name == field)?;
                let bindings = args
                    .iter()
                    .enumerate()
                    .filter_map(|(i, ty)| u32::try_from(i).ok().map(|i| (i, ty.clone())))
                    .collect();
                Some(crate::env::subst_vars(ty, &bindings))
            }
            _ => None,
        }
    }

    /// Resolve a named callee's signature against this application's binders.
    #[must_use]
    pub fn application_type(&self, position: Option<osprey_ast::Position>, ty: &Type) -> Type {
        position
            .and_then(|p| self.applications.get(&(p.line, p.column)))
            .map_or_else(
                || ty.clone(),
                |bindings| crate::env::subst_vars(ty, bindings),
            )
    }

    /// Attach inferred applications to a backend copy of the source program.
    /// The source AST and its diagnostic positions remain unchanged.
    pub fn elaborate_calls(&mut self, program: &osprey_ast::Program) -> osprey_ast::Program {
        crate::applications::elaborate(program, self)
    }

    /// Specialize one callee's metadata without fixing other generic bodies.
    /// Expanded call bindings can mention their variables too, but each body
    /// needs its own instantiation. Implements [TYPE-GENERICS-APPLY].
    #[must_use]
    pub fn specialized(&self, bindings: &HashMap<VarId, Type>, function: Option<&str>) -> Self {
        let mut result = self.clone();
        let substitute = |ty: &mut Type| *ty = crate::env::subst_vars(ty, bindings);
        result.specialize_body(function, &substitute);
        result.specialize_operations(&substitute);
        result
            .applications
            .values_mut()
            .flat_map(HashMap::values_mut)
            .chain(
                result
                    .declared_params
                    .values_mut()
                    .flat_map(HashMap::values_mut),
            )
            .for_each(substitute);
        result
    }

    fn specialize_body(&mut self, function: Option<&str>, substitute: &impl Fn(&mut Type)) {
        for (_, (params, ret)) in self
            .functions
            .iter_mut()
            .filter(|(name, _)| function.is_none_or(|f| f == name.as_str()))
        {
            params.iter_mut().for_each(substitute);
            substitute(ret);
        }
        for table in [&mut self.lambdas, &mut self.lets, &mut self.lists] {
            table.specialize(function, substitute);
        }
    }

    fn specialize_operations(&mut self, substitute: &impl Fn(&mut Type)) {
        for site in self.performs.values_mut() {
            site.effect_args.iter_mut().for_each(substitute);
            site.op.params.iter_mut().for_each(substitute);
            substitute(&mut site.op.ret);
        }
        for site in self.handler_ops.values_mut() {
            site.effect_args.iter_mut().for_each(substitute);
            for op in site.ops.values_mut() {
                op.params.iter_mut().for_each(substitute);
                substitute(&mut op.ret);
            }
        }
    }

    /// The resolved return type of a named function, if known.
    #[must_use]
    pub fn return_type(&self, name: &str) -> Option<&Type> {
        self.functions.get(name).map(|(_, ret)| ret)
    }

    /// The resolved parameter types of a named function, if known.
    #[must_use]
    pub fn param_types(&self, name: &str) -> Option<&[Type]> {
        self.functions.get(name).map(|(p, _)| p.as_slice())
    }

    /// The resolved function type of the lambda written at `position` inside
    /// top-level function `function` (`None` at file scope or when unknown).
    #[must_use]
    pub fn lambda_type(
        &self,
        function: Option<&str>,
        position: Option<osprey_ast::Position>,
    ) -> Option<&Type> {
        self.lambdas.get(function, position)
    }

    /// The resolved type of the `let` binding written at `position` inside
    /// top-level function `function`. Implements [LSP-HOVER-VARIABLES]
    #[must_use]
    pub fn let_type(
        &self,
        function: Option<&str>,
        position: Option<osprey_ast::Position>,
    ) -> Option<&Type> {
        self.lets.get(function, position)
    }

    /// The resolved element type of the list literal written at `position`
    /// inside top-level function `function`, when inference pinned it to
    /// something concrete. This is how an EMPTY literal still reaches the
    /// backend with an element representation ([GPU-BUFFER-ELEM]).
    #[must_use]
    pub fn list_elem_type(
        &self,
        function: Option<&str>,
        position: Option<osprey_ast::Position>,
    ) -> Option<&Type> {
        self.lists.get(function, position).and_then(|ty| match ty {
            Type::Con { name, args } if name == names::LIST => args.first(),
            _ => None,
        })
    }
}

/// Inferred types keyed by source position, one entry per enclosing top-level
/// function. Static discharge copies a function body once per handler region
/// ([STAGE-POLY]), so two copies share every position and only the function
/// each landed in tells a `List<float>` binding from a `GpuBuffer<float>` one.
/// A lookup names the function being lowered; a position recorded under no
/// matching function (an inlined generic body) answers with the last entry,
/// which is what a flat position map always did.
#[derive(Debug, Clone, Default)]
pub struct SiteTable {
    entries: HashMap<(u32, u32), Vec<Entry>>,
}

/// One recorded entry: the enclosing top-level function and the resolved type.
type Entry = (Option<String>, Type);

impl SiteTable {
    fn specialize(&mut self, function: Option<&str>, substitute: &impl Fn(&mut Type)) {
        self.entries
            .values_mut()
            .flatten()
            .filter(|(owner, _)| function.is_none_or(|f| owner.as_deref() == Some(f)))
            .for_each(|(_, ty)| substitute(ty));
    }

    pub(crate) fn insert(&mut self, function: Option<&str>, position: (u32, u32), ty: Type) {
        let recorded = self.entries.entry(position).or_default();
        match recorded
            .iter_mut()
            .find(|(owner, _)| owner.as_deref() == function)
        {
            Some(slot) => slot.1 = ty,
            None => recorded.push((function.map(str::to_owned), ty)),
        }
    }

    /// The type recorded at `position` for `function`, else the last one
    /// recorded there.
    #[must_use]
    pub fn get(
        &self,
        function: Option<&str>,
        position: Option<osprey_ast::Position>,
    ) -> Option<&Type> {
        let recorded = self.entries.get(&(position?.line, position?.column))?;
        recorded
            .iter()
            .find(|(owner, _)| owner.as_deref() == function)
            .or_else(|| recorded.last())
            .map(|(_, ty)| ty)
    }

    /// Whether nothing was recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every recorded type, in no particular order.
    pub fn values(&self) -> impl Iterator<Item = &Type> {
        self.entries.values().flatten().map(|(_, ty)| ty)
    }

    pub(crate) fn values_mut(&mut self) -> impl Iterator<Item = &mut Type> {
        self.entries.values_mut().flatten().map(|(_, ty)| ty)
    }

    /// Every entry as its position and type; a position copied into several
    /// functions appears once per copy.
    pub fn iter(&self) -> impl Iterator<Item = (&(u32, u32), &Type)> {
        self.entries
            .iter()
            .flat_map(|(position, recorded)| recorded.iter().map(move |(_, ty)| (position, ty)))
    }
}

#[cfg(test)]
mod site_table_tests {
    use super::{SiteTable, Type};
    use osprey_ast::Position;

    const SHARED: (u32, u32) = (7, 4);

    fn at(position: (u32, u32)) -> Position {
        Position {
            line: position.0,
            column: position.1,
        }
    }

    /// Two specializations of one function share every source position; each
    /// reads back its own type, and a reader naming neither gets the last one.
    #[test]
    fn one_position_holds_one_type_per_enclosing_function() {
        let mut table = SiteTable::default();
        table.insert(Some("run__stage1"), SHARED, Type::list(Type::float()));
        table.insert(Some("run__stage5"), SHARED, Type::prim("GpuBuffer"));
        assert_eq!(
            table.get(Some("run__stage1"), Some(at(SHARED))),
            Some(&Type::list(Type::float()))
        );
        assert_eq!(
            table.get(Some("run__stage5"), Some(at(SHARED))),
            Some(&Type::prim("GpuBuffer"))
        );
        assert_eq!(
            table.get(None, Some(at(SHARED))),
            Some(&Type::prim("GpuBuffer"))
        );
        assert_eq!(
            table.get(Some("other"), Some(at(SHARED))),
            Some(&Type::prim("GpuBuffer"))
        );
        assert_eq!(table.get(Some("run__stage1"), Some(at((1, 1)))), None);
        assert_eq!(table.get(Some("run__stage1"), None), None);
        assert_eq!(table.iter().count(), 2);
    }

    /// Re-recording a site for the same function replaces its type instead of
    /// adding a second entry.
    #[test]
    fn re_recording_a_site_replaces_its_type() {
        let mut table = SiteTable::default();
        table.insert(None, SHARED, Type::int());
        table.insert(None, SHARED, Type::float());
        assert_eq!(table.get(None, Some(at(SHARED))), Some(&Type::float()));
        assert_eq!(table.values().count(), 1);
        assert!(!table.is_empty());
    }
}
