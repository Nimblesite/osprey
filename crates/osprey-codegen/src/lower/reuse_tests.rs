//! The build path runs one whole-program effect proof per checked program:
//! the CLI gate retains its solve and codegen reuses it instead of re-running
//! [`osprey_types::infer_program`]. [TYPE-EFFECT-PROOF-COUNT]
use super::*;
use osprey_syntax::parse_program;

fn parsed(source: &str) -> Program {
    let parsed = parse_program(source);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    parsed.program
}

#[test]
fn build_path_runs_one_effect_check_per_program() {
    let program = parsed("fn main() = 7\n");
    let start = osprey_types::effect_check_count();
    let checked = osprey_types::check_and_publish(&program, &[]).expect("well-typed");
    assert_eq!(
        osprey_types::effect_check_count(),
        start + 1,
        "the gate runs exactly one effect proof"
    );
    set_reused_check(Some(checked));
    let _ir = compile_program(&program).expect("reuse lowers");
    assert_eq!(
        osprey_types::effect_check_count(),
        start + 1,
        "codegen must reuse the supplied solve, not re-check"
    );
}

#[test]
fn reused_solve_matches_fresh_codegen() {
    let program = parsed("fn main() = 7\n");
    let checked = osprey_types::check_and_publish(&program, &[]).expect("well-typed");
    let fresh = compile_program(&program).expect("fresh lowers");
    set_reused_check(Some(checked));
    let reused = compile_program(&program).expect("reuse lowers");
    assert_eq!(
        reused, fresh,
        "reused IR must be byte-identical to fresh IR"
    );
}

#[test]
fn a_retained_check_cannot_replace_a_different_program() {
    let first = parsed("fn main() = print(\"first\")\n");
    let checked = osprey_types::check_and_publish(&first, &[]).expect("first program checks");
    let other = parsed("fn main() = print(\"second\")\n");
    let fresh = compile_program(&other).expect("second program lowers");
    let original = compile_program(&first).expect("first program lowers");
    assert_ne!(original, fresh);
    set_reused_check(Some(checked));
    let reused = compile_program(&other).expect("second program still lowers");
    assert_eq!(reused, fresh, "retained solve must match its source");
}

#[test]
fn retained_checks_distinguish_signed_float_zero() {
    let first = parsed("fn main() = print(0.0)\n");
    let second = parsed("fn main() = print(-0.0)\n");
    let first_ir = compile_program(&first).expect("positive zero lowers");
    let fresh = compile_program(&second).expect("negative zero lowers");
    assert_ne!(first_ir, fresh);
    let checked = osprey_types::check_and_publish(&first, &[]).expect("positive zero checks");
    set_reused_check(Some(checked));
    let reused = compile_program(&second).expect("negative zero still lowers");
    assert_eq!(reused, fresh, "signed zero is part of source identity");
}

#[test]
fn a_failed_codegen_does_not_retain_a_previous_check() {
    let program = parsed("fn main() = 7\n");
    let checked = osprey_types::check_and_publish(&program, &[]).expect("program checks");
    let mut invalid = parsed("type Token = int\n");
    if let Some(Stmt::Type { opaque, .. }) = invalid.statements.first_mut() {
        *opaque = true;
    }
    set_reused_check(Some(checked));
    assert!(compile_program(&invalid).is_err());
    let start = osprey_types::effect_check_count();
    let _ = compile_program(&program).expect("fresh compilation works");
    assert_eq!(osprey_types::effect_check_count(), start + 1);
}
