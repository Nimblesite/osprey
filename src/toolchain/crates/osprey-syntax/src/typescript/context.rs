//! Source positions and lexical scope owned by the TypeScript frontend.
use crate::SyntaxError;
use osprey_ast::{Expr, Position, Stmt, TypeExpr};
use oxc_span::Span;
use std::collections::{BTreeMap, BTreeSet};

pub(super) type Result<T> = std::result::Result<T, SyntaxError>;

/// The record type an object literal constructs, read from its syntactic
/// context. Implements [FLAVOR-TS-UNIONS].
#[derive(Clone, Debug)]
pub(super) struct Expected {
    pub(super) name: String,
    pub(super) type_args: Vec<TypeExpr>,
}

/// A `switch (value.kind)` case in progress: `value.field` inside the case
/// reads the field the case's pattern binds. Implements [FLAVOR-TS-UNIONS].
pub(super) struct Narrowing {
    pub(super) subject: String,
    pub(super) fields: Vec<String>,
}

pub(super) struct Lowerer<'s> {
    source: &'s str,
    line_starts: Vec<usize>,
    scopes: Vec<Scope>,
    pub(super) errors: Vec<SyntaxError>,
    /// Import-bound module aliases, each with the entry function a call of
    /// the alias itself names (a package default import).
    pub(super) modules: BTreeMap<String, Option<String>>,
    pub(super) exports: BTreeSet<String>,
    /// This file's namespace directive, which a relative import resolves in.
    pub(super) namespace: Option<String>,
    pub(super) narrowed: Vec<Narrowing>,
}

struct Scope {
    names: BTreeSet<String>,
    /// Locals a nested function reads, which a later update would not reach.
    captured: BTreeSet<String>,
    function: bool,
    tail: bool,
    /// The parameter this function updates in place and therefore returns.
    updated: Option<String>,
    returns: Option<Expected>,
}

pub(super) fn unit() -> Expr {
    Expr::Block {
        statements: Vec::new(),
        value: None,
        position: None,
    }
}

pub(super) fn apply(name: &str, arguments: Vec<Expr>) -> Expr {
    Expr::Call {
        function: Box::new(Expr::Identifier(name.into())),
        arguments,
        named_arguments: Vec::new(),
    }
}

impl<'s> Lowerer<'s> {
    pub(super) fn new(source: &'s str) -> Self {
        let line_starts = std::iter::once(0)
            .chain(source.match_indices('\n').map(|(index, _)| index + 1))
            .collect();
        Self {
            source,
            line_starts,
            scopes: vec![Scope::new(true, false)],
            errors: Vec::new(),
            modules: BTreeMap::new(),
            exports: BTreeSet::new(),
            namespace: None,
            narrowed: Vec::new(),
        }
    }

    pub(super) fn text(&self, span: Span) -> &'s str {
        let range =
            usize::try_from(span.start).unwrap_or(0)..usize::try_from(span.end).unwrap_or(0);
        self.source.get(range).unwrap_or_default()
    }

    /// One-based line and zero-based byte column of a byte offset.
    pub(super) fn offset_position(&self, offset: u32) -> Position {
        let offset = usize::try_from(offset).unwrap_or(usize::MAX);
        let line = self.line_starts.partition_point(|start| *start <= offset);
        let start = line
            .checked_sub(1)
            .and_then(|index| self.line_starts.get(index))
            .copied()
            .unwrap_or(0);
        Position {
            line: u32::try_from(line).unwrap_or(u32::MAX),
            column: u32::try_from(offset.saturating_sub(start)).unwrap_or(u32::MAX),
        }
    }

    pub(super) fn at(&self, span: Span) -> Position {
        self.offset_position(span.start)
    }

    pub(super) fn error(&self, span: Span, message: impl Into<String>) -> SyntaxError {
        SyntaxError {
            message: message.into(),
            position: self.at(span),
        }
    }

    /// Reject a named TypeScript construct at its own position.
    pub(super) fn unsupported<T>(&self, span: Span, construct: &str) -> Result<T> {
        Err(self.error(
            span,
            format!("TypeScript {construct} has no Osprey lowering"),
        ))
    }

    pub(super) fn scoped<T>(
        &mut self,
        function: bool,
        tail: bool,
        action: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<T> {
        self.scopes.push(Scope::new(function, tail));
        let result = action(self);
        let _ = self.scopes.pop();
        result
    }

    pub(super) fn bind(&mut self, name: &str) {
        if let Some(scope) = self.scopes.last_mut() {
            let _ = scope.names.insert(name.to_owned());
        }
    }

    pub(super) fn file_scope(&self) -> bool {
        self.scopes.len() == 1
    }

    /// Whether `name` is bound by the innermost block.
    pub(super) fn bound_here(&self, name: &str) -> bool {
        self.scopes
            .last()
            .is_some_and(|scope| scope.names.contains(name))
    }

    /// Whether `name` is a local of the enclosing function (or of file scope).
    pub(super) fn bound_in_function(&self, name: &str) -> bool {
        for scope in self.scopes.iter().rev() {
            if scope.names.contains(name) {
                return true;
            }
            if scope.function {
                break;
            }
        }
        false
    }

    /// Record a read of `name`; a read from inside a nested function captures it.
    pub(super) fn note_use(&mut self, name: &str) {
        let mut crossed = false;
        for scope in self.scopes.iter_mut().rev() {
            if scope.names.contains(name) {
                if crossed {
                    let _ = scope.captured.insert(name.to_owned());
                }
                return;
            }
            crossed |= scope.function;
        }
    }

    pub(super) fn captured(&self, name: &str) -> bool {
        self.scopes
            .iter()
            .rev()
            .find(|scope| scope.names.contains(name))
            .is_some_and(|scope| scope.captured.contains(name))
    }

    pub(super) fn in_tail(&self) -> bool {
        self.scopes.last().is_some_and(|scope| scope.tail)
    }

    fn function_scope(&mut self) -> Option<&mut Scope> {
        self.scopes.iter_mut().rev().find(|scope| scope.function)
    }

    pub(super) fn set_function_result(
        &mut self,
        updated: Option<String>,
        returns: Option<Expected>,
    ) {
        if let Some(scope) = self.function_scope() {
            scope.updated = updated;
            scope.returns = returns;
        }
    }

    pub(super) fn updated_parameter(&self) -> Option<String> {
        let scope = self.scopes.iter().rev().find(|scope| scope.function)?;
        scope.updated.clone()
    }

    pub(super) fn expected_return(&self) -> Option<Expected> {
        let scope = self.scopes.iter().rev().find(|scope| scope.function)?;
        scope.returns.clone()
    }

    pub(super) fn let_statement(
        &self,
        name: &str,
        value: Expr,
        ty: Option<TypeExpr>,
        span: Span,
    ) -> Stmt {
        Stmt::Let {
            name: name.into(),
            mutable: false,
            ty,
            value,
            doc: None,
            position: Some(self.at(span)),
        }
    }
}

impl Scope {
    fn new(function: bool, tail: bool) -> Self {
        Self {
            names: BTreeSet::new(),
            captured: BTreeSet::new(),
            function,
            tail,
            updated: None,
            returns: None,
        }
    }
}
