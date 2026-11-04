//! A `//!` written where no scope can hold it. Implements [DOC-SIGIL-INNER].
//!
//! The grammar's `line_comment` token may be a bare `//` — an empty comment is
//! legal — and it refuses a `!` straight after the slashes, so that `//!` stays
//! the inner-doc token. Where the parser cannot take an inner doc, the lexer
//! falls back to what it can take: the two-character comment `//`, then `!` as
//! the NOT operator, then the rest of the line as CODE. `//! ready` at the end of
//! a block compiled as `!ready` and became the block's value, so a comment
//! silently changed what a program printed. A bare `//` comment immediately
//! followed by `!` can only be a written `//!`, so that shape is reported here
//! with the error the ML frontend gives, and the errors the misread line raised
//! are dropped: that text is a comment, and naming it as code is not truthful.

use super::position_from_point;
use crate::SyntaxError;
use osprey_ast::Position;
use tree_sitter::Node;

/// How a Default-flavor author documents the declaration that follows.
const OUTER_SIGIL: &str = "///";

/// Replace what a misread `//!` line produced with the one error that names it.
pub(super) fn reclassify(root: Node<'_>, src: &[u8], errors: &mut Vec<SyntaxError>) {
    for position in misplaced(root, src) {
        errors.retain(|error| !on_the_comment(error.position, position));
        let at = errors
            .iter()
            .position(|error| {
                (error.position.line, error.position.column) > (position.line, position.column)
            })
            .unwrap_or(errors.len());
        errors.insert(
            at,
            SyntaxError {
                message: crate::docparse::misplaced_inner_doc(OUTER_SIGIL),
                position,
            },
        );
    }
}

/// Where each `//!` the lexer split into `//` and `!` begins, in source order.
fn misplaced(root: Node<'_>, src: &[u8]) -> Vec<Position> {
    let mut found = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if is_split_inner_doc(node, src) {
            found.push(position_from_point(node.start_position()));
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
    found.sort_by_key(|position| (position.line, position.column));
    found
}

/// A comment that is exactly `//` with a `!` as the very next byte.
fn is_split_inner_doc(node: Node<'_>, src: &[u8]) -> bool {
    node.kind() == "line_comment"
        && node.end_byte().saturating_sub(node.start_byte()) == 2
        && src.get(node.end_byte()) == Some(&b'!')
}

/// An error raised on the rest of the misread line, which is comment text.
fn on_the_comment(error: Position, comment: Position) -> bool {
    error.line == comment.line && error.column >= comment.column
}

#[cfg(test)]
mod tests {
    use crate::parse_program_with_flavor;
    use crate::Flavor;

    fn errors(source: &str) -> Vec<(u32, u32, String)> {
        parse_program_with_flavor(source, Flavor::Default)
            .errors
            .into_iter()
            .map(|error| (error.position.line, error.position.column, error.message))
            .collect()
    }

    #[test]
    fn a_misplaced_inner_doc_is_named_even_where_its_text_parses_as_code() {
        // [DOC-SIGIL-INNER] `ready` is valid code; the line still must not be.
        let found = errors("fn check(ready) = {\n    print(\"x\")\n    //! ready\n}\n");
        assert!(
            matches!(
                found.as_slice(),
                [(3, 4, message)]
                    if message.starts_with("`//!` documents the enclosing") && message.contains("`///`")
            ),
            "expected exactly one `//!` error at 3:4 naming the Default sigil `///`: {found:?}"
        );
    }

    #[test]
    fn ordinary_and_legal_comments_are_left_alone() {
        // An empty `//`, a `// !` with a space, and a `//!` opening the file
        // are all legal; none is the split shape.
        for source in [
            "//\nlet a = 1\n",
            "// !important\nlet a = 1\n",
            "//! The file.\nlet a = 1\n",
            "let a = \"//! not a comment\"\n",
        ] {
            assert!(
                errors(source).is_empty(),
                "{source:?}: {:?}",
                errors(source)
            );
        }
    }

    #[test]
    fn errors_the_program_already_had_survive_a_misplaced_inner_doc() {
        // Only the misread line's own errors are replaced; every error the
        // same program raises without the `//!` must still be reported.
        let baseline = errors("let a = )\nlet b = 1\n");
        assert!(!baseline.is_empty(), "the fixture must already be broken");
        let found = errors("let a = )\nlet b = 1 //! note\n");
        for error in &baseline {
            assert!(found.contains(error), "{error:?} was lost: {found:?}");
        }
        assert!(
            found
                .iter()
                .any(|error| error.2.starts_with("`//!`") && error.0 == 2),
            "{found:?}"
        );
    }
}
