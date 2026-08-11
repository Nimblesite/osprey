//! The Osprey type language — the representation every stage of the checker
//! operates on.
//!
//! A single `Type` enum models every type. Following the standard
//! Hindley-Milner representation, primitives, nullary nominals and generics
//! collapse into one *type-constructor application* (`Con`) — smaller than a
//! per-category split, and exactly what unification operates on — while
//! exhaustive matches over the enum give compiler-enforced totality. Rendered
//! spellings are the language's canonical ones, so inferred types print
//! exactly as they appear in source and diagnostics.

use std::collections::BTreeMap;
use std::fmt;

/// Identifier for an inference type variable.
pub type VarId = u32;

/// The language's canonical type-constructor names. The type checker, builtins
/// table, and codegen all agree on these exact strings.
pub mod names {
    /// The 64-bit integer primitive.
    pub const INT: &str = "int";
    /// The floating-point primitive.
    pub const FLOAT: &str = "float";
    /// The string primitive.
    pub const STRING: &str = "string";
    /// The boolean primitive.
    pub const BOOL: &str = "bool";
    /// The erased compatibility type that unifies with every value [TYPE-ANY].
    pub const ANY: &str = "any";
    /// The unit type, returned by expressions with no meaningful value.
    pub const UNIT: &str = "Unit";
    /// The `Result<ok, err>` sum type.
    pub const RESULT: &str = "Result";
    /// `Result`'s ok-variant constructor (`Success { value }`).
    pub(crate) const SUCCESS: &str = "Success";
    /// `Result`'s error-variant constructor (`Error { message }`).
    pub(crate) const ERROR: &str = "Error";
    /// The error type produced by failing arithmetic operations.
    pub(crate) const MATH_ERROR: &str = "MathError";
    /// The `List<elem>` collection type.
    pub const LIST: &str = "List";
    /// The `Map<key, value>` collection type.
    pub const MAP: &str = "Map";
    /// A fused range pipeline, materialized only while lowering iterator calls.
    pub const ITERATOR: &str = "Iterator";
    /// The lightweight concurrent execution context type.
    pub const FIBER: &str = "Fiber";
    /// The inter-fiber message-passing channel type.
    pub const CHANNEL: &str = "Channel";
    /// The opaque foreign pointer type used for C interop.
    pub const PTR: &str = "Ptr";
    /// The dense scalar buffer for GPU computation [GPU-BUFFER]
    /// (docs/specs/0034-GPUComputation.md).
    pub const GPU_BUFFER: &str = "GpuBuffer";
}

/// A type in the Osprey type system.
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    /// An inference variable, rendered `t<id>`.
    Var(VarId),
    /// A named constructor applied to zero+ arguments. Zero args ⇒ a
    /// primitive/nullary type (`int`, `Unit`); arguments make it generic
    /// (`List<t>`, `Result<t, e>`).
    Con {
        /// The constructor's name.
        name: String,
        /// The type arguments applied to the constructor.
        args: Vec<Type>,
    },
    /// A function `(p0, p1, ...) -> ret`.
    Fun {
        /// The parameter types, in order.
        params: Vec<Type>,
        /// The return type.
        ret: Box<Type>,
    },
    /// A structural record — equality is by field name+type, never field order:
    /// HM unification must accept two spellings of the same record regardless
    /// of the order their fields were written in.
    Record {
        /// The record's name.
        name: String,
        /// The record's fields, keyed by field name (order-independent).
        fields: BTreeMap<String, Type>,
    },
    /// A nominal sum type whose variants are nullary `Con`s or `Record`s.
    Union {
        /// The union's name.
        name: String,
        /// The union's variant types.
        variants: Vec<Type>,
    },
}

impl Type {
    /// A constructor application, e.g. `Type::con("List", vec![Type::int()])`.
    pub(crate) fn con(name: impl Into<String>, args: Vec<Type>) -> Type {
        Type::Con {
            name: name.into(),
            args,
        }
    }
    /// A nullary named type (`int`, `Unit`, a bare user type).
    pub(crate) fn prim(name: impl Into<String>) -> Type {
        Type::con(name, Vec::new())
    }
    /// The `int` primitive type.
    #[must_use]
    pub(crate) fn int() -> Type {
        Type::prim(names::INT)
    }
    /// The `float` primitive type.
    #[must_use]
    pub(crate) fn float() -> Type {
        Type::prim(names::FLOAT)
    }
    /// The `string` primitive type.
    #[must_use]
    pub(crate) fn string() -> Type {
        Type::prim(names::STRING)
    }
    /// The `bool` primitive type.
    #[must_use]
    pub(crate) fn bool() -> Type {
        Type::prim(names::BOOL)
    }
    /// The `Unit` primitive type.
    #[must_use]
    pub fn unit() -> Type {
        Type::prim(names::UNIT)
    }
    /// The `any` top type.
    #[must_use]
    pub(crate) fn any() -> Type {
        Type::prim(names::ANY)
    }
    /// The opaque foreign-pointer type [FFI-PTR].
    #[must_use]
    pub fn ptr() -> Type {
        Type::prim(names::PTR)
    }
    /// A function type from the given parameters to the given return type.
    #[must_use]
    pub fn fun(params: Vec<Type>, ret: Type) -> Type {
        Type::Fun {
            params,
            ret: Box::new(ret),
        }
    }
    /// `Result<ok, err>`.
    #[must_use]
    pub(crate) fn result(ok: Type, err: Type) -> Type {
        Type::con(names::RESULT, vec![ok, err])
    }
    /// `List<elem>`.
    #[must_use]
    pub(crate) fn list(elem: Type) -> Type {
        Type::con(names::LIST, vec![elem])
    }
    /// `Map<key, value>`.
    #[must_use]
    pub(crate) fn map(key: Type, value: Type) -> Type {
        Type::con(names::MAP, vec![key, value])
    }
    /// `Iterator<elem>` used by range/map/filter/fold pipelines.
    #[must_use]
    pub(crate) fn iterator(elem: Type) -> Type {
        Type::con(names::ITERATOR, vec![elem])
    }
    /// `GpuBuffer<elem>` — the dense scalar buffer [GPU-BUFFER].
    #[must_use]
    pub(crate) fn gpu_buffer(elem: Type) -> Type {
        Type::con(names::GPU_BUFFER, vec![elem])
    }

    /// True if this is a nullary-or-applied constructor with the given name.
    #[must_use]
    pub(crate) fn is_named(&self, n: &str) -> bool {
        matches!(self, Type::Con { name, .. } if name == n)
    }
}

/// Whether a (fully substituted) type still mentions a type variable — the
/// mark of a polymorphic signature that must be specialised per use.
#[must_use]
pub fn has_type_var(ty: &Type) -> bool {
    match ty {
        Type::Var(_) => true,
        Type::Con { args, .. } => args.iter().any(has_type_var),
        Type::Fun { params, ret } => params.iter().any(has_type_var) || has_type_var(ret),
        Type::Record { fields, .. } => fields.values().any(has_type_var),
        Type::Union { variants, .. } => variants.iter().any(has_type_var),
    }
}

/// A polymorphic type scheme `forall vars. ty` — the engine of
/// let-polymorphism: generalize at bindings, instantiate at uses.
#[derive(Debug, Clone, PartialEq)]
pub struct Scheme {
    /// The universally quantified type variables.
    pub(crate) vars: Vec<VarId>,
    /// The quantified type body.
    pub(crate) ty: Type,
    /// Built-in representation obligations this body carries on its quantified
    /// variables: `(built-in name, the type it was applied to)`. A wrapper like
    /// `fn bufferLength(xs) = gpuLength(toGpu(xs))` generalizes over `xs`, and
    /// its obligation — that `xs` is a scalar list [GPU-BUFFER-ELEM] — must
    /// travel with the scheme, or the wrapper launders `List<string>` into a
    /// buffer at a call site the obligation never reaches. Each instantiation
    /// re-states them against that site's fresh variables
    /// ([`crate::env::instantiate`]).
    pub(crate) obligations: Vec<(String, Type)>,
}

impl Scheme {
    /// A monomorphic scheme — no quantified variables.
    #[must_use]
    pub(crate) fn mono(ty: Type) -> Scheme {
        Scheme {
            vars: Vec::new(),
            ty,
            obligations: Vec::new(),
        }
    }
    /// A polymorphic scheme over the given variables.
    #[must_use]
    pub(crate) fn poly(vars: Vec<VarId>, ty: Type) -> Scheme {
        Scheme {
            vars,
            ty,
            obligations: Vec::new(),
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Var(id) => write!(f, "t{id}"),
            Type::Con { name, args } if args.is_empty() => write!(f, "{name}"),
            Type::Con { name, args } => {
                write!(f, "{name}<")?;
                write_seq(f, args)?;
                write!(f, ">")
            }
            Type::Fun { params, ret } => {
                write!(f, "(")?;
                write_seq(f, params)?;
                write!(f, ") -> {ret}")
            }
            Type::Record { fields, .. } => {
                write!(f, "{{ ")?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, " }}")
            }
            Type::Union { name, .. } => write!(f, "{name}"),
        }
    }
}

fn write_seq(f: &mut fmt::Formatter<'_>, items: &[Type]) -> fmt::Result {
    for (i, t) in items.iter().enumerate() {
        if i > 0 {
            write!(f, ", ")?;
        }
        write!(f, "{t}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_primitives_and_generics() {
        assert_eq!(Type::int().to_string(), "int");
        assert_eq!(Type::list(Type::string()).to_string(), "List<string>");
        assert_eq!(
            Type::result(Type::int(), Type::prim("MathError")).to_string(),
            "Result<int, MathError>"
        );
        assert_eq!(
            Type::fun(vec![Type::int(), Type::int()], Type::bool()).to_string(),
            "(int, int) -> bool"
        );
        assert_eq!(Type::Var(3).to_string(), "t3");
    }

    #[test]
    fn ptr_is_the_named_pointer_primitive() {
        assert!(Type::ptr().is_named(names::PTR));
        assert_eq!(Type::ptr().to_string(), names::PTR);
    }

    #[test]
    fn has_type_var_walks_records_and_unions() {
        // A record whose only field is a variable is polymorphic.
        let rec = Type::Record {
            name: "R".into(),
            fields: [("x".to_string(), Type::Var(0))].into_iter().collect(),
        };
        assert!(has_type_var(&rec));
        // A union mentioning a variable in a variant is polymorphic; a fully
        // concrete one is not.
        let poly_union = Type::Union {
            name: "U".into(),
            variants: vec![Type::int(), Type::Var(1)],
        };
        let mono_union = Type::Union {
            name: "U".into(),
            variants: vec![Type::int(), Type::string()],
        };
        assert!(has_type_var(&poly_union));
        assert!(!has_type_var(&mono_union));
        // A generic constructor application is polymorphic via its args.
        assert!(has_type_var(&Type::list(Type::Var(2))));
        assert!(!has_type_var(&Type::list(Type::int())));
    }
}
