//! The **Default flavor** frontend: C-style braces, parens-and-named-argument
//! calls, explicit currying — the language of specs 0001–0022. It parses with
//! the embedded **tree-sitter** grammar and lowers that CST to the canonical
//! [`osprey_ast::Program`] with an explicit recursive descent over named nodes
//! (no visitor plumbing, exhaustive matching).
//!
//! This is one of two sibling flavor folders ([`crate::ml`] is the other); both
//! converge on the same AST, after which nothing may tell them apart
//! ([FLAVOR-BOUNDARY], docs/specs/0023-LanguageFlavors.md). Errors are
//! collected, never fatal: the frontend never panics on bad input and always
//! produces a best-effort tree.

use crate::{Flavor, Parsed, SyntaxError};
use osprey_ast::{Position, Program};
use tree_sitter::{Node, Parser, Point, Tree};

mod expr;
mod lower;

pub use lower::Lowerer;

/// The Default (brace) frontend: tree-sitter CST + [`Lowerer`] → [`Program`].
pub(crate) fn parse(source: &str) -> Parsed {
    let Some(tree) = parse_tree(source) else {
        return Parsed {
            program: Program {
                statements: Vec::new(),
            },
            errors: vec![SyntaxError {
                message: "failed to initialize Osprey grammar".to_owned(),
                position: Position { line: 1, column: 0 },
            }],
            flavor: Flavor::Default,
        };
    };
    let root = tree.root_node();
    let lowerer = Lowerer::new(source.as_bytes());
    let program = lowerer.lower_program(root);
    let mut errors = Vec::new();
    collect_errors(root, source.as_bytes(), &mut errors);
    Parsed {
        program,
        errors,
        flavor: Flavor::Default,
    }
}

/// Run only the tree-sitter parse (used by tooling that wants the raw CST).
///
/// Returns [`None`] if the embedded Osprey grammar cannot be loaded or
/// tree-sitter declines to produce a tree (neither happens for a valid build).
#[must_use]
pub fn parse_tree(source: &str) -> Option<Tree> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_osprey::LANGUAGE.into())
        .ok()?;
    parser.parse(source, None)
}

fn collect_errors(node: Node<'_>, src: &[u8], out: &mut Vec<SyntaxError>) {
    if node.is_error() || node.is_missing() {
        let p = node.start_position();
        out.push(SyntaxError {
            message: if node.is_missing() {
                format!("missing {}", node.kind())
            } else {
                format!("syntax error near {:?}", node.utf8_text(src).unwrap_or(""))
            },
            position: position_from_point(p),
        });
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_errors(child, src, out);
    }
}

/// Convert a tree-sitter point to Osprey's one-based-line source position.
pub(crate) fn position_from_point(point: Point) -> Position {
    Position {
        line: u32::try_from(point.row)
            .unwrap_or(u32::MAX)
            .saturating_add(1),
        column: u32::try_from(point.column).unwrap_or(u32::MAX),
    }
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    reason = "test assertions: an out-of-bounds index is a test failure, not a production panic"
)]
mod tests {
    use crate::parse_program;
    use osprey_ast::{Expr, Pattern, Stmt};

    fn one(src: &str) -> Stmt {
        let parsed = parse_program(src);
        assert!(parsed.errors.is_empty(), "errors: {:?}", parsed.errors);
        assert_eq!(parsed.program.statements.len(), 1);
        parsed.program.statements.into_iter().next().unwrap()
    }

    #[test]
    fn lowers_doc_comments_on_let_and_function() {
        // A `///` block above a binding is captured as its `doc`, stripped of the
        // markers, and the recorded position stays on the declaration keyword/name
        // (line 3 here), not the comment lines. Implements [LSP-HOVER-DOCS]
        match one(
            "/// The retry budget.\n/// Bounded above by `maxRetries`.\nlet retries: int = 3\n",
        ) {
            Stmt::Let {
                name,
                doc,
                position,
                ..
            } => {
                assert_eq!(name, "retries");
                // The multi-line doc is one paragraph → the whole summary.
                let d = doc.as_ref().expect("doc present");
                assert_eq!(
                    d.summary,
                    "The retry budget. Bounded above by `maxRetries`."
                );
                assert_eq!(position.map(|p| p.line), Some(3));
            }
            s => panic!("expected let, got {s:?}"),
        }
        match one("/// Adds two ints.\nfn add(a: int, b: int) -> int = a + b\n") {
            Stmt::Function { doc, position, .. } => {
                assert_eq!(
                    doc.as_ref().map(|d| d.summary.clone()).as_deref(),
                    Some("Adds two ints.")
                );
                assert_eq!(position.map(|p| p.line), Some(2));
            }
            s => panic!("expected function, got {s:?}"),
        }
        // An undocumented binding carries no doc.
        match one("let x = 1\n") {
            Stmt::Let { doc, .. } => assert_eq!(doc, None),
            s => panic!("expected let, got {s:?}"),
        }
    }

    #[test]
    fn lowers_let() {
        match one("let x = 42\n") {
            Stmt::Let {
                name,
                value,
                mutable,
                ..
            } => {
                assert_eq!(name, "x");
                assert!(!mutable);
                assert_eq!(value, Expr::Integer(42));
            }
            s => panic!("expected let, got {s:?}"),
        }
    }

    #[test]
    fn lowers_function_with_binary_body() {
        match one("fn add(a: int, b: int) -> int = a + b\n") {
            Stmt::Function {
                name,
                parameters,
                return_type,
                body,
                ..
            } => {
                assert_eq!(name, "add");
                assert_eq!(parameters.len(), 2);
                assert_eq!(parameters[0].name, "a");
                assert_eq!(return_type.unwrap().name, "int");
                match body {
                    Expr::Binary { op, .. } => assert_eq!(op, "+"),
                    b => panic!("expected binary, got {b:?}"),
                }
            }
            s => panic!("expected function, got {s:?}"),
        }
    }

    #[test]
    fn lowers_union_type() {
        match one("type Color = Red | Green | Blue\n") {
            Stmt::Type { name, variants, .. } => {
                assert_eq!(name, "Color");
                assert_eq!(variants.len(), 3);
                assert_eq!(variants[2].name, "Blue");
            }
            s => panic!("expected type, got {s:?}"),
        }
    }

    #[test]
    fn lowers_extern_with_ptr() {
        match one("extern fn sqlite3_open(filename: string, ppDb: Ptr) -> int\n") {
            Stmt::Extern {
                name,
                parameters,
                return_type,
                ..
            } => {
                assert_eq!(name, "sqlite3_open");
                assert_eq!(parameters.len(), 2);
                assert_eq!(parameters[1].ty.name, "Ptr");
                assert_eq!(return_type.unwrap().name, "int");
            }
            s => panic!("expected extern, got {s:?}"),
        }
    }

    #[test]
    fn lowers_match() {
        match one("let r = match x {\n  Ok { value } => value\n  _ => 0\n}\n") {
            Stmt::Let {
                value: Expr::Match { arms, .. },
                ..
            } => {
                assert_eq!(arms.len(), 2);
                assert!(matches!(arms[1].pattern, Pattern::Wildcard));
            }
            s => panic!("expected let-match, got {s:?}"),
        }
    }

    #[test]
    fn lowers_effect_and_perform() {
        let parsed = parse_program(
            "effect Log { info: fn(string) -> Unit }\nfn go() = perform Log.info(msg: \"hi\")\n",
        );
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        assert!(matches!(parsed.program.statements[0], Stmt::Effect { .. }));
    }

    #[test]
    fn reports_syntax_error() {
        let parsed = parse_program("fn (= \n");
        assert!(!parsed.errors.is_empty());
    }

    #[test]
    fn reports_missing_node_error() {
        // `type T =` with no variant name forces tree-sitter to insert a MISSING
        // identifier; collect_errors reports it via the is_missing format branch.
        let parsed = parse_program("type T =\n");
        assert!(
            parsed
                .errors
                .iter()
                .any(|e| e.message.starts_with("missing")),
            "expected a missing-node error, got {:?}",
            parsed.errors
        );
        // The error carries a 1-based line.
        assert!(parsed.errors[0].position.line >= 1);
    }
}
