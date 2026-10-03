//! Test-only helpers shared by the editor-feature unit tests.
//!
//! Every feature test opens the same way: parse a snippet, fail loudly on any
//! syntax error, then ask one editor question about a cursor position. Each
//! `mod tests` used to carry its own copy of that preamble, so the copies
//! drifted apart on their failure messages while asserting the same thing.

use osprey_ast::Program;

/// The feature tests' shared document: one fully annotated function and one
/// call. `satAdd` is total ([ARITH-EFFECT-TOTAL-HELPERS]), so the file-scope
/// call needs no `Arith` policy and the program type-checks with exactly its
/// three redundant-annotation warnings.
pub(crate) const ADD_SRC: &str =
    "fn add(a: int, b: int) -> int = satAdd(a, b)\nlet total = add(1, 2)\n";

/// Parse `src`, asserting it is syntactically valid. A snippet the PARSER
/// rejects must never be scored as a passing editor view.
pub(crate) fn parsed(src: &str) -> Program {
    let parsed = osprey_syntax::parse_program(src);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    parsed.program
}

/// The symbol view of a valid snippet — the rendering the inference-view tests
/// inspect.
pub(crate) fn symbols(src: &str) -> String {
    crate::analysis::symbols_json(&parsed(src))
}

/// The 0-based column just inside the first occurrence of `needle` on 0-based
/// `line` of `src` — a cursor position over that word.
pub(crate) fn col_of(src: &str, line: usize, needle: &str) -> u32 {
    let text = src.lines().nth(line).expect("line exists");
    let at = text.find(needle).expect("needle on line");
    u32::try_from(at).expect("column fits") + 1
}
