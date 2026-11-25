//! Runs of closing brackets: eight or more `)`, `}` or `]` on one line with
//! only spaces between them. A callback pyramid always ends in one, and a flat
//! `use` block never does. Implements [LINT-NESTING-LAMBDA].

use crate::Flavor;
use osprey_ast::Position;

/// The shortest run of closing brackets that marks code nested too deep.
pub const MIN_CLOSING_RUN: usize = 8;

/// One run of closing brackets: where its first bracket is and how many there are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClosingRun {
    /// The first bracket of the run (1-based line, 0-based column).
    pub position: Position,
    /// How many brackets the run holds.
    pub length: usize,
}

/// Every run of at least [`MIN_CLOSING_RUN`] closing brackets in `source`.
/// Brackets inside strings and comments are text, so only real tokens count.
/// The Python and TypeScript flavors keep their own conventions and are exempt.
#[must_use]
pub fn closing_runs(source: &str, flavor: Flavor) -> Vec<ClosingRun> {
    let closers = match flavor {
        Flavor::Default => crate::default::bracket_tokens(source),
        Flavor::Ml => crate::ml::bracket_tokens(source),
        _ => Vec::new(),
    };
    runs(&closers)
}

/// One token as the run counter sees it.
#[derive(Debug, Clone, Copy)]
pub(crate) enum BracketToken {
    /// A closing bracket at this position.
    Closer(Position),
    /// Any other token: it ends the current run.
    Other,
}

fn runs(tokens: &[BracketToken]) -> Vec<ClosingRun> {
    let mut found = Vec::new();
    let mut current: Option<ClosingRun> = None;
    for token in tokens.iter().chain(std::iter::once(&BracketToken::Other)) {
        current = match (*token, current) {
            (BracketToken::Closer(position), Some(run)) if position.line == run.position.line => {
                Some(ClosingRun {
                    length: run.length + 1,
                    ..run
                })
            }
            (BracketToken::Closer(position), run) => {
                found.extend(run.filter(is_long));
                Some(ClosingRun {
                    position,
                    length: 1,
                })
            }
            (BracketToken::Other, run) => {
                found.extend(run.filter(is_long));
                None
            }
        };
    }
    found
}

fn is_long(run: &ClosingRun) -> bool {
    run.length >= MIN_CLOSING_RUN
}
