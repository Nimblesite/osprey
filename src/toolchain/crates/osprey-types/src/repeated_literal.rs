//! Repeated literals. Implements [TYPE-WARNINGS-REPEATED-LITERAL].
//!
//! A long string, or a list of literals, written again and again in one file
//! is a constant that was never named: a change to it must be made at every
//! copy, and a copy that is missed drifts silently. Duplication is a bug, so
//! a literal of at least [`MIN_CHARS`] characters written at least
//! [`MIN_COUNT`] times is reported once, at its first use. Pattern literals
//! are not expressions (a pattern cannot name a constant), so they never count.

use std::collections::BTreeMap;

use osprey_ast::{
    stmt_meta::statement_position, walk_program, AstVisitor, Expr, Position, Program, Stmt,
};

use crate::TypeWarning;

/// Shared rule identifier for compiler and editor diagnostics.
pub const REPEATED_LITERAL: &str = "repeated-literal";

/// The shortest literal, as written, that is worth naming.
const MIN_CHARS: usize = 12;

/// How many copies make a literal a constant that was never named.
const MIN_COUNT: usize = 3;

/// How much of a long literal the message quotes.
const SHOWN_CHARS: usize = 32;

/// Every literal of `program` written [`MIN_COUNT`] or more times, in the
/// order of each one's first use.
#[must_use]
pub fn repeated_literals(program: &Program) -> Vec<TypeWarning> {
    let mut literals = Literals::default();
    walk_program(program, &mut literals);
    let mut repeated: Vec<&Seen> = literals
        .seen
        .values()
        .filter(|seen| seen.count >= MIN_COUNT)
        .collect();
    repeated.sort_by_key(|seen| seen.order);
    repeated
        .into_iter()
        .map(|seen| TypeWarning {
            message: format!(
                "the literal `{}` is written {} times in this file; name it once as a constant",
                shortened(&seen.spelled),
                seen.count
            ),
            position: seen.first,
            rule: REPEATED_LITERAL,
        })
        .collect()
}

/// One literal: how often it is written, and where first.
struct Seen {
    spelled: String,
    count: usize,
    first: Option<Position>,
    order: usize,
}

/// The literals met so far, keyed by their Default spelling, and the position
/// of the innermost statement around the walk (a string carries none).
#[derive(Default)]
struct Literals {
    seen: BTreeMap<String, Seen>,
    at: Option<Position>,
}

impl AstVisitor for Literals {
    fn statement(&mut self, statement: &Stmt) {
        if let Some(position) = statement_position(statement) {
            self.at = Some(position);
        }
    }

    fn expression(&mut self, expression: &Expr) {
        let at = match expression {
            Expr::List(_, Some(position)) => Some(*position),
            _ => self.at,
        };
        if let Some(spelled) =
            spelling(expression).filter(|_| written_chars(expression) >= MIN_CHARS)
        {
            self.record(spelled, at);
        }
    }
}

impl Literals {
    fn record(&mut self, spelled: String, at: Option<Position>) {
        let order = self.seen.len();
        let seen = self.seen.entry(spelled.clone()).or_insert(Seen {
            spelled,
            count: 0,
            first: at,
            order,
        });
        seen.count += 1;
    }
}

/// A literal as Default writes it: a string, a number, a bool, or a
/// non-empty list of those. Anything that computes is not a literal.
fn spelling(expression: &Expr) -> Option<String> {
    match expression {
        Expr::Str(text) => Some(format!("{text:?}")),
        Expr::List(items, _) if !items.is_empty() => {
            let items: Option<Vec<String>> = items.iter().map(scalar).collect();
            items.map(|items| format!("[{}]", items.join(", ")))
        }
        _ => None,
    }
}

/// The characters a reader sees: a string's text without its quotes, or
/// a list as written.
fn written_chars(expression: &Expr) -> usize {
    match expression {
        Expr::Str(text) => text.chars().count(),
        _ => spelling(expression).map_or(0, |spelled| spelled.chars().count()),
    }
}

fn scalar(expression: &Expr) -> Option<String> {
    match expression {
        Expr::Str(text) => Some(format!("{text:?}")),
        Expr::Integer(value) => Some(value.to_string()),
        Expr::Float(value) => Some(format!("{value:?}")),
        Expr::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn shortened(spelled: &str) -> String {
    if spelled.chars().count() <= SHOWN_CHARS {
        return spelled.to_owned();
    }
    let head: String = spelled.chars().take(SHOWN_CHARS).collect();
    format!("{head}…")
}
