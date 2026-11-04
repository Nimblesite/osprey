use super::*;
use crate::testkit::shows;
use osprey_syntax::{parse_program, parse_program_with_flavor, Flavor};

/// `src` parsed under `flavor`, failing the test on any syntax error.
fn parsed(src: &str, flavor: Flavor) -> osprey_ast::Program {
    let parsed = parse_program_with_flavor(src, flavor);
    assert!(
        parsed.errors.is_empty(),
        "syntax errors: {:?}",
        parsed.errors
    );
    parsed.program
}

/// A debug source named `filename` in `/tmp`.
fn tmp_source(filename: &str) -> DebugSource {
    DebugSource {
        filename: filename.to_string(),
        directory: "/tmp".to_string(),
    }
}

fn module(src: &str) -> String {
    compile_program(&parsed(src, Flavor::Default)).expect("codegen should succeed")
}

fn ml_module(src: &str) -> String {
    compile_program(&parsed(src, Flavor::Ml)).expect("ML codegen should succeed")
}

fn debug_module(src: &str) -> String {
    compile_program_debug(&parsed(src, Flavor::Default), tmp_source("debug.osp"))
        .expect("debug codegen should succeed")
}

/// The body of one emitted function, so an assertion about its instructions
/// cannot be satisfied by an unrelated function elsewhere in the module. A
/// missing header fails here, so a negative assertion on the body can never
/// pass against an empty string.
fn function_body(ir: &str, header: &str) -> String {
    shows(ir, &[header]);
    ir.split(header)
        .nth(1)
        .and_then(|rest| rest.split("\n}").next())
        .unwrap_or_default()
        .to_string()
}

/// Compile `src` and assert codegen rejected it (used for the loud-failure
/// branches that have no surface syntax of their own).
fn compile_err(src: &str) -> CodegenError {
    compile_program(&parsed(src, Flavor::Default)).unwrap_err()
}

#[path = "ir_tests/gpu.rs"]
mod gpu;
#[path = "ir_tests/lowering_1.rs"]
mod lowering_1;
#[path = "ir_tests/lowering_2.rs"]
mod lowering_2;
#[path = "ir_tests/lowering_3.rs"]
mod lowering_3;
#[path = "ir_tests/lowering_4.rs"]
mod lowering_4;
#[path = "ir_tests/lowering_5.rs"]
mod lowering_5;
#[path = "ir_tests/symbolic.rs"]
mod symbolic;
