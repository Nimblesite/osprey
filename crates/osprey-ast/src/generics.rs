//! Generic-declaration nodes shared by every statement form that binds type
//! parameters: variance-annotated parameters and effect references carrying
//! type arguments. Implements [TYPE-GENERICS-DECL] and [EFFECTS-GENERIC-ROWS].

use crate::{Position, TypeExpr};

/// Declaration-site variance of a type parameter. Implements
/// [TYPE-VARIANCE-DECL].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Variance {
    /// No annotation — the parameter must match exactly.
    #[default]
    Invariant,
    /// `out T` — the parameter may appear only in output (covariant) positions.
    Covariant,
    /// `in T` — the parameter may appear only in input (contravariant) positions.
    Contravariant,
}

/// One declared type parameter (`T`, `out T`, `in T`).
#[derive(Debug, Clone, PartialEq)]
pub struct TypeParam {
    /// The parameter name.
    pub name: String,
    /// Declared variance (`Invariant` when unannotated).
    pub variance: Variance,
}

impl TypeParam {
    /// An unannotated (invariant) type parameter.
    pub fn invariant(name: impl Into<String>) -> TypeParam {
        TypeParam {
            name: name.into(),
            variance: Variance::Invariant,
        }
    }
}

/// A reference to an effect in a function's effect row, optionally applied to
/// type arguments (`!Logger`, `!State<int>`). Implements [EFFECTS-GENERIC-ROWS].
#[derive(Debug, Clone, PartialEq)]
pub struct EffectRef {
    /// The effect's name.
    pub name: String,
    /// The type arguments the row instantiates the effect at (empty for a
    /// bare reference to a non-generic effect, or to let inference choose).
    pub type_args: Vec<TypeExpr>,
    /// Source position, when the parser recorded one.
    pub position: Option<Position>,
}

impl EffectRef {
    /// A bare, un-instantiated effect reference.
    pub fn named(name: impl Into<String>) -> EffectRef {
        EffectRef {
            name: name.into(),
            type_args: Vec::new(),
            position: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_invariant_and_bare() {
        assert_eq!(TypeParam::invariant("T").variance, Variance::Invariant);
        assert_eq!(Variance::default(), Variance::Invariant);
        let e = EffectRef::named("Logger");
        assert!(e.type_args.is_empty());
        assert_eq!(e.name, "Logger");
    }
}
