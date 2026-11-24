//! Helpers the TypeScript contract tests share.

use osprey_syntax::{parse_program_with_flavor, Flavor};

/// Asserts `source` is rejected by a diagnostic containing `needle` at the
/// one-based `line` and zero-based `column` of the construct it names.
pub(crate) fn rejected_at(source: &str, needle: &str, line: u32, column: u32) {
    let parsed = parse_program_with_flavor(source, Flavor::TypeScript);
    let found = parsed
        .errors
        .iter()
        .find(|error| error.message.contains(needle))
        .map(|error| (error.position.line, error.position.column));
    assert_eq!(
        found,
        Some((line, column)),
        "{source}\nexpected `{needle}`, got {:?}",
        parsed.errors
    );
}
