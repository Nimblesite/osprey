//! The **Default flavor** frontend: C-style braces, parens-and-named-argument
//! calls, explicit currying — the language of specs 0001–0022. It parses with
//! the embedded **tree-sitter** grammar and lowers that CST to the canonical
//! [`osprey_ast::Program`] with an explicit recursive descent over named nodes
//! (no visitor plumbing, exhaustive matching).
//!
//! This is one of two sibling flavor folders ([`crate::ml`] is the other); both
//! converge on the same AST, after which nothing may tell them apart
//! ([FLAVOR-BOUNDARY], docs/specs/language/0023-LanguageFlavors.md). Errors are
//! collected, never fatal: the frontend never panics on bad input and always
//! produces a best-effort tree.

use crate::{Flavor, Parsed, SyntaxError};
use osprey_ast::{Position, Program};
use tree_sitter::{Node, Parser, Point, Tree};

pub(crate) mod binding_ranges;
mod expr;
mod handle_rest;
mod handlers;
mod inner_doc;
mod kernel;
mod lower;
mod modules;

fn is_i64_min_magnitude_text(text: &str) -> bool {
    text.chars()
        .filter(|c| !c.is_whitespace() && *c != '(' && *c != ')')
        .eq(crate::I64_MIN_MAGNITUDE.chars())
}

pub(crate) use lower::Lowerer;

pub(crate) fn binding_ranges(source: &str) -> Vec<crate::BindingRange> {
    binding_ranges::collect(source)
}

/// The Default (brace) frontend: tree-sitter CST + [`Lowerer`] → [`Program`].
pub(crate) fn parse(source: &str) -> Parsed {
    let Some(tree) = parse_tree(source) else {
        return Parsed {
            program: Program {
                statements: Vec::new(),
                doc: None,
            },
            errors: vec![SyntaxError {
                message: "failed to initialize Osprey grammar".to_owned(),
                position: Position { line: 1, column: 0 },
            }],
            flavor: Flavor::Default,
        };
    };
    let root = tree.root_node();
    if let Err(error) = crate::check_tree_depth(root) {
        return Parsed {
            program: Program {
                statements: Vec::new(),
                doc: None,
            },
            errors: vec![error],
            flavor: Flavor::Default,
        };
    }
    let lowerer = Lowerer::new(source.as_bytes());
    let program = lowerer.lower_program(root);
    let mut errors = Vec::new();
    collect_errors(root, source.as_bytes(), &mut errors);
    inner_doc::reclassify(root, source.as_bytes(), &mut errors);
    handle_rest::check(root, &mut errors);
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
    if let Some(error) = call_variance_error(node, src) {
        out.push(error);
        return;
    }
    if node.is_error() {
        out.push(error_node_message(node, src));
    } else if node.is_missing() {
        out.push(SyntaxError {
            message: format!("missing {}", node.kind()),
            position: position_from_point(node.start_position()),
        });
    } else if node.kind() == "identifier"
        && node
            .utf8_text(src)
            .is_ok_and(|word| MODULE_KEYWORDS.contains(&word))
    {
        let p = node.start_position();
        let word = node.utf8_text(src).unwrap_or_default();
        out.push(SyntaxError {
            message: format!("`{word}` is reserved for the module system"),
            position: position_from_point(p),
        });
    } else if node.kind() == "integer" {
        let text = node.utf8_text(src).unwrap_or_default();
        let valid = text.parse::<i64>().is_ok()
            || (text == crate::I64_MIN_MAGNITUDE && is_negative_numeric(node, src));
        if !valid {
            out.push(SyntaxError {
                message: format!("integer literal `{text}` is outside the signed 64-bit range"),
                position: position_from_point(node.start_position()),
            });
        }
    } else if node.kind() == "float" {
        // [FLOAT-LITERAL-RANGE] Match the ML lexer's finite-literal contract.
        let text = node.utf8_text(src).unwrap_or_default();
        if !text.parse::<f64>().is_ok_and(f64::is_finite) {
            out.push(SyntaxError {
                message: format!("float literal `{text}` is outside the finite 64-bit range"),
                position: position_from_point(node.start_position()),
            });
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_errors(child, src, out);
    }
}

/// Variance declares a binder; it cannot decorate a call's type argument.
/// Characters of the offending text an error quotes: enough to find it, never
/// a whole block of escaped source.
const SNIPPET_CHARS: usize = 40;

/// What a tree-sitter ERROR node means, pointed at the token to change: a `;`
/// is a C-style terminator Osprey does not have; anything else quotes the
/// first line of the text the parser could not place.
fn error_node_message(node: Node<'_>, src: &[u8]) -> SyntaxError {
    match semicolon_in(node) {
        Some(semicolon) => SyntaxError {
            message: "unexpected `;`: Osprey ends a statement at the line break, so delete the `;`"
                .to_string(),
            position: position_from_point(semicolon.start_position()),
        },
        None => SyntaxError {
            message: format!(
                "syntax error near `{}`",
                snippet(node.utf8_text(src).unwrap_or(""))
            ),
            position: position_from_point(node.start_position()),
        },
    }
}

/// The first `;` token inside an ERROR node, at any depth.
fn semicolon_in(node: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = node.walk();
    let children: Vec<Node<'_>> = node.children(&mut cursor).collect();
    children.into_iter().find_map(|child| {
        if child.kind() == ";" {
            Some(child)
        } else {
            semicolon_in(child)
        }
    })
}

/// The first line of `text`, trimmed, cut to [`SNIPPET_CHARS`] with an ellipsis.
fn snippet(text: &str) -> String {
    let line = text.lines().next().unwrap_or("").trim();
    if line.chars().count() > SNIPPET_CHARS {
        format!("{}…", line.chars().take(SNIPPET_CHARS).collect::<String>())
    } else {
        line.to_string()
    }
}

fn call_variance_error(node: Node<'_>, src: &[u8]) -> Option<SyntaxError> {
    if node.kind() != "call_expression" {
        return None;
    }
    let args = node.child_by_field_name("type_arguments")?;
    let list = args.named_child(0)?;
    let marker = list.child_by_field_name("variance")?;
    let argument = marker.next_named_sibling()?.utf8_text(src).ok()?;
    let callee = node.child_by_field_name("callee")?.utf8_text(src).ok()?;
    Some(SyntaxError {
        message: format!("variance annotations are only valid on type and effect declarations; remove the marker from `{argument}` at the call to `{callee}`"),
        position: position_from_point(node.start_position()),
    })
}

fn is_negative_numeric(node: Node<'_>, src: &[u8]) -> bool {
    let mut ancestor = node.parent();
    while let Some(parent) = ancestor {
        let is_negative = parent
            .child_by_field_name("operator")
            .and_then(|op| op.utf8_text(src).ok())
            == Some("-");
        if parent.kind() == "unary_expression" && is_negative {
            return parent
                .child_by_field_name("operand")
                .and_then(|operand| operand.utf8_text(src).ok())
                .is_some_and(is_i64_min_magnitude_text);
        }
        if parent.kind() == "pattern" && is_negative {
            return true;
        }
        if matches!(parent.kind(), "statement" | "source_file") {
            break;
        }
        ancestor = parent.parent();
    }
    false
}

/// Tree-sitter keywords are contextual at identifier-only parse states. The
/// language contract reserves module words globally, so reject an identifier
/// node carrying one even when the CST could otherwise accept it.
///
/// Exactly the words [LEX-RESERVED] lists, and no others. `extra` is NOT one:
/// the grammar uses it only in signature-ascription position (`: Sig + extra`),
/// where it is read from its own CST field, so reserving the bare word globally
/// rejected an ordinary binding the spec allows — and rejected it in Default
/// only, splitting the flavors on a name ML accepts.
const MODULE_KEYWORDS: &[&str] = &["namespace", "signature", "export", "opaque", "state", "as"];

/// Convert a tree-sitter point to Osprey's one-based-line source position.
pub(crate) fn position_from_point(point: Point) -> Position {
    Position {
        line: u32::try_from(point.row)
            .unwrap_or(u32::MAX)
            .saturating_add(1),
        column: u32::try_from(point.column).unwrap_or(u32::MAX),
    }
}

pub(crate) fn string_literals(source: &str) -> Vec<crate::fragment_ranges::Literal> {
    binding_ranges::literals(source)
}

#[cfg(test)]
mod tests;
