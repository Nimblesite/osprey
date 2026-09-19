//! Staged effects ([STAGE-RESIDUE], [STAGE-SIGNALS-DIRTY],
//! docs/specs/0035-StagedEffects.md).
//!
//! The claim a static effect makes is not "usually optimised away" but
//! *absent*: after the rewrite, the emitted IR contains no handler-runtime
//! symbol for it at all. That is an observable property of the IR text, so it
//! is asserted here rather than argued in prose. The dynamic control case
//! proves these assertions can fail — without it, a compiler that stopped
//! emitting handlers entirely would still pass.

#[path = "common/staging.rs"]
mod staging;

use osprey_syntax::{dependency_report, Flavor};
use staging::{compile_for_target, compile_staged, diagnostics};

/// The C runtime symbols a dynamic handler region registers and looks up.
const HANDLER_RUNTIME_SYMBOLS: &[&str] = &["__osprey_handler_push", "__osprey_handler_lookup"];

const STATIC_SOURCE: &str = r#"
static effect Alloc { scratch: fn(int) -> int }
fn main() = {
    let used = {
        handle static Alloc {
            scratch bytes => bytes * 4 ?: 0
        }
        perform Alloc.scratch(16)
    }
    print("scratch=${used}")
}
"#;

const DYNAMIC_SOURCE: &str = r#"
effect Alloc { scratch: fn(int) -> int }
fn main() = {
    let used = {
        handle Alloc {
            scratch bytes => bytes * 4 ?: 0
        }
        perform Alloc.scratch(16)
    }
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
    let deps = dependency_report(source, Flavor::Default).0;
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
fn root() = {
    handle static CountSignal {
        read => 7
    }
    label()
}
"#;
    let deps = dependency_report(source, Flavor::Default).0;
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
    let a = {
        handle static Tile {
            size => 8
        }
        scaled(2)
    }
    let b = {
        handle static Tile {
            size => 3
        }
        scaled(2)
    }
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

// ---------------------------------------------------------------------------
// The rules of docs/specs/0035-StagedEffects.md that reach past discharge: the
// target a static row may be built for, the offload boundary a kernel is, and
// the identity a reactive read carries. Each test below states the spec's
// contract in the form the spec states it, so the rule cannot regress into a
// comment saying it once worked. Every one names the spec ID it is the
// executable form of.
// ---------------------------------------------------------------------------

const STATIC_ROW_SOURCE: &str = r#"
static effect Scale { by: fn() -> int }
fn scaled(n) = (n * perform Scale.by()) ?: 0
fn main() = {
    let v = {
        handle static Scale {
            by => 3
        }
        scaled(2)
    }
    print("v=${v}")
}
"#;

#[test]
fn a_static_row_needs_no_stack_switching_on_wasm32() {
    // [STAGE-WASM] Static handlers require no continuation, so they are
    // available on every target. This is the half of the stage payoff that
    // already holds, and it is asserted so a regression in discharge ordering
    // cannot quietly push static effects back onto the native-only path.
    let (ok, err) = compile_for_target(STATIC_ROW_SOURCE, "wasm32");
    assert!(ok, "a wholly static row must compile for wasm32: {err}");
}

#[test]
fn a_static_effect_inside_a_kernel_body_is_stage_legal() {
    // [STAGE-GPU-LEGAL] generalizes [GPU-KERNEL-PURE] from "empty row" to
    // "empty *dynamic* row". A kernel that answers its requests before it runs
    // is legal, and it is legal through the existing purity gate rather than a
    // second one: discharge follows data typing but precedes residual purity.
    let source = r#"
static effect Tile { size: fn() -> int }
fn shade(px) = (px * perform Tile.size()) ?: 0
fn main() = {
    let out = {
        handle static Tile {
            size => 3
        }
        fromGpu(toGpu([1, 2, 3]) |> gpuMap(shade))
    }
    print("${listGet(out, 0) ?: 0}")
}
"#;
    let errors = diagnostics(source, Flavor::Default);
    assert!(
        errors.is_empty(),
        "a kernel whose only requests are static must be accepted: {errors}"
    );
    // The region must ENCLOSE the kernel: a static handler installed after
    // the offload answers nothing the device code asked for, so the request
    // is residual and the kernel is rejected by name.
    let outside = r#"
static effect Tile { size: fn() -> int }
fn shade(px) = (px * perform Tile.size()) ?: 0
fn main() = {
    let out = fromGpu(toGpu([1, 2, 3]) |> gpuMap(shade))
    handle static Tile {
        size => 3
    }
    print("${listGet(out, 0) ?: 0}")
}
"#;
    let errors = diagnostics(outside, Flavor::Default);
    assert!(
        errors.contains("kernel body is not stage-legal; it requires dynamic effects: Tile.size"),
        "a kernel outside the static region keeps its residual request: {errors}"
    );
}

#[test]
fn a_kernel_region_is_a_handler_region_not_a_magic_block() {
    // [STAGE-GPU-KERNEL] `kernel` is a handler region whose signature admits
    // only stage-legal rows, supplying the static handlers for the device
    // dialects its body may use. There is no such form today: the surface does
    // not parse, so a kernel cannot carry its own dialect handlers and the
    // device effects `Parallel`, `Alloc` and `Tensor` have nowhere to be
    // answered.
    let source = r#"
static effect Tile { size: fn() -> int }
fn shade(px) = (px * perform Tile.size()) ?: 0
fn main() = {
    let frame = kernel
        Tile size => 8
    in shade(2)
    print("${frame}")
}
"#;
    let errors = diagnostics(source, Flavor::Default);
    assert!(
        errors.is_empty(),
        "a `kernel` region supplying its dialect handlers must be accepted: {errors}"
    );
}

#[test]
fn an_unstage_legal_kernel_says_so_instead_of_only_saying_impure() {
    // [STAGE-GPU-DIAG] Stage adds the case where the checker CAN see the
    // body's provenance and the answer is no. The existing fail-closed message
    // ("cannot prove GPU kernel pure") is retained for the case where it
    // cannot; it is the wrong message here, because it describes an absence of
    // evidence rather than the evidence of a dynamic row.
    let source = r#"
effect Log { control write: fn(string) -> Unit }
fn noisy(px) = {
    perform Log.write("px")
    px
}
fn main() = {
    let out = {
        handle Log {
            write msg => resume(print(msg))
        }
        fromGpu(toGpu([1, 2, 3]) |> gpuMap(noisy))
    }
    print("${listGet(out, 0) ?: 0}")
}
"#;
    let errors = diagnostics(source, Flavor::Default);
    assert!(
        errors.contains("not stage-legal") && errors.contains("Log.write"),
        "expected the stage-legality diagnostic naming the dynamic effect, got: {errors}"
    );
}

#[test]
fn signal_identity_is_the_generic_instantiation() {
    // [STAGE-SIGNALS-EXACT] "Signal identity is the generic instantiation:
    // `Signal<Count>` and `Signal<Cursor>` are distinct dependencies." That is
    // the surface contract the whole reactive story rests on — a widget's
    // dirty set is its row — and the explicit instantiation it requires at the
    // `perform` and `handle` sites does not parse today, so two signals are
    // indistinguishable in a row.
    let source = r#"
type Count = { value: int }
type Cursor = { at: int }
static effect Signal<T> { read: fn() -> T }
fn counterLabel() = "count: ${(perform Signal<Count>.read()).value}"
fn cursorLabel() = "at: ${(perform Signal<Cursor>.read()).at}"
"#;
    let deps = dependency_report(source, Flavor::Default).0;
    let of = |name: &str| deps.get(name).cloned().unwrap_or_default();
    assert_eq!(
        of("counterLabel"),
        vec!["Signal<Count>.read"],
        "a view depends on the instantiation it reads, not the bare effect"
    );
    assert_eq!(of("cursorLabel"), vec!["Signal<Cursor>.read"]);
}

#[test]
fn the_ml_surface_carries_the_stage_axis_too() {
    // [STAGE-DECL] The spec gives `static effect` and `handle static` an ML
    // spelling. Both lower to the same staged AST and pass through source
    // validation before discharge ([STAGE-LOWER-ORDER-PHASE]).
    let source = "static effect Tile\n    size : Unit => int\n\nscaled n = n * perform Tile.size () ?: 0\n\nanswer =\n    handle static Tile\n        size => 8\n    scaled 2\n";
    let errors = diagnostics(source, Flavor::Ml);
    assert!(
        errors.is_empty(),
        "the ML surface must declare and discharge a static effect: {errors}"
    );
}

/// Static substitution must preserve the declaration's data contract, including
/// arms whose operations are never requested. [STAGE-LOWER-ORDER-PHASE]
#[test]
fn static_contracts_are_checked_before_erasure_in_both_flavors() {
    let cases = [
        (
            "static effect Read { value: fn() -> int }\nlet answer = {\n    handle static Read {\n        value => \"wrong\"\n    }\n    length(perform Read.value())\n}\nprint(answer)",
            "static effect Read\n    value : Unit => int\n\nanswer =\n    handle static Read\n        value => \"wrong\"\n    length (perform Read.value ())\nprint answer\n",
            "cannot unify int with string",
        ),
        (
            "static effect Read { value: fn() -> int }\nlet answer = {\n    handle static Read {\n        value => \"wrong\"\n    }\n    42\n}\nprint(answer)",
            "static effect Read\n    value : Unit => int\n\nanswer =\n    handle static Read\n        value => \"wrong\"\n    42\nprint answer\n",
            "cannot unify int with string",
        ),
        (
            "static effect Echo { value: fn(int) -> int }\nlet answer = {\n    handle static Echo {\n        value input => length(input)\n    }\n    perform Echo.value(\"bad\")\n}\nprint(answer)",
            "static effect Echo\n    value : int => int\n\nanswer =\n    handle static Echo\n        value input => length input\n    perform Echo.value \"bad\"\nprint answer\n",
            "cannot unify",
        ),
        (
            "static effect Echo { value: fn(int) -> int }\nlet answer = {\n    handle static Echo {\n        value => 7\n    }\n    42\n}\nprint(answer)",
            "static effect Echo\n    value : int => int\n\nanswer =\n    handle static Echo\n        value => 7\n    42\nprint answer\n",
            "expects 1 parameter(s), got 0",
        ),
        (
            "static effect Echo<T> { value: fn() -> T }\nlet answer = {\n    handle static Echo<int> {\n        value => \"bad\"\n    }\n    length(perform Echo<int>.value())\n}\nprint(answer)",
            "static effect Echo T\n    value : Unit => T\n\nanswer =\n    handle static Echo<int>\n        value => \"bad\"\n    length (perform Echo<int>.value ())\nprint answer\n",
            "cannot unify int with string",
        ),
    ];
    for (default, ml, expected) in cases {
        for (flavor, source) in [(Flavor::Default, default), (Flavor::Ml, ml)] {
            let errors = diagnostics(source, flavor);
            assert!(
                errors.contains(expected),
                "{flavor:?}: expected {expected}, got {errors}"
            );
        }
    }
}

#[test]
fn staging_does_not_authorize_source_mutation_outside_handler_arms() {
    let source = "static effect Read { value: fn() -> int }\nfn main() = {\nmut count = 0\ncount = 1\nlet answer = {\n    handle static Read {\n        value => count\n    }\n    perform Read.value()\n}\nprint(answer)\n}";
    let errors = diagnostics(source, Flavor::Default);
    assert!(
        errors.contains("state mutation is only allowed inside an effect handler arm"),
        "{errors}"
    );
}

#[test]
fn a_static_region_missing_an_operation_names_the_operation_it_left_out() {
    // [STAGE-STATIC-TOTAL] A static handler must answer every operation, and
    // structural validation says so before the rewrite runs: it names the
    // uncovered operation rather than leaving the request to surface later as
    // an unhandled effect ([STAGE-LOWER-ORDER-PHASE]).
    let source = r#"
static effect Tensor {
    stride: fn() -> int
    rank: fn() -> int
}
fn main() = {
    handle static Tensor {
        stride => 3
    }
    print("${perform Tensor.stride()}")
}
"#;
    let errors = diagnostics(source, Flavor::Default);
    assert!(
        errors.contains("static handler for `Tensor` does not cover operation `Tensor.rank`"),
        "a partial static handler must name the uncovered operation: {errors}"
    );
}

#[test]
fn a_static_region_over_an_undeclared_effect_is_named_by_its_own_keyword() {
    // The diagnostic spells the region the way the source does, so a reader
    // is not told about `handle` when they wrote `handle static`.
    let source = r#"
fn main() = {
    handle static Missing {
        get => 1
    }
    print("unreachable")
}
"#;
    let errors = diagnostics(source, Flavor::Default);
    assert!(
        errors.contains("handle static names unknown effect `Missing`"),
        "an unknown static effect must be reported against `handle static`: {errors}"
    );
}
