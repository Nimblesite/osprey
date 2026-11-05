//! What the comparison operators are decided for. `==` and `!=` compare
//! scalars and payload-free union variants; the ordering operators compare
//! scalars. Every other operand type is a compile error because its meaning is
//! not decided. Implements [BOOL-EQUALITY] and [BOOL-ORDERING]
//! (docs/specs/language/0009-BooleanOperations.md).
//!
//! The requirement is an obligation on the operand type, checked after
//! inference like a built-in's representation constraint, so a generic
//! function carries it to each call site and each is checked at its own type.

use crate::check::Checker;
use crate::ty::{names, Type};
use osprey_ast::{Expr, Position};

/// Obligation key prefix for a comparison operand. The space keeps it from
/// colliding with a source identifier or a built-in name.
pub(crate) const COMPARED_OPERAND_PREFIX: &str = "compared ";

impl Checker {
    /// Require `operand`, the one type both sides of `op` unified to, to have a
    /// decided meaning for `op`. A variant test (`shape == Dot`) reads only the
    /// variant, so it is decided whatever the union carries and records nothing.
    pub(crate) fn constrain_comparison(
        &mut self,
        op: &str,
        sides: [&Expr; 2],
        operand: &Type,
        position: Option<Position>,
    ) {
        if is_equality(op) && sides.iter().any(|side| self.is_variant_test(side)) {
            return;
        }
        let name = self.located_use(&format!("{COMPARED_OPERAND_PREFIX}{op}"), position);
        self.builtin_uses.push((name, operand.clone()));
    }

    /// `listContains` compares its needle with each element by `==`, so the
    /// element type needs a decided equality exactly as `==` does.
    /// Implements [BUILTIN-LIST-CONTAINS].
    pub(crate) fn constrain_named_comparison(&mut self, name: Option<&str>, args: &[Type]) {
        if let (Some(LIST_CONTAINS), Some(needle)) = (name, args.get(1)) {
            let name = self.sited_use(&format!("{COMPARED_OPERAND_PREFIX}{LIST_CONTAINS}"));
            self.builtin_uses.push((name, needle.clone()));
        }
    }

    /// The diagnostic for a comparison whose operand type has no decided
    /// meaning; `None` for any other obligation, or a decided comparison. An
    /// unresolved variable is left alone: nothing says what it is.
    pub(crate) fn undecided_comparison(&self, name: &str, ty: &Type) -> Option<String> {
        let op = name.strip_prefix(COMPARED_OPERAND_PREFIX)?;
        let decided = matches!(ty, Type::Var(_))
            || is_scalar(ty)
            || (is_equality(op) && self.is_payload_free_union(ty));
        (!decided).then(|| undecided_message(op, ty))
    }

    fn is_variant_test(&self, side: &Expr) -> bool {
        matches!(side, Expr::Identifier(name) if self.is_payload_free_variant(name))
    }

    fn is_payload_free_variant(&self, name: &str) -> bool {
        self.ctors
            .get(name)
            .is_some_and(|ctor| !ctor.owner_is_record && ctor.fields.is_empty())
    }

    /// A union none of whose variants carries a payload: its values are their
    /// variants, so comparing them compares variants.
    fn is_payload_free_union(&self, ty: &Type) -> bool {
        let Type::Con { name, .. } = ty else {
            return false;
        };
        self.union_variants.get(name).is_some_and(|variants| {
            variants
                .iter()
                .all(|variant| self.is_payload_free_variant(variant))
        })
    }
}

const LIST_CONTAINS: &str = "listContains";

fn is_equality(op: &str) -> bool {
    matches!(op, "==" | "!=" | LIST_CONTAINS)
}

fn is_scalar(ty: &Type) -> bool {
    crate::numeric::Numeric::of_type(ty).is_some()
        || ty.is_named(names::STRING)
        || ty.is_named(names::BOOL)
}

fn undecided_message(op: &str, ty: &Type) -> String {
    if is_equality(op) {
        format!(
            "cannot compare `{ty}` with `{op}`: equality is decided only for int, float, \
             string, bool and payload-free union variants; compare a key, id or revision \
             field, or call `structuralEquals(a, b)` for a deliberate deep comparison"
        )
    } else {
        format!(
            "cannot order `{ty}` with `{op}`: ordering is decided only for int, float, \
             string and bool"
        )
    }
}

#[cfg(test)]
mod tests;
