//! TypeScript 7 syntax tree → canonical Osprey AST, with no JavaScript engine.
//! Implements [FLAVOR-TS-BOUNDARY], [FLAVOR-TS-CST].
use crate::{Flavor, Parsed};
use osprey_ast::Program;
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

mod context;
mod documents;
mod expressions;
mod modules;
mod objects;
mod statements;
mod types;

use context::Lowerer;

pub(crate) fn parse(source: &str) -> Parsed {
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
