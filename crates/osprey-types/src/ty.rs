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

/// The nullary type constructors (`int`, `Unit`, …). Each is the same body
/// over a different name in [`names`], so they are enumerated as rows rather
/// than written out one identical function at a time.
macro_rules! prim_types {
    ($($(#[$doc:meta])* $vis:vis $name:ident = $konst:ident;)*) => {
        $(
            $(#[$doc])*
            #[must_use]
            $vis fn $name() -> Type {
                Type::prim(names::$konst)
            }
        )*
    };
}

/// The applied type constructors (`List<e>`, `Map<k, v>`, …): one row per
/// constructor, listing its argument names in order.
macro_rules! applied_types {
    ($($(#[$doc:meta])* $vis:vis $name:ident = $konst:ident($($arg:ident),+);)*) => {
        $(
            $(#[$doc])*
            #[must_use]
            $vis fn $name($($arg: Type),+) -> Type {
                Type::con(names::$konst, vec![$($arg),+])
            }
        )*
    };
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
    prim_types! {
        /// The `int` primitive type.
        pub(crate) int = INT;
        /// The `float` primitive type.
        pub(crate) float = FLOAT;
        /// The `string` primitive type.
        pub(crate) string = STRING;
        /// The `bool` primitive type.
        pub(crate) bool = BOOL;
        /// The `Unit` primitive type.
        pub unit = UNIT;
        /// The `any` top type.
        pub(crate) any = ANY;
        /// The opaque foreign-pointer type [FFI-PTR].
        pub ptr = PTR;
    }

    /// A function type from the given parameters to the given return type.
    #[must_use]
    pub fn fun(params: Vec<Type>, ret: Type) -> Type {
        Type::Fun {
            params,
            ret: Box::new(ret),
        }
    }

    applied_types! {
        /// `Result<ok, err>`.
        pub(crate) result = RESULT(ok, err);
        /// `List<elem>`.
        pub(crate) list = LIST(elem);
        /// `Map<key, value>`.
        pub(crate) map = MAP(key, value);
        /// `Iterator<elem>` used by range/map/filter/fold pipelines.
        pub(crate) iterator = ITERATOR(elem);
        /// `GpuBuffer<elem>` — the dense scalar buffer [GPU-BUFFER].
        pub(crate) gpu_buffer = GPU_BUFFER(elem);
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

/// The spelling of a slot the checker proved nothing about.
///
/// This is a DISPLAY spelling and **not valid Osprey source**: the parser reads
/// `_` as an ordinary nominal type name, so `fn f(x: int) -> _ = x` fails with
/// *cannot unify `_` with int*, exactly as a typo would. A rendered type
/// carrying a hole is therefore a description of what is known, never a
/// paste-back annotation ([TYPE-RENDER-HOLES] records the caveat).
pub const HOLE: &str = "_";

/// Render a type for a READER: every unsolved variable spelled [`HOLE`], and
/// every DECLARED record spelled by its name AND its row.
///
/// [`Display`] is the faithful rendering and must stay that way — the checker
/// keys effect-row sites off it — so both of those reader courtesies live here
/// instead. A variable Displays as `t5`, the checker's private name: unstable
/// across edits and meaningless outside the run that produced it. The
/// alternative tooling used was to give up and print `Unit`, which is worse —
/// a hole admits ignorance, `Unit` asserts a return type the checker refutes.
/// This keeps every proven part (`Result<int, _>` still says the payload is
/// `int`). Use it for anything a person reads and never to build source.
/// Implements [TYPE-RENDER-HOLES].
#[must_use]
pub fn render_with_holes(ty: &Type) -> String {
    for_reader(ty).to_string()
}

/// Rewrite a type into the shape a reader should see, then let [`Display`]
/// spell it — one rendering engine, not two.
fn for_reader(ty: &Type) -> Type {
    let over = |ts: &[Type]| ts.iter().map(for_reader).collect();
    match ty {
        Type::Var(_) => Type::prim(HOLE),
        // A declared record is shown by NAME AND ROW: `Point { x: int, y: int }`.
        // The name alone was tried and lost information the reader used to
        // have — an instantiated `Box<int>` collapsed to `Box`, because a
        // record carries no type arguments to put back (#214). The row does
        // carry them, so showing both keeps the author's spelling and the
        // instantiation. Built as a `prim` so this stays a READER concern:
        // `Display` renders the bare row and remains the faithful key
        // `check.rs` builds effect-row sites from.
        Type::Record { name, fields } if !name.is_empty() => Type::prim(format!(
            "{name} {}",
            Type::Record {
                name: String::new(),
                fields: fields
                    .iter()
                    .map(|(k, v)| (k.clone(), for_reader(v)))
                    .collect(),
            }
        )),
        Type::Con { name, args } => Type::con(name.clone(), over(args)),
        Type::Fun { params, ret } => Type::Fun {
            params: over(params),
            ret: Box::new(for_reader(ret)),
        },
        Type::Record { name, fields } => Type::Record {
            name: name.clone(),
            fields: fields
                .iter()
                .map(|(k, v)| (k.clone(), for_reader(v)))
                .collect(),
        },
        Type::Union { name, variants } => Type::Union {
            name: name.clone(),
            variants: over(variants),
        },
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
            // A record renders STRUCTURALLY here, name or no name. This is the
            // faithful rendering — `check.rs` builds effect-row site keys from
            // it, so collapsing a declared record to its bare name made
            // `Box<int>` and `Box<string>` one key and let a handler discharge
            // an operation it did not match. The nominal spelling a reader
            // wants is [`render_with_holes`]'s job, not this one.
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
    fn the_reader_rendering_gives_a_declared_record_its_name_and_its_row() {
        // A DECLARED record's name is inferred and carried (`infer_constructor`
        // builds `Record { name: owner, .. }`), but rendering dropped it, so a
        // function returning `Point` was reported as returning
        // `{ x: int, y: int }` — a spelling the author never wrote and, for a
        // record declared with `type`, not even a valid annotation.
        //
        // The name belongs in the READER rendering only: `Display` stays
        // structural because `check.rs` keys effect-row sites off it, and a
        // name-only key collapses `Box<int>` with `Box<string>`.
        let point = Type::Record {
            name: "Point".into(),
            fields: [
                ("x".to_string(), Type::int()),
                ("y".to_string(), Type::int()),
            ]
            .into_iter()
            .collect(),
        };
        assert_eq!(
            render_with_holes(&point),
            "Point { x: int, y: int }",
            "the reader gets the author's NAME and the row, because a record \
             carries no type arguments to put back and the row is where an \
             instantiation survives (#214)"
        );
        assert_eq!(
            point.to_string(),
            "{ x: int, y: int }",
            "Display stays faithful so it is safe to key off"
        );
        // An anonymous literal's row has no name to render, so it stays
        // structural in BOTH renderings — that spelling is the only
        // description it has.
        let anonymous = Type::Record {
            name: String::new(),
            fields: [("x".to_string(), Type::int())].into_iter().collect(),
        };
        assert_eq!(anonymous.to_string(), "{ x: int }");
        assert_eq!(render_with_holes(&anonymous), "{ x: int }");
    }

    #[test]
    fn a_partially_resolved_type_renders_its_proven_part_with_holes_for_the_rest() {
        // `fn bothArms(f) = if f { Success { value: 1 } } else { Error { .. } }`
        // infers `Result<int, e0>`: the payload is PROVEN, the error side is
        // free because `Error { message }` unifies with whichever error type
        // the call site supplies. Tooling had only two moves — render `t6`, an
        // unstable private name, or fall back to `Unit` — and it chose `Unit`,
        // a positive claim the checker itself refutes with
        // "cannot unify Unit with Result<t5, t6>". A hole says exactly what is
        // known and no more. Implements [TYPE-RENDER-HOLES].
        let partial = Type::Con {
            name: "Result".into(),
            args: vec![Type::int(), Type::Var(6)],
        };
        assert_eq!(render_with_holes(&partial), "Result<int, _>");
        // A fully resolved type is untouched — holes appear only where the
        // checker genuinely proved nothing.
        assert_eq!(render_with_holes(&Type::int()), "int");
        // Holes reach every nested position Display can walk.
        let nested = Type::Fun {
            params: vec![Type::Var(1)],
            ret: Box::new(Type::Con {
                name: "List".into(),
                args: vec![Type::Var(2)],
            }),
        };
        assert_eq!(render_with_holes(&nested), "(_) -> List<_>");
    }

    #[test]
    fn both_renderings_keep_two_instantiations_of_one_record_apart() {
        // `check.rs` keys effect-row perform/handler sites by rendering each
        // argument type with `Display` (check.rs:1011/1027/1050). Rendering a
        // nominal record as its bare NAME there made `Box<int>` and
        // `Box<string>` the same key, and a `Stash<Box<string>>` handler
        // discharged a `Stash<Box<int>>` perform with zero errors.
        //
        // So `Display` is the FAITHFUL rendering — it must distinguish whatever
        // the checker distinguishes — and the friendlier nominal spelling lives
        // in the reader path alone ([`render_with_holes`]). Anything that keys
        // off `Display` by accident is then still correct.
        let boxed = |inner: Type| Type::Record {
            name: "Box".into(),
            fields: [("value".to_string(), inner)].into_iter().collect(),
        };
        assert_ne!(
            boxed(Type::int()).to_string(),
            boxed(Type::string()).to_string(),
            "Display is an identity: two instantiations must not collide"
        );
        // The reader path keeps them apart too, and must: naming the record
        // alone was tried, and it lost the very instantiation `Display` is
        // being kept faithful to preserve.
        assert_eq!(render_with_holes(&boxed(Type::int())), "Box { value: int }");
        assert_eq!(
            render_with_holes(&boxed(Type::string())),
            "Box { value: string }"
        );
    }

    #[test]
    fn a_hole_is_a_display_spelling_and_not_a_parseable_type() {
        // [`HOLE`] is deliberately NOT wildcard syntax: the parser has no
        // wildcard, so `_` arrives here as an ordinary nominal type name and
        // unifies with nothing. Rendering must never be mistaken for producing
        // an annotation a reader can paste back, and if a wildcard is ever
        // added to the grammar this assertion is what says so.
        let hole = Type::prim(HOLE);
        assert_eq!(
            hole,
            Type::Con {
                name: HOLE.into(),
                args: Vec::new()
            }
        );
        assert_ne!(
            hole,
            Type::unit(),
            "a hole is not Unit, the claim it replaced"
        );
        assert!(
            !has_type_var(&hole),
            "a hole is a rendered NAME, not a variable: the variable it stands \
             for is gone by the time it exists"
        );
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
