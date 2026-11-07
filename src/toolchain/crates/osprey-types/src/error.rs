//! Type errors as plain located messages — the CLI prints `file:line:col:
//! message`, identical to the syntax-error path.

use crate::ty::{names, Type};
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
            message: readable(message),
            position: None,
        }
    }

    /// Create an error with a message anchored to a source position.
    pub fn at(message: impl Into<String>, position: Position) -> TypeError {
        TypeError {
            message: readable(message),
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

    /// Build an error for two types that fail to unify. A `Result` met where
    /// its own payload was expected, as programs written against the HTTP
    /// externs' old `int` returns do, names how to decode it ([FAILURE-EXPLICIT]).
    #[must_use]
    pub(crate) fn mismatch(a: &Type, b: &Type) -> TypeError {
        let message = format!("type mismatch: cannot unify {a} with {b}");
        TypeError::new(match payload_of(b, a) {
            Some(payload) => format!("{message}; decode the `Result` with `match` or `?:` before using it as `{payload}`"),
            None => message,
        })
    }

    /// Build an error for a type that recursively contains itself (occurs check).
    #[must_use]
    pub(crate) fn recursive(a: &Type, b: &Type) -> TypeError {
        TypeError::new(format!("recursive type: {a} occurs in {b}"))
    }
}

/// `wanted` when `result` is a `Result` whose success type is exactly it.
fn payload_of<'t>(result: &Type, wanted: &'t Type) -> Option<&'t Type> {
    match result {
        Type::Con { name, args } if name == names::RESULT && args.first() == Some(wanted) => {
            Some(wanted)
        }
        _ => None,
    }
}

/// Restore source names in a message before it is stored.
///
/// A declaration inside a namespace reaches the checker under the flat linkage
/// name project assembly gave it, so a diagnostic naming a function would
/// otherwise read `__osp_4x62616e6b_5x7365727665` instead of `bank::serve`.
/// Doing this once at construction covers every consumer — CLI, LSP, tests.
fn readable(message: impl Into<String>) -> String {
    match osprey_ast::symbol::demangle_message(&message.into()) {
        std::borrow::Cow::Borrowed(text) => text.to_owned(),
        std::borrow::Cow::Owned(text) => text,
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
