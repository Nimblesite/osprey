//! Helpers the syntax contract tests share.

use osprey_syntax::{parse_program_with_flavor, Flavor};

/// The canonical AST of `source` with every position scrubbed; a syntax error
/// fails the test that asked for it.
pub(crate) fn canonical(source: &str, flavor: Flavor) -> String {
    let parsed = parse_program_with_flavor(source, flavor);
    assert!(
        parsed.errors.is_empty(),
        "{flavor} syntax errors: {:?}",
        parsed.errors
    );
    osprey_ast::canonical::without_positions(&parsed.program)
}
