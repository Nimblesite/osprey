use super::*;
use crate::testkit::shows;
use osprey_syntax::{parse_program, parse_program_with_flavor, Flavor};

fn module(src: &str) -> String {
    let parsed = parse_program(src);
    assert!(
        parsed.errors.is_empty(),
        "syntax errors: {:?}",
        parsed.errors
    );
    compile_program(&parsed.program).expect("codegen should succeed")
}

fn ml_module(src: &str) -> String {
    let parsed = parse_program_with_flavor(src, Flavor::Ml);
    assert!(
        parsed.errors.is_empty(),
        "syntax errors: {:?}",
        parsed.errors
    );
    compile_program(&parsed.program).expect("ML codegen should succeed")
}

fn debug_module(src: &str) -> String {
    let parsed = parse_program(src);
    assert!(
        parsed.errors.is_empty(),
        "syntax errors: {:?}",
        parsed.errors
    );
    compile_program_debug(
        &parsed.program,
        DebugSource {
            filename: "debug.osp".to_string(),
            directory: "/tmp".to_string(),
        },
    )
    .expect("debug codegen should succeed")
}

/// The body of one emitted function, so an assertion about its instructions
/// cannot be satisfied by an unrelated function elsewhere in the module.
fn function_body(ir: &str, header: &str) -> String {
    ir.split(header)
        .nth(1)
        .and_then(|rest| rest.split("\n}").next())
        .unwrap_or_default()
        .to_string()
}

/// Compile `src` and assert codegen rejected it (used for the loud-failure
/// branches that have no surface syntax of their own).
fn compile_err(src: &str) -> CodegenError {
    let parsed = parse_program(src);
    assert!(
        parsed.errors.is_empty(),
        "syntax errors: {:?}",
        parsed.errors
    );
    compile_program(&parsed.program).unwrap_err()
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
