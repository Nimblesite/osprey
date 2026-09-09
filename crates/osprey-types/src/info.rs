//! Resolved type information published for the code generator. Type inference
//! runs to completion; the resulting concrete signatures, record layouts and
//! union tags are then frozen into the plain, substitution-free tables below
//! so the backend can drive codegen off real types instead of guessing `i64`.

use crate::ty::{names, Type, VarId};
use std::collections::HashMap;

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
}

/// Everything the code generator needs from inference: per-function signatures
/// (fully resolved against the final substitution), constructor layouts and
/// union memberships.
#[derive(Debug, Clone, Default)]
pub struct ProgramTypes {
    /// Function/extern name → (resolved parameter types, resolved return type).
    pub functions: HashMap<String, (Vec<Type>, Type)>,
    /// Constructor name → its record/variant layout.
    pub ctors: HashMap<String, CtorLayout>,
    /// Union type name → ordered variant constructor names (tag order).
    pub unions: HashMap<String, Vec<String>>,
    /// Effect name → operation name → resolved signature.
    pub effects: HashMap<String, HashMap<String, OpType>>,
    /// Lambda source position `(line, column)` → its resolved function type,
    /// so the backend lowers every lambda from inferred types, not guesses.
    pub(crate) lambdas: HashMap<(u32, u32), Type>,
    /// `let` binding source position `(line, column)` → its resolved type, so
    /// editor hover can show the type of an unannotated binding.
    /// Implements [LSP-HOVER-VARIABLES]
    pub(crate) lets: HashMap<(u32, u32), Type>,
    /// Each list literal's resolved `List<T>`, keyed by its source position.
    /// The backend reads it when the literal itself cannot supply an element
    /// representation — i.e. when it is empty.
    pub(crate) lists: HashMap<(u32, u32), Type>,
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
    /// Explicit call substitutions, applied while specializing generic bodies.
    pub applications: HashMap<(u32, u32), HashMap<VarId, Type>>,
    /// Binder identity is part of the contract even when its spelling is omitted.
    pub(crate) declared_params: HashMap<String, HashMap<String, Type>>,
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
    /// Specialize inferred body metadata with one call's type substitution.
    /// Constructor and effect declarations retain their shared erased ABI.
    #[must_use]
    pub fn specialized(&self, bindings: &HashMap<VarId, Type>) -> Self {
        let mut result = self.clone();
        let substitute = |ty: &mut Type| *ty = crate::env::subst_vars(ty, bindings);
        for (params, ret) in result.functions.values_mut() {
            params.iter_mut().for_each(&substitute);
            substitute(ret);
        }
        for table in [&mut result.lambdas, &mut result.lets, &mut result.lists] {
            table.values_mut().for_each(&substitute);
        }
        for site in result.performs.values_mut() {
            site.effect_args.iter_mut().for_each(&substitute);
            site.op.params.iter_mut().for_each(&substitute);
            substitute(&mut site.op.ret);
        }
        for site in result.handler_ops.values_mut() {
            site.effect_args.iter_mut().for_each(&substitute);
            for op in site.ops.values_mut() {
                op.params.iter_mut().for_each(&substitute);
                substitute(&mut op.ret);
            }
        }
        for args in result.applications.values_mut() { args.values_mut().for_each(&substitute); }
        for args in result.declared_params.values_mut() { args.values_mut().for_each(&substitute); }
        result
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

    /// The resolved function type of the lambda written at `position`.
    #[must_use]
    pub fn lambda_type(&self, position: Option<osprey_ast::Position>) -> Option<&Type> {
        position.and_then(|p| self.lambdas.get(&(p.line, p.column)))
    }

    /// The resolved type of the `let` binding written at `position`.
    /// Implements [LSP-HOVER-VARIABLES]
    #[must_use]
    pub fn let_type(&self, position: Option<osprey_ast::Position>) -> Option<&Type> {
        position.and_then(|p| self.lets.get(&(p.line, p.column)))
    }

    /// The resolved element type of the list literal written at `position`,
    /// when inference pinned it to something concrete. This is how an EMPTY
    /// literal still reaches the backend with an element representation
    /// ([GPU-BUFFER-ELEM]).
    #[must_use]
    pub fn list_elem_type(&self, position: Option<osprey_ast::Position>) -> Option<&Type> {
        position
            .and_then(|p| self.lists.get(&(p.line, p.column)))
            .and_then(|ty| match ty {
                Type::Con { name, args } if name == names::LIST => args.first(),
                _ => None,
            })
    }
}
