//! Statement, type, and pattern lowering: declarations (`fn`, `let`, `type`,
//! `effect`, `extern`, `module`), type expressions, and match patterns.

use super::position_from_point;
use osprey_ast::{DocComment, DocScope, Expr, Position, Stage};
use tree_sitter::Node;

/// The surface spelling of an ignored parameter ([PARAM-WILDCARD]).
mod declarations;
mod patterns;
mod signatures;
#[cfg(test)]
mod tests;
const WILDCARD_PARAM: &str = "_";

/// Strip one doc-comment line's leading whitespace, its `///`/`//!` marker, and
/// one optional following space — leaving the prose. Implements
/// [DOC-SIGIL-DEFAULT].
fn strip_doc_line(line: &str) -> &str {
    let t = line.trim_start();
    let rest = t
        .strip_prefix("///")
        .or_else(|| t.strip_prefix("//!"))
        .unwrap_or(line);
    rest.strip_prefix(' ').unwrap_or(rest)
}

/// Holds the source bytes so node text can be sliced during lowering.
#[derive(Debug)]
pub(crate) struct Lowerer<'a> {
    src: &'a [u8],
}

impl<'a> Lowerer<'a> {
    /// Creates a lowerer over the given source bytes.
    #[must_use]
    pub(crate) fn new(src: &'a [u8]) -> Self {
        Lowerer { src }
    }

    pub(crate) fn text(&self, node: Node<'_>) -> String {
        node.utf8_text(self.src).unwrap_or("").to_string()
    }

    #[expect(
        clippy::unused_self,
        reason = "kept for Lowerer method-call ergonomics"
    )]
    pub(crate) fn pos(&self, node: Node<'_>) -> Position {
        position_from_point(node.start_position())
    }

    /// The stage of an effect declaration or handler region: [`Stage::Static`]
    /// when the `static` marker is present, dynamic otherwise — so every effect
    /// written before staging keeps its meaning. Implements [STAGE-DECL],
    /// [STAGE-HANDLE-STATIC], [STAGE-COMPAT].
    #[expect(
        clippy::unused_self,
        reason = "kept for Lowerer method-call ergonomics"
    )]
    pub(crate) fn stage(&self, node: Node<'_>) -> Stage {
        match node.child_by_field_name("stage") {
            Some(_) => Stage::Static,
            None => Stage::Dynamic,
        }
    }

    /// Position of `node`'s named `field`, or `node`'s own start when absent. A
    /// leading `///` doc comment shifts `node.start` onto the comment, so a
    /// declaration keeps a stable position by anchoring on its keyword/name.
    fn field_pos(&self, node: Node<'_>, field: &str) -> Position {
        self.pos(node.child_by_field_name(field).unwrap_or(node))
    }

    /// The leading `///` documentation of a declaration, stripped of its
    /// markers and lowered into a structured [`DocComment`] by the shared
    /// flavor-neutral parser; `None` when the declaration carries no doc
    /// comment. Implements [DOC-SIGIL-DEFAULT], [DOC-MODEL].
    pub(crate) fn doc_text(&self, node: Node<'_>) -> Option<DocComment> {
        self.doc_of_kind(node, "doc_comment", DocScope::Outer)
    }

    /// The `//!` documentation written INSIDE `node` — the file, namespace body
    /// or module body that `node` opens. The grammar keeps the two sigils in
    /// separate rules, so a scope carrying both an outer `///` and an inner
    /// `//!` lowers both. Implements [DOC-SIGIL-INNER].
    pub(crate) fn inner_doc_text(&self, node: Node<'_>) -> Option<DocComment> {
        self.doc_of_kind(node, "inner_doc_comment", DocScope::Inner)
    }

    /// Lower the first `kind` child of `node` into a [`DocComment`] at `scope`.
    /// Both sigils differ only in which node kind carries them.
    fn doc_of_kind(&self, node: Node<'_>, kind: &str, scope: DocScope) -> Option<DocComment> {
        let text = self.text(self.first_child_of_kind(node, kind)?);
        let body: Vec<&str> = text.lines().map(strip_doc_line).collect();
        Some(crate::docparse::parse_doc(&body.join("\n"), scope))
    }

    /// First *named* child (skips anonymous tokens). Used to unwrap the wrapper
    /// nodes tree-sitter inserts (`statement`, `expression`, `primary_expression`).
    #[expect(
        clippy::unused_self,
        reason = "kept for Lowerer method-call ergonomics"
    )]
    pub(crate) fn first_named<'t>(&self, node: Node<'t>) -> Option<Node<'t>> {
        let mut cursor = node.walk();
        let found = node.named_children(&mut cursor).next();
        found
    }

    /// The last named child — the expression of an `expression_statement`,
    /// whose only other child is an optional doc comment.
    pub(crate) fn last_named(node: Node<'_>) -> Option<Node<'_>> {
        let mut cursor = node.walk();
        let found = node.named_children(&mut cursor).last();
        found
    }

    /// First named child of a given kind.
    #[expect(
        clippy::unused_self,
        reason = "kept for Lowerer method-call ergonomics"
    )]
    pub(crate) fn first_child_of_kind<'t>(&self, node: Node<'t>, kind: &str) -> Option<Node<'t>> {
        let mut cursor = node.walk();
        let found = node.named_children(&mut cursor).find(|c| c.kind() == kind);
        found
    }
}

/// Negate a numeric literal for a `-N` pattern; non-numeric literals pass through.
fn negate_literal(e: Expr) -> Expr {
    match e {
        Expr::Integer(n) => Expr::Integer(-n),
        Expr::Float(f) => Expr::Float(-f),
        other => other,
    }
}
