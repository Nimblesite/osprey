//! Position-blind program rendering.
//!
//! A [`Position`](crate::Position) records *where* a node was written, never
//! what it means, yet it participates in `Program`'s derived `PartialEq`. Two
//! callers need to ask whether two ASTs say the same thing while allowing them
//! to sit at different coordinates:
//!
//! * the formatter's meaning-preservation guard, whose entire purpose is to
//!   move nodes — comparing positions there rejects exactly the reindentation
//!   the formatter exists to perform;
//! * the cross-flavor equivalence tests, where the same program written in the
//!   brace and layout surfaces necessarily lands on different lines.
//!
//! Both get [`without_positions`] rather than a hand-written traversal, which
//! would have to be extended every time a node gains a position and would fail
//! silently — as a stale comparison — when it was not.

use crate::Program;

/// Render `program` with every source coordinate removed, so two renderings
/// compare equal exactly when the programs mean the same thing.
///
/// The pretty-`Debug` form puts each field on its own line and escapes string
/// contents onto a single line, so the shapes below are unambiguous: a string
/// literal always shares its line with its own quotes and can never be mistaken
/// for one of them.
///
/// Both the *coordinates* and the *presence* of a position are erased —
/// `position: None` and `position: Some(Position { … })` render alike — because
/// whether a frontend happened to record a span is metadata about parsing, not
/// about meaning. The one shape left alone is a bare `None` in a tuple variant,
/// which is indistinguishable from any other absent field; a divergence there
/// is reported rather than assumed benign.
#[must_use]
pub fn without_positions(program: &Program) -> String {
    let dump = format!("{program:#?}");
    let lines: Vec<&str> = dump.lines().collect();
    let mut out = Vec::with_capacity(lines.len());
    let mut index = 0;
    while let Some(line) = lines.get(index) {
        if let Some(next) = position_field(&lines, index) {
            out.push(format!("{}position: <erased>", " ".repeat(indent_of(line))));
            index = next;
        } else {
            out.push((*line).to_owned());
            index = index.saturating_add(1);
        }
    }
    out.join("\n")
}

/// If the line at `index` opens a position field, the index just past it.
///
/// Three spellings reach here: the absent named field, the present named field,
/// and the present *tuple* field, whose `Some(` carries no field name and so is
/// recognised by the `Position {` on the following line.
fn position_field(lines: &[&str], index: usize) -> Option<usize> {
    let trimmed = lines.get(index)?.trim_start();
    if trimmed == "position: None," {
        return Some(index.saturating_add(1));
    }
    let opens = trimmed == "position: Some(" || trimmed == "Some(";
    let holds_position = lines
        .get(index.saturating_add(1))
        .is_some_and(|next| next.trim_start() == "Position {");
    (opens && holds_position).then(|| closer(lines, index))?
}

/// The index just past the `),` closing the `Some(` at `index`: the pretty form
/// indents every nested line further, so the first close back at that line's own
/// column is the matching one.
fn closer(lines: &[&str], index: usize) -> Option<usize> {
    let indent = indent_of(lines.get(index)?);
    lines
        .iter()
        .enumerate()
        .skip(index.saturating_add(1))
        .find(|(_, line)| line.trim_start() == ")," && indent_of(line) == indent)
        .map(|(at, _)| at.saturating_add(1))
}

/// Width of a line's leading whitespace.
fn indent_of(line: &str) -> usize {
    line.len().saturating_sub(line.trim_start().len())
}

#[cfg(test)]
mod tests {
    use super::without_positions;
    use crate::{Expr, Position, Program, Stmt};

    fn let_at(value: &str, position: Option<Position>) -> Stmt {
        Stmt::Let {
            name: "a".to_owned(),
            mutable: false,
            ty: None,
            value: Expr::Str(value.to_owned()),
            doc: None,
            position,
        }
    }

    #[test]
    fn the_same_program_at_different_coordinates_renders_identically() {
        let here = Program {
            statements: vec![let_at("x", Some(Position { line: 1, column: 0 }))],
        };
        let there = Program {
            statements: vec![let_at(
                "x",
                Some(Position {
                    line: 90,
                    column: 12,
                }),
            )],
        };
        assert_ne!(here, there, "the derived equality does compare positions");
        assert_eq!(without_positions(&here), without_positions(&there));
    }

    #[test]
    fn a_difference_that_is_not_a_coordinate_survives() {
        let one = Program {
            statements: vec![let_at("x", None)],
        };
        let other = Program {
            statements: vec![let_at("y", None)],
        };
        assert_ne!(without_positions(&one), without_positions(&other));
    }

    /// The hazard the line-oriented filter has to survive: a program whose own
    /// string data is spelled like a coordinate. `{:#?}` keeps the literal on
    /// one line with its quotes, so it is never mistaken for one.
    #[test]
    fn a_string_literal_spelled_like_a_coordinate_is_not_stripped() {
        let literal = "line: 42,\ncolumn: 7,";
        let program = Program {
            statements: vec![let_at(literal, None)],
        };
        let other = Program {
            statements: vec![let_at("column: 7,\nline: 42,", None)],
        };
        assert!(without_positions(&program).contains("line: 42"));
        assert_ne!(without_positions(&program), without_positions(&other));
    }

    /// A frontend that records a span and one that does not must still compare
    /// equal: whether a position was captured is a fact about parsing.
    #[test]
    fn a_recorded_position_and_an_absent_one_render_alike() {
        let recorded = Program {
            statements: vec![let_at("x", Some(Position { line: 3, column: 1 }))],
        };
        let absent = Program {
            statements: vec![let_at("x", None)],
        };
        assert_eq!(without_positions(&recorded), without_positions(&absent));
        assert!(
            !without_positions(&recorded).contains("Position"),
            "{}",
            without_positions(&recorded)
        );
    }
}
