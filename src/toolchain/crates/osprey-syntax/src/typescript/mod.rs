//! TypeScript 7 syntax tree → canonical Osprey AST, with no JavaScript engine.
//! Implements [FLAVOR-TS-BOUNDARY], [FLAVOR-TS-CST].
use crate::{Flavor, Parsed};
use osprey_ast::Program;
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

mod context;
mod depth;
mod documents;
mod effects;
mod expressions;
mod fibers;
mod handlers;
mod modules;
mod objects;
mod statements;
mod types;
mod updates;

use context::Lowerer;

pub(crate) fn parse(source: &str) -> Parsed {
    if let Some(offset) = depth::too_deep(source, crate::MAX_SYNTAX_DEPTH) {
        let lowerer = Lowerer::new(source);
        let position = lowerer.offset_position(u32::try_from(offset).unwrap_or(u32::MAX));
        return Parsed {
            program: Program::default(),
            errors: vec![crate::nesting_error(position)],
            flavor: Flavor::TypeScript,
        };
    }
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    let mut lowerer = Lowerer::new(source);
    for diagnostic in parsed.diagnostics.errors() {
        let offset = diagnostic
            .labels
            .first()
            .map_or(0, oxc_span::LabeledSpan::offset);
        lowerer.errors.push(crate::SyntaxError {
            message: format!("invalid TypeScript syntax: {diagnostic}"),
            position: lowerer.offset_position(offset),
        });
    }
    let program = if lowerer.errors.is_empty() {
        lowerer.program(&parsed.program)
    } else {
        Program::default()
    };
    Parsed {
        program,
        errors: lowerer.errors,
        flavor: Flavor::TypeScript,
    }
}
