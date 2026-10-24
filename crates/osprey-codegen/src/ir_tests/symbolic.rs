//! `[SYMEX-CODEGEN]`: the module a symbolic-execution engine explores.

use super::*;

const DYNAMIC_EFFECT: &str = "effect Wide { control op: fn(int, string) -> int }\n\
     fn body() -> int !Wide = perform Wide.op(1, \"x\")\n\
     fn main() -> int { let r = {\n\
         handle Wide {\n\
             op a b => resume(a)\n\
         }\n\
         body()\n\
     }\n\
       print(\"r=${toString(r)}\")\n\
       0 }\n";

fn symbolic_module(src: &str) -> String {
    compile_program_symbolic(&parsed(src, Flavor::Default), tmp_source("debug.osp"))
        .expect("symbolic codegen should succeed")
}

#[test]
fn klee_mode_aborts_on_the_unhandled_effect_guard() {
    let plain = module(DYNAMIC_EFFECT);
    let symbolic = symbolic_module(DYNAMIC_EFFECT);
    assert!(
        plain.contains("call void @exit(i32 1)") && !plain.contains("@abort"),
        "a native build keeps exit on the guard path"
    );
    assert!(
        symbolic.contains("call void @abort()") && !symbolic.contains("call void @exit"),
        "a symbolic build aborts on the guard path"
    );
}

#[test]
fn klee_mode_carries_debug_metadata_without_debug_flag() {
    let symbolic = symbolic_module("print(\"hello\")\n");
    assert!(
        symbolic.contains("!DICompileUnit") && symbolic.contains("!DILocation"),
        "{symbolic}"
    );
    assert!(!symbolic.contains("optnone"));
}

/// Programs whose symbolic and `--debug` builds are compared line for line:
/// a guard-free one, a dynamic handler with its guard, and the `osprey hunt`
/// corpus [SYMEX-CODEGEN].
const PROGRAMS: [&str; 5] = [
    "print(\"hello\")
",
    DYNAMIC_EFFECT,
    include_str!("../../../../tests/hunt/clean.osp"),
    include_str!("../../../../tests/hunt/process.osp"),
    include_str!("../../../../tests/hunt/record_update_list.osp"),
];

/// A module's instructions in order and its declarations as a set: the
/// symbolic switch swaps one extern, which may sort differently.
fn shape(module: &str) -> (Vec<&str>, std::collections::BTreeSet<&str>) {
    let (declares, body): (Vec<&str>, Vec<&str>) = module
        .lines()
        .partition(|line| line.starts_with("declare "));
    (body, declares.into_iter().collect())
}

/// "Builds without it do not change": the symbolic module is the `--debug`
/// module with each fatal `exit` turned into `abort`, nothing else.
#[test]
fn klee_mode_is_the_debug_build_with_abort_for_exit() {
    for src in PROGRAMS {
        let symbolic = symbolic_module(src);
        let as_native = symbolic
            .replace("call void @abort()", "call void @exit(i32 1)")
            .replace("declare void @abort()", "declare void @exit(i32)");
        let debug = debug_module(src);
        assert_eq!(shape(&as_native), shape(&debug), "{src}");
    }
}

#[test]
fn every_klee_module_has_metadata_no_optnone_and_no_fatal_exit() {
    for src in PROGRAMS {
        let symbolic = symbolic_module(src);
        assert!(symbolic.contains("!DICompileUnit"), "{src}");
        assert!(symbolic.contains("debug.osp"), "{src}");
        assert!(!symbolic.contains("optnone"), "{src}");
        assert!(!symbolic.contains("call void @exit(i32 1)"), "{src}");
        assert_eq!(symbolic, symbolic_module(src), "deterministic: {src}");
    }
}

/// Each fatal path prints its message, then aborts, then is unreachable.
#[test]
fn every_abort_follows_its_message_and_ends_the_block() {
    let symbolic = symbolic_module(DYNAMIC_EFFECT);
    let lines: Vec<&str> = symbolic.lines().map(str::trim).collect();
    let aborts: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.starts_with("call void @abort()"))
        .map(|(at, _)| at)
        .collect();
    assert!(!aborts.is_empty(), "{symbolic}");
    for at in aborts {
        let before = lines.get(at.wrapping_sub(1)).copied().unwrap_or_default();
        let after = lines.get(at + 1).copied().unwrap_or_default();
        assert!(before.contains("call i32 @puts("), "{before}");
        assert!(after.starts_with("unreachable"), "{after}");
    }
    assert!(symbolic.contains("unhandled effect: Wide.op"), "{symbolic}");
    assert!(symbolic.contains("declare void @abort()"));
    assert!(!symbolic.contains("declare void @exit(i32)"));
}

/// The plain build is untouched by the switch: same guard count, `exit`.
#[test]
fn the_native_build_keeps_one_exit_per_guard() {
    let plain = module(DYNAMIC_EFFECT);
    let symbolic = symbolic_module(DYNAMIC_EFFECT);
    assert_eq!(
        plain.matches("call void @exit(i32 1)").count(),
        symbolic.matches("call void @abort()").count()
    );
}
