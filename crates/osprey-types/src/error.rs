//! Type errors as plain located messages — the CLI prints `file:line:col:
//! message`, identical to the syntax-error path.

use crate::ty::Type;
use osprey_ast::Position;

/// A single type error with an optional source position.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeError {
    /// Human-readable description of what went wrong.
    pub message: String,
    /// Source location the error refers to, when known.
    pub position: Option<Position>,
}

impl TypeError {
    /// Create an error with a message but no associated position.
    pub(crate) fn new(message: impl Into<String>) -> TypeError {
        TypeError {
            message: message.into(),
            position: None,
        }
    }

    /// Create an error with a message anchored to a source position.
    pub fn at(message: impl Into<String>, position: Position) -> TypeError {
        TypeError {
            message: message.into(),
            position: Some(position),
        }
    }

    /// Attach a position if one is known and none is set yet.
    #[must_use]
    pub(crate) fn with_pos(mut self, position: Option<Position>) -> TypeError {
        if self.position.is_none() {
            self.position = position;
        }
        self
    }

    /// Build an error for two types that fail to unify.
    #[must_use]
    pub(crate) fn mismatch(a: &Type, b: &Type) -> TypeError {
        TypeError::new(format!("type mismatch: cannot unify {a} with {b}"))
    }

    /// Build an error for a type that recursively contains itself (occurs check).
    #[must_use]
    pub(crate) fn recursive(a: &Type, b: &Type) -> TypeError {
        TypeError::new(format!("recursive type: {a} occurs in {b}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_anchors_to_a_position_and_with_pos_keeps_first() {
        let pos = Position { line: 3, column: 7 };
        let e = TypeError::at("boom", pos);
        assert_eq!(e.message, "boom");
        assert_eq!(e.position, Some(pos));
        // `with_pos` is a no-op once a position is set.
        let other = Position { line: 9, column: 1 };
        let e = e.with_pos(Some(other));
        assert_eq!(e.position, Some(pos));
        // `new` has no position; `with_pos` then fills it.
        let filled = TypeError::new("x").with_pos(Some(other));
        assert_eq!(filled.position, Some(other));
    }
}
