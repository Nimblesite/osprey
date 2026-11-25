//! How the per-file lint tests read a lint: parse the source in one flavor,
//! run the lint, and keep each warning's message and line once its rule is
//! confirmed.

use crate::TypeWarning;
use osprey_ast::Program;
use osprey_syntax::{parse_program_with_flavor, Flavor};

pub(crate) fn warned(
    flavor: Flavor,
    source: &str,
    check: fn(&Program) -> Vec<TypeWarning>,
    rule: &str,
) -> Vec<(String, u32)> {
    let parsed = parse_program_with_flavor(source, flavor);
    assert!(parsed.errors.is_empty(), "{source}\n{:?}", parsed.errors);
    check(&parsed.program)
        .into_iter()
        .map(|warning| {
            assert_eq!(warning.rule, rule);
            let line = warning.position.map_or(0, |position| position.line);
            (warning.message, line)
        })
        .collect()
}
