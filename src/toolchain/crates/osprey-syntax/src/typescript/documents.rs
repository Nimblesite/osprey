//! `/** … */` comments enter the shared documentation model.
//! Implements [FLAVOR-TS-CST].
use super::context::Lowerer;
use osprey_ast::{DocComment, DocScope, Stmt};
use oxc_span::Span;

/// A documentation comment and whether a declaration has claimed it.
pub(super) struct DocBlock {
    span: Span,
    claimed: bool,
}

impl Lowerer<'_> {
    pub(super) fn collect_docs(&mut self, comments: &[oxc_ast::ast::Comment]) {
        self.docs = comments
            .iter()
            .filter(|comment| self.text(comment.span).starts_with("/**"))
            .map(|comment| DocBlock {
                span: comment.span,
                claimed: false,
            })
            .collect();
    }

    /// Document the first of `declarations` with the comment written directly
    /// before the statement that starts at `start`.
    pub(super) fn document(&mut self, declarations: &mut [Stmt], start: u32) {
        let Some(slot) = declarations.first_mut().and_then(doc_slot) else {
            return;
        };
        let source = self.text(Span::new(0, start));
        let block = self.docs.iter_mut().find(|block| {
            block.span.end <= start
                && source
                    .get(usize::try_from(block.span.end).unwrap_or(usize::MAX)..)
                    .is_some_and(|between| between.trim().is_empty())
        });
        if let Some(block) = block {
            block.claimed = true;
            let span = block.span;
            *slot = Some(self.parsed_doc(span, DocScope::Outer));
        }
    }

    /// The file's own documentation: a leading comment no declaration claimed.
    pub(super) fn file_doc(&self, first_statement: u32) -> Option<DocComment> {
        self.docs
            .first()
            .filter(|block| !block.claimed && block.span.end <= first_statement)
            .map(|block| self.parsed_doc(block.span, DocScope::Inner))
    }

    fn parsed_doc(&self, span: Span, scope: DocScope) -> DocComment {
        crate::docparse::parse_doc(&clean(self.text(span)), scope)
    }
}

/// The documentation field of a declaration that can carry one.
fn doc_slot(statement: &mut Stmt) -> Option<&mut Option<DocComment>> {
    match statement {
        Stmt::Let { doc, .. }
        | Stmt::Function { doc, .. }
        | Stmt::Type { doc, .. }
        | Stmt::Effect { doc, .. } => Some(doc),
        _ => None,
    }
}

/// A `/** … */` comment's text without its delimiters and line-leading `*`.
fn clean(comment: &str) -> String {
    let body = comment
        .strip_prefix("/**")
        .and_then(|rest| rest.strip_suffix("*/"))
        .unwrap_or(comment);
    body.lines()
        .map(|line| {
            let line = line.trim_start();
            let line = line.strip_prefix('*').unwrap_or(line);
            line.strip_prefix(' ').unwrap_or(line).trim_end()
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}
