//! The checker's answers for one document.
//!
//! A file of a project is checked as the compiler checks it: assembled with
//! every other file, so a type that only another package fixes is known. The
//! assembled program names declarations by linkage name and numbers lines
//! across all its files, so a function is found by where it is declared, a
//! position is shifted on the way in and a rendered type is respelled on the
//! way out. A document outside any project — or in
//! one that does not assemble — is checked on its own.
//! Implements [LSP-HOVER-PROJECT].

use std::collections::BTreeMap;
use std::sync::Arc;

use osprey_ast::{Position, Program};
use osprey_types::{EffectRequirements, ProgramTypes, Type};

use crate::analysis::SymbolInfo;

/// A program and where the checker's answers for it come from.
pub(crate) struct Typed<'a> {
    pub(crate) program: &'a Program,
    pub(crate) project: Option<Inferred>,
}

impl Typed<'_> {
    /// The project's answers when the document is part of one, else the
    /// answers of the document checked alone.
    pub(crate) fn inferred(&self) -> Inferred {
        self.project
            .clone()
            .unwrap_or_else(|| Inferred::file(self.program))
    }
}

/// Where each function of an assembled program is declared.
pub(crate) type Functions = Arc<BTreeMap<(u32, u32), String>>;

/// Checked types, addressed by the document's own names and positions.
#[derive(Debug, Clone)]
pub(crate) struct Inferred {
    types: Arc<ProgramTypes>,
    /// Declaration position in the assembled program → the function's linkage
    /// name; empty for a document checked alone, whose names are its own.
    functions: Functions,
    /// Lines of the assembled program that precede the document's first.
    lines_before: u32,
}

impl Inferred {
    /// The answers for `program` checked on its own.
    pub(crate) fn file(program: &Program) -> Self {
        Self::placed(
            Arc::new(osprey_types::infer_program(program)),
            Arc::default(),
            0,
        )
    }

    pub(crate) const fn placed(
        types: Arc<ProgramTypes>,
        functions: Functions,
        lines_before: u32,
    ) -> Self {
        Self {
            types,
            functions,
            lines_before,
        }
    }

    /// The checker's name for the function the document declares as
    /// `function`.
    fn key<'a>(&'a self, function: &'a SymbolInfo) -> &'a str {
        function
            .position
            .map(|at| self.at(at))
            .and_then(|at| self.functions.get(&(at.line, at.column)))
            .map_or(&function.name, String::as_str)
    }

    fn at(&self, position: Position) -> Position {
        Position {
            line: position.line.saturating_add(self.lines_before),
            column: position.column,
        }
    }

    pub(crate) fn param_types(&self, function: &SymbolInfo) -> Option<&[Type]> {
        self.types.param_types(self.key(function))
    }

    pub(crate) fn return_type(&self, function: &SymbolInfo) -> Option<&Type> {
        self.types.return_type(self.key(function))
    }

    pub(crate) fn effects(&self, function: &SymbolInfo) -> Option<EffectRequirements> {
        let mut found = self.types.function_effects.get(self.key(function))?.clone();
        for name in found
            .operations
            .iter_mut()
            .chain(&mut found.runtime_builtins)
        {
            *name = source_names(std::mem::take(name));
        }
        Some(found)
    }

    pub(crate) fn let_type(&self, position: Option<Position>) -> Option<&Type> {
        self.types
            .source_let_type(None, position.map(|at| self.at(at)))
    }

    pub(crate) fn binder_type(
        &self,
        name: &str,
        region: Position,
        declared: Position,
    ) -> Option<&Type> {
        self.types
            .binder_type(name, self.at(region), self.at(declared))
    }

    /// `ty` as a reader sees it, or `None` for a bare hole
    /// ([`crate::analysis::shown`]).
    pub(crate) fn shown(&self, ty: &Type) -> Option<String> {
        crate::analysis::shown(ty, &self.types).map(source_names)
    }

    /// `ty` with holes for its unsolved parts ([TYPE-RENDER-HOLES]).
    pub(crate) fn rendered(ty: &Type) -> String {
        source_names(osprey_types::render_with_holes(ty))
    }
}

/// Linkage names never reach a reader: each is respelled as its source name.
fn source_names(text: String) -> String {
    match osprey_ast::symbol::demangle_message(&text) {
        std::borrow::Cow::Borrowed(_) => text,
        std::borrow::Cow::Owned(named) => named,
    }
}
