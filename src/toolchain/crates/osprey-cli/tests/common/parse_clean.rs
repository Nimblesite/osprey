//! Parsing a test fixture that must parse: one helper for every suite that
//! asserts its source is free of syntax errors before checking anything else.

use osprey_syntax::{parse_program_with_flavor, Flavor, Parsed};

/// Parses `source` in `flavor`, asserting it parses, `label` naming the fixture.
pub(crate) fn parse_clean(source: &str, flavor: Flavor, label: &str) -> Parsed {
    let parsed = parse_program_with_flavor(source, flavor);
    assert!(parsed.errors.is_empty(), "{label}: {:?}", parsed.errors);
    parsed
}
