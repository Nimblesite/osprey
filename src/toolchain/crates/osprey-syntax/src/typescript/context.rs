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
    /// The case's source text, which a field's binder must not capture.
    pub(super) span: Span,
    /// Each field read, and the name the pattern binds it to.
    pub(super) fields: Vec<(String, String)>,
}

pub(super) struct Lowerer<'s> {
    source: &'s str,
    line_starts: Vec<usize>,
    scopes: Vec<Scope>,
    pub(super) errors: Vec<SyntaxError>,
    /// Package default imports: the local alias and the entry function a
    /// call of the alias names.
    pub(super) packages: BTreeMap<String, String>,
    pub(super) exports: BTreeSet<String>,
    /// `export { local as exported }` pairs, applied once the file is lowered.
    pub(super) renamed: Vec<(String, String, Span)>,
    /// This file's namespace directive, which a relative import resolves in.
    pub(super) namespace: Option<String>,
    pub(super) narrowed: Vec<Narrowing>,
    pub(super) docs: Vec<super::documents::DocBlock>,
}

struct Scope {
    names: BTreeSet<String>,
    /// The `const` bindings among `names`, which no assignment may rebind.
    constants: BTreeSet<String>,
    /// Locals a nested function reads, which a later update would not reach.
    captured: BTreeSet<String>,
    /// Locals a handler arm writes: handler-owned state, so Osprey `mut`.
    mutated: BTreeSet<String>,
    function: bool,
    /// A handler arm's function scope, whose assignments may write state.
    arm: bool,
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
            packages: BTreeMap::new(),
            exports: BTreeSet::new(),
            renamed: Vec::new(),
            namespace: None,
            narrowed: Vec::new(),
            docs: Vec::new(),
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
        self.with_scope(Scope::new(function, tail), action)
    }

    /// A handler arm's function scope. [EFFECTS-HANDLER-STATE]
    pub(super) fn arm_scoped<T>(
        &mut self,
        action: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<T> {
        let mut scope = Scope::new(true, true);
        scope.arm = true;
        self.with_scope(scope, action)
    }

    fn with_scope<T>(
        &mut self,
        scope: Scope,
        action: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<T> {
        self.scopes.push(scope);
        let result = action(self);
        let _ = self.scopes.pop();
        result
    }

    /// Whether the innermost function is a handler arm.
    pub(super) fn in_arm(&self) -> bool {
        self.function_scope().is_some_and(|scope| scope.arm)
    }

    /// Record that a handler arm writes `name`, which therefore becomes
    /// handler state; false when no enclosing scope binds it.
    pub(super) fn write_state(&mut self, name: &str) -> bool {
        let Some(scope) = self
            .scopes
            .iter_mut()
            .rev()
            .find(|scope| scope.names.contains(name))
        else {
            return false;
        };
        let _ = scope.mutated.insert(name.to_owned());
        let _ = scope.captured.insert(name.to_owned());
        true
    }

    /// The names a handler arm writes among the innermost scope's bindings.
    pub(super) fn take_mutated(&mut self) -> BTreeSet<String> {
        self.scopes
            .last_mut()
            .map_or_else(BTreeSet::new, |scope| std::mem::take(&mut scope.mutated))
    }

    pub(super) fn bind(&mut self, name: &str) {
        if let Some(scope) = self.scopes.last_mut() {
            let _ = scope.names.insert(name.to_owned());
        }
    }

    /// Bind `name` as a `const`, which an assignment may not rebind.
    pub(super) fn bind_constant(&mut self, name: &str) {
        self.bind(name);
        if let Some(scope) = self.scopes.last_mut() {
            let _ = scope.constants.insert(name.to_owned());
        }
    }

    /// Whether the innermost binding of `name` is a `const`.
    pub(super) fn constant(&self, name: &str) -> bool {
        self.innermost_scope_with(name)
            .is_some_and(|scope| scope.constants.contains(name))
    }

    /// The innermost scope that binds `name`, if any does.
    fn innermost_scope_with(&self, name: &str) -> Option<&Scope> {
        self.scopes
            .iter()
            .rev()
            .find(|scope| scope.names.contains(name))
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
        self.innermost_scope_with(name)
            .is_some_and(|scope| scope.captured.contains(name))
    }

    pub(super) fn in_tail(&self) -> bool {
        self.scopes.last().is_some_and(|scope| scope.tail)
    }

    fn function_scope(&self) -> Option<&Scope> {
        self.scopes.iter().rev().find(|scope| scope.function)
    }

    fn function_scope_mut(&mut self) -> Option<&mut Scope> {
        self.scopes.iter_mut().rev().find(|scope| scope.function)
    }

    pub(super) fn set_function_result(
        &mut self,
        updated: Option<String>,
        returns: Option<Expected>,
    ) {
        if let Some(scope) = self.function_scope_mut() {
            scope.updated = updated;
            scope.returns = returns;
        }
    }

    pub(super) fn updated_parameter(&self) -> Option<String> {
        self.function_scope()?.updated.clone()
    }

    pub(super) fn expected_return(&self) -> Option<Expected> {
        self.function_scope()?.returns.clone()
    }

    /// The name a case binds `subject.field` to: the field's own name, unless
    /// the case also reads a binding of that name, which the pattern would
    /// then shadow; a fresh name that occurs nowhere in the file otherwise.
    pub(super) fn field_binder(&self, subject: &str, field: &str, case: Span) -> String {
        if !reads_word(self.text(case), field) {
            return field.to_owned();
        }
        let mut chars = field.chars();
        let capitalised: String = chars.next().map_or_else(String::new, |first| {
            first.to_uppercase().chain(chars).collect()
        });
        let mut fresh = format!("{subject}{capitalised}");
        while self.source.contains(fresh.as_str()) {
            fresh.push('_');
        }
        fresh
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
            constants: BTreeSet::new(),
            captured: BTreeSet::new(),
            mutated: BTreeSet::new(),
            function,
            arm: false,
            tail,
            updated: None,
            returns: None,
        }
    }
}

/// Whether `text` has `word` as an identifier that is not a member name.
fn reads_word(text: &str, word: &str) -> bool {
    let identifier = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    text.match_indices(word).any(|(at, _)| {
        let before = text.get(..at).and_then(|head| head.chars().next_back());
        let after = text
            .get(at + word.len()..)
            .and_then(|tail| tail.chars().next());
        !before.is_some_and(|c| identifier(c) || c == '.') && !after.is_some_and(identifier)
    })
}
