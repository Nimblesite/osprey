//! Python CST → canonical Osprey AST, with no Python execution environment.
//! Implements [FLAVOR-PYTHON-BOUNDARY], [FLAVOR-PYTHON-CST].
use crate::{Flavor, Parsed, SyntaxError};
use osprey_ast::{Position, Program};
use tree_sitter::{Node, Parser, Tree};

mod comprehensions;
mod context;
mod declarations;
mod documents;
mod effects;
mod expressions;
mod intrinsics;
mod modules;
mod patterns;
mod statements;
mod strings;
mod types;
mod typing;

use context::Lowerer;

pub(crate) fn parse(source: &str) -> Parsed {
    let (program, lowerer) = lower(source);
    Parsed {
        program,
        errors: lowerer.errors,
        flavor: Flavor::Python,
    }
}

pub(crate) fn binding_ranges(source: &str) -> Vec<crate::BindingRange> {
    lower(source).1.bindings
}

pub(crate) fn annotation_edits(source: &str) -> Vec<crate::AnnotationEdit> {
    lower(source).1.annotations
}

/// Parse a Python concrete syntax tree without executing or lowering its source.
///
/// # Errors
/// Returns an initialization/cancellation diagnostic when no tree is available.
/// Syntax recovery nodes remain in the returned tree for authoring tools.
pub fn parse_python_tree(source: &str) -> Result<Tree, SyntaxError> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .map_err(|error| SyntaxError {
            message: format!("cannot initialize Python grammar: {error}"),
            position: Position { line: 1, column: 0 },
        })?;
    parser.parse(source, None).ok_or_else(|| SyntaxError {
        message: "Python parser did not produce a concrete syntax tree".into(),
        position: Position { line: 1, column: 0 },
    })
}

fn lower(source: &str) -> (Program, Lowerer<'_>) {
    let mut lowerer = Lowerer::new(source);
    let program = match parse_python_tree(source) {
        Ok(tree) => {
            indentation(source, tree.root_node(), &mut lowerer);
            syntax_errors(tree.root_node(), &mut lowerer);
            lowerer.program(tree.root_node())
        }
        Err(error) => {
            lowerer.errors.push(error);
            Program {
                statements: Vec::new(),
                doc: None,
            }
        }
    };
    (program, lowerer)
}

fn indentation(source: &str, root: Node<'_>, lowerer: &mut Lowerer<'_>) {
    let mut offset = 0;
    for text in source.split_inclusive('\n') {
        for (column, ch) in text.char_indices().take_while(|(_, ch)| ch.is_whitespace()) {
            if ch == '\t' && !inside_string(root, offset + column) {
                lowerer.errors.push(Lowerer::error(
                    root.descendant_for_byte_range(offset + column, offset + column + 1)
                        .unwrap_or(root),
                    "tabs are not allowed in Python-flavor indentation",
                ));
            }
        }
        offset += text.len();
    }
}

fn inside_string(root: Node<'_>, offset: usize) -> bool {
    let mut node = root.descendant_for_byte_range(offset, offset + 1);
    while let Some(current) = node {
        if current.kind() == "string" {
            return true;
        }
        node = current.parent();
    }
    false
}

fn syntax_errors(node: Node<'_>, lowerer: &mut Lowerer<'_>) {
    if node.is_error() || node.is_missing() {
        lowerer.errors.push(Lowerer::error(
            node,
            format!("invalid Python syntax near `{}`", lowerer.text(node)),
        ));
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        syntax_errors(child, lowerer);
    }
}
