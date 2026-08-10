//! Staged effects ([STAGE-RESIDUE], [STAGE-SIGNALS-DIRTY],
//! docs/specs/0035-StagedEffects.md).
//!
//! The claim a static effect makes is not "usually optimised away" but
//! *absent*: after the rewrite, the emitted IR contains no handler-runtime
//! symbol for it at all. That is an observable property of the IR text, so it
//! is asserted here rather than argued in prose. The dynamic control case
//! proves these assertions can fail — without it, a compiler that stopped
//! emitting handlers entirely would still pass.

use osprey_codegen::compile_program;
use osprey_syntax::{dependency_sets, parse_program_with_flavor, Flavor};

/// The C runtime symbols a dynamic handler region registers and looks up.
const HANDLER_RUNTIME_SYMBOLS: &[&str] = &["__osprey_handler_push", "__osprey_handler_lookup"];

/// Parse — which discharges static handlers at the flavor boundary
/// ([STAGE-LOWER-ORDER-PHASE]) — and emit LLVM IR.
fn compile_staged(source: &str) -> String {
    let parsed = parse_program_with_flavor(source, Flavor::Default);
    assert!(
        parsed.errors.is_empty(),
        "parse errors: {:?}",
        parsed.errors
    );
    compile_program(&parsed.program).expect("codegen")
}

const STATIC_SOURCE: &str = r#"
static effect Alloc { scratch: fn(int) -> int }
fn main() = {
    let used = handle static Alloc
        scratch bytes => bytes * 4 ?: 0
    in perform Alloc.scratch(16)
    print("scratch=${used}")
}
"#;

const DYNAMIC_SOURCE: &str = r#"
effect Alloc { scratch: fn(int) -> int }
fn main() = {
    let used = handle Alloc
        scratch bytes => bytes * 4 ?: 0
    in perform Alloc.scratch(16)
    print("scratch=${used}")
}
"#;

#[test]
fn a_static_handler_leaves_no_runtime_residue() {
    let ir = compile_staged(STATIC_SOURCE);
    for symbol in HANDLER_RUNTIME_SYMBOLS {
        assert!(
            !ir.contains(symbol),
            "static discharge must leave no `{symbol}` in the emitted IR"
        );
    }
    assert!(
        !ir.contains("Alloc"),
        "the discharged effect must not appear in the emitted IR"
    );
}

#[test]
fn the_dynamic_twin_still_uses_the_handler_runtime() {
    let ir = compile_staged(DYNAMIC_SOURCE);
    for symbol in HANDLER_RUNTIME_SYMBOLS {
        assert!(
            ir.contains(symbol),
            "a dynamic handler must still reach `{symbol}` — otherwise the \
             residue assertion above proves nothing"
        );
    }
}

#[test]
fn dependencies_are_derived_transitively_and_exactly() {
    let source = r#"
static effect CountSignal { read: fn() -> int }
static effect NameSignal { read: fn() -> string }
fn doubled() = (perform CountSignal.read() * 2) ?: 0
fn counterLabel() = "count: ${doubled()}"
fn greeting() = "hello ${perform NameSignal.read()}"
fn statusBar() = "${greeting()} | ${counterLabel()}"
fn footer() = "osprey"
"#;
    let deps = dependency_sets(source, Flavor::Default);
    let of = |name: &str| deps.get(name).cloned().unwrap_or_default();
    assert_eq!(of("doubled"), vec!["CountSignal.read"]);
    // Transitive through a call, and only what is actually read.
    assert_eq!(of("counterLabel"), vec!["CountSignal.read"]);
    assert_eq!(of("greeting"), vec!["NameSignal.read"]);
    assert_eq!(
        of("statusBar"),
        vec!["CountSignal.read", "NameSignal.read"],
        "a caller depends on the union of what it reaches"
    );
    assert!(
        of("footer").is_empty(),
        "a function that reads nothing has no dependencies and never rebuilds"
    );
}

#[test]
fn a_function_that_answers_a_signal_does_not_depend_on_it() {
    let source = r#"
static effect CountSignal { read: fn() -> int }
fn label() = "count: ${perform CountSignal.read()}"
fn root() = handle static CountSignal
    read => 7
in label()
"#;
    let deps = dependency_sets(source, Flavor::Default);
    assert_eq!(
        deps.get("label").cloned().unwrap_or_default(),
        vec!["CountSignal.read"]
    );
    assert!(
        deps.get("root").cloned().unwrap_or_default().is_empty(),
        "a region that answers the signal is not a dependent of it"
    );
}

#[test]
fn nested_regions_answer_the_same_effect_differently() {
    let source = r#"
static effect Tile { size: fn() -> int }
fn scaled(n) = (n * perform Tile.size()) ?: 0
fn main() = {
    let a = handle static Tile
        size => 8
    in scaled(2)
    let b = handle static Tile
        size => 3
    in scaled(2)
    print("${a}/${b}")
}
"#;
    let ir = compile_staged(source);
    assert!(
        !ir.contains("Tile"),
        "both regions must be fully discharged"
    );
    assert!(
        ir.contains("scaled__stage1") && ir.contains("scaled__stage2"),
        "each region owns its own copy of the helper it reaches"
    );
}
