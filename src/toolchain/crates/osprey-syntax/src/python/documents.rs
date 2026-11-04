//! Python docstrings enter the shared documentation model without execution.
//! Implements [FLAVOR-PYTHON-CST].
use super::context::{children, Lowerer, Result};
use osprey_ast::{DocComment, DocScope, Expr, Stmt};
use tree_sitter::Node;

impl Lowerer<'_> {
    pub(super) fn documented(&mut self, node: Node<'_>, mut statement: Stmt) -> Result<Stmt> {
        let documentation = self.declaration_doc(node)?;
        match &mut statement {
            Stmt::Let { doc, .. }
            | Stmt::Function { doc, .. }
            | Stmt::Effect { doc, .. }
            | Stmt::Extern { doc, .. } => {
                *doc = documentation;
            }
            _ => return self.unsupported(node),
        }
        Ok(statement)
    }
    pub(super) fn declaration_doc(&mut self, node: Node<'_>) -> Result<Option<DocComment>> {
        node.child_by_field_name("body")
            .map(|body| self.docstring(body, DocScope::Outer))
            .transpose()
            .map(Option::flatten)
    }
    pub(super) fn is_docstring(&self, node: Node<'_>) -> bool {
        if node.kind() != "expression_statement" {
            return false;
        }
        let Ok(string) = Self::first(node) else {
            return false;
        };
        if string.kind() != "string" {
            return false;
        }
        let Ok(opening) = Self::first(string) else {
            return false;
        };
        let text = self.text(opening);
        (text.ends_with("\"\"\"") || text.ends_with("'''"))
            && !text.to_ascii_lowercase().contains('f')
    }

    /// A PEP 257 attribute docstring: a plain string-literal statement that
    /// documents the declaration written directly before it
    /// ([FLAVOR-PYTHON-CST]). Returns `None` for any other node.
    pub(super) fn attribute_docstring(&mut self, node: Node<'_>) -> Result<Option<DocComment>> {
        if node.kind() != "expression_statement" {
            return Ok(None);
        }
        let Ok(string) = Self::first(node) else {
            return Ok(None);
        };
        if string.kind() != "string" {
            return Ok(None);
        }
        let Expr::Str(text) = self.string(string)? else {
            return Ok(None);
        };
        Ok(Some(crate::docparse::parse_doc(
            text.trim(),
            DocScope::Outer,
        )))
    }

    pub(super) fn body_nodes<'t>(&self, node: Node<'t>) -> Vec<Node<'t>> {
        children(node)
            .into_iter()
            .enumerate()
            .filter_map(|(index, node)| (index != 0 || !self.is_docstring(node)).then_some(node))
            .collect()
    }

    pub(super) fn docstring(
        &mut self,
        node: Node<'_>,
        scope: DocScope,
    ) -> Result<Option<DocComment>> {
        let Some(first) = children(node)
            .first()
            .copied()
            .filter(|node| self.is_docstring(*node))
        else {
            return Ok(None);
        };
        let Expr::Str(text) = self.string(Self::first(first)?)? else {
            return self.unsupported(first);
        };
        Ok(Some(crate::docparse::parse_doc(text.trim(), scope)))
    }
}
