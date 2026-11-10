//! Staged effects ([STAGE-RESIDUE], [STAGE-SIGNALS-DIRTY],
//! docs/specs/language/0017-AlgebraicEffects.md).
//!
//! The claim a static effect makes is not "usually optimised away" but
//! *absent*: after the rewrite, the emitted IR contains no handler-runtime
//! symbol for it at all. That is an observable property of the IR text, so it
//! is asserted here rather than argued in prose. The dynamic control case
//! proves these assertions can fail — without it, a compiler that stopped
//! emitting handlers entirely would still pass.

#[path = "common/parse_clean.rs"]
mod parse_clean;
#[path = "common/residue.rs"]
mod residue;
#[path = "common/staging.rs"]
mod staging;

use std::collections::BTreeMap;

use osprey_syntax::Flavor;
use parse_clean::parse_clean;
use residue::HANDLER_RUNTIME_SYMBOLS;
use staging::{assert_rejected, compile_for_target, compile_staged, diagnostics};

/// Asserts `result` is `Ok` under `label`, then unwraps it.
fn expect_ok<T: Default + std::fmt::Debug, E: std::fmt::Debug>(
    result: Result<T, E>,
    label: &str,
) -> T {
    assert!(result.is_ok(), "{label}: {result:?}");
    result.unwrap_or_default()
}

/// The dependency report for `source`, asserting the fixture's contracts hold.
fn dependency_report(source: &str, flavor: Flavor) -> BTreeMap<String, Vec<String>> {
    let parsed = parse_clean(source, flavor, "invalid dependency fixture");
    expect_ok(
        osprey_types::signal_dependencies(&parsed.program),
        "invalid signal contracts",
    )
}

/// The operations `name` depends on per `deps`, empty when it depends on none.
fn deps_of(deps: &BTreeMap<String, Vec<String>>, name: &str) -> Vec<String> {
    deps.get(name).cloned().unwrap_or_default()
}

/// Asserts `ir` mentions no handler-runtime symbol, `label` naming the case.
fn assert_no_runtime_residue(ir: &str, label: &str) {
    for symbol in HANDLER_RUNTIME_SYMBOLS {
        assert!(
            !ir.contains(symbol),
            "{label}: `{symbol}` must not survive discharge"
        );
    }
}

/// Asserts the frontend accepts `source` in `flavor`, `label` naming the rule.
fn assert_accepted(source: &str, flavor: Flavor, label: &str) {
    let errors = diagnostics(source, flavor);
    assert!(errors.is_empty(), "{label}: {errors}");
}

/// Asserts the default flavor rejects `source` with a diagnostic naming `expected`.
fn assert_names(source: &str, expected: &str) {
    let errors = diagnostics(source, Flavor::Default);
    assert!(errors.contains(expected), "expected `{expected}`: {errors}");
}

/// Emits IR for `source` in `flavor`, `label` naming the case on failure.
fn compile_in_flavor(source: &str, flavor: Flavor, label: &str) -> String {
    if flavor == Flavor::Default {
        return compile_staged(source);
    }
    let parsed = parse_clean(source, flavor, label);
    expect_ok(
        osprey_codegen::compile_program(&parsed.program),
        &format!("{label}: static codegen"),
    )
}

const STATIC_SOURCE: &str = r#"
static effect Alloc { scratch: fn(int) -> int }
fn main() = {
    let used = {
        handle static Alloc {
            scratch bytes => satMul(bytes, 4)
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
            scratch bytes => satMul(bytes, 4)
        }
        perform Alloc.scratch(16)
    }
    print("scratch=${used}")
}
"#;

#[test]
fn a_discharged_effect_leaves_no_dispatch_or_runtime_residue() {
    for (label, source, flavor, effect) in [
        ("static handler", STATIC_SOURCE, Flavor::Default, "Alloc"),
        ("open-row helper", "effect Read { value: fn() -> int }\nfn relay(callback) -> int !e = callback()\nfn work() !Read = perform Read.value()\nlet answer = {\n    handle static Read { value => 41 }\n    relay(work)\n}\nprint(answer)\n", Flavor::Default, "Read"),
        ("ML open-row helper", "effect Read\n    value : Unit => int\nrelay : (Unit -> int) -> int !e\nrelay callback = callback ()\nwork : Unit -> int !Read\nwork () = perform Read.value ()\nanswer =\n    handle static Read\n        value => 41\n    relay work\nprint answer\n", Flavor::Ml, "Read"),
    ] {
        let ir = compile_in_flavor(source, flavor, label);
        assert_no_runtime_residue(&ir, label);
        assert!(
            !ir.contains(effect),
            "{label}: `{effect}` must not appear in the emitted IR"
        );
    }
}

#[test]
fn the_dynamic_twin_still_uses_the_handler_runtime() {
    let ir = compile_staged(DYNAMIC_SOURCE);
    // Without this control, a compiler that stopped emitting handlers entirely
    // would still pass the residue assertions above.
    for symbol in HANDLER_RUNTIME_SYMBOLS {
        assert!(
            ir.contains(symbol),
            "a dynamic handler must still reach `{symbol}`"
        );
    }
}

#[test]
fn dependencies_are_derived_transitively_and_exactly() {
    let source = r#"
static effect CountSignal { read: fn() -> int }
static effect NameSignal { read: fn() -> string }
fn doubled() = satMul(perform CountSignal.read(), 2)
fn counterLabel() = "count: ${doubled()}"
fn greeting() = "hello ${perform NameSignal.read()}"
fn statusBar() = "${greeting()} | ${counterLabel()}"
fn footer() = "osprey"
"#;
    let deps = dependency_report(source, Flavor::Default);
    assert_eq!(deps_of(&deps, "doubled"), vec!["CountSignal.read"]);
    // Transitive through a call, and only what is actually read.
    assert_eq!(deps_of(&deps, "counterLabel"), vec!["CountSignal.read"]);
    assert_eq!(deps_of(&deps, "greeting"), vec!["NameSignal.read"]);
    assert_eq!(
        deps_of(&deps, "statusBar"),
        vec!["CountSignal.read", "NameSignal.read"],
        "a caller depends on the union of what it reaches"
    );
    assert!(
        deps_of(&deps, "footer").is_empty(),
        "a function that reads nothing has no dependencies and never rebuilds"
    );
}

#[test]
fn dependency_report_keeps_unknown_callback_requirements_visible() {
    let source = "static effect Signal { read: fn() -> int }\nfn invoke(callback) = callback()\nfn reading() = perform Signal.read()\nfn widget() = invoke(reading)\n";
    let deps = dependency_report(source, Flavor::Default);
    assert_eq!(deps.get("invoke"), Some(&vec!["<unknown>".to_owned()]));
    assert_eq!(deps.get("widget"), Some(&vec!["Signal.read".to_owned()]),);
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
    let deps = dependency_report(source, Flavor::Default);
    assert_eq!(deps_of(&deps, "label"), vec!["CountSignal.read"]);
    assert!(
        deps_of(&deps, "root").is_empty(),
        "a region that answers the signal is not a dependent of it"
    );
}

#[test]
fn nested_regions_answer_the_same_effect_differently() {
    let source = r#"
static effect Tile { size: fn() -> int }
fn scaled(n) = satMul(n, perform Tile.size())
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
// The rules of docs/specs/language/0017-AlgebraicEffects.md that reach past discharge: the
// target a static row may be built for, the offload boundary a kernel is, and
// the identity a reactive read carries. Each test below states the spec's
// contract in the form the spec states it, so the rule cannot regress into a
// comment saying it once worked. Every one names the spec ID it is the
// executable form of.
// ---------------------------------------------------------------------------

const STATIC_ROW_SOURCE: &str = r#"
static effect Scale { by: fn() -> int }
fn scaled(n) = satMul(n, perform Scale.by())
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
    let program = r"
static effect Tile { size: fn() -> int }
fn shade(px) = satMul(px, perform Tile.size())
fn main() = {
    let out = {
        @inside@
        match toGpu([1, 2, 3]) {
            Success { value } => match gpuMap(value, shade) {
                Success { value } => fromGpu(value)
                Error { message } => Error { message: message }
            }
            Error { message } => Error { message: message }
        }
    }
    @outside@
    match out {
        Success { value } => print(listLength(value))
        Error { message } => print(toString(message))
    }
}
";
    let handler = "handle static Tile { size => 3 }";
    let source = program
        .replace("@inside@", handler)
        .replace("@outside@", "");
    assert_accepted(
        &source,
        Flavor::Default,
        "only static requests are legal in a kernel",
    );
    // The region must ENCLOSE the kernel: a static handler installed after
    // the offload answers nothing the device code asked for, so the request
    // is residual and the kernel is rejected by name.
    let outside = program
        .replace("@inside@", "")
        .replace("@outside@", handler);
    let residual = "kernel body is not stage-legal; it requires dynamic effects: Tile.size";
    assert_names(&outside, residual);
}

#[test]
fn a_kernel_region_is_a_handler_region_not_a_magic_block() {
    // [STAGE-GPU-KERNEL] `kernel` is a handler region whose signature admits
    // only stage-legal rows, supplying the static handlers for the device
    // dialects its body may use, so a kernel answers its own device requests
    // without an enclosing `handle static`.
    let source = r#"
static effect Tile { size: fn() -> int }
fn shade(px) = satMul(px, perform Tile.size())
fn main() = {
    let frame = kernel
        Tile size => 8
    in shade(2)
    print("${frame}")
}
"#;
    assert_accepted(
        source,
        Flavor::Default,
        "a kernel region supplies its dialect handlers",
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
    assert_rejected(
        source,
        Flavor::Default,
        &["not stage-legal", "Log.write"],
        "expected the stage-legality diagnostic naming the dynamic effect",
    );
}

#[test]
fn signal_identity_is_the_generic_instantiation() {
    // [STAGE-SIGNALS-EXACT] "Signal identity is the generic instantiation:
    // `Signal<Count>` and `Signal<Cursor>` are distinct dependencies." That is
    // the surface contract the whole reactive story rests on — a widget's
    // dirty set is its row — and the explicit instantiation it requires at the
    // `perform` and `handle` sites must retain that identity.
    let source = r#"
type Count = { value: int }
type Cursor = { at: int }
static effect Signal<T> { read: fn() -> T }
fn counterLabel() = "count: ${(perform Signal<Count>.read()).value}"
fn cursorLabel() = "at: ${(perform Signal<Cursor>.read()).at}"
"#;
    let deps = dependency_report(source, Flavor::Default);
    assert_eq!(
        deps_of(&deps, "counterLabel"),
        vec!["Signal<Count>.read"],
        "a view depends on the instantiation it reads, not the bare effect"
    );
    assert_eq!(deps_of(&deps, "cursorLabel"), vec!["Signal<Cursor>.read"]);
}

#[test]
fn the_ml_surface_carries_the_stage_axis_too() {
    // [STAGE-DECL] The spec gives `static effect` and `handle static` an ML
    // spelling. Both lower to the same staged AST and pass through source
    // validation before discharge ([STAGE-LOWER-ORDER-PHASE]).
    let source = "static effect Tile\n    size : Unit => int\n\nscaled n = satMul n (perform Tile.size ())\n\nanswer =\n    handle static Tile\n        size => 8\n    scaled 2\n";
    assert_accepted(
        source,
        Flavor::Ml,
        "ML declares and discharges a static effect",
    );
}

/// Static substitution must preserve the declaration's data contract, including
/// arms whose operations are never requested. [STAGE-LOWER-ORDER-PHASE]
#[test]
fn static_contracts_are_checked_before_erasure_in_both_flavors() {
    let cases = [
        ("static effect Read { value: fn() -> int }\nlet answer = {\n    handle static Read {\n        value => \"wrong\"\n    }\n    length(perform Read.value())\n}\nprint(answer)", "static effect Read\n    value : Unit => int\n\nanswer =\n    handle static Read\n        value => \"wrong\"\n    length (perform Read.value ())\nprint answer\n", "cannot unify int with string"),
        ("static effect Read { value: fn() -> int }\nlet answer = {\n    handle static Read {\n        value => \"wrong\"\n    }\n    42\n}\nprint(answer)", "static effect Read\n    value : Unit => int\n\nanswer =\n    handle static Read\n        value => \"wrong\"\n    42\nprint answer\n", "cannot unify int with string"),
        ("static effect Echo { value: fn(int) -> int }\nlet answer = {\n    handle static Echo {\n        value input => length(input)\n    }\n    perform Echo.value(\"bad\")\n}\nprint(answer)", "static effect Echo\n    value : int => int\n\nanswer =\n    handle static Echo\n        value input => length input\n    perform Echo.value \"bad\"\nprint answer\n", "cannot unify"),
        ("static effect Echo { value: fn(int) -> int }\nlet answer = {\n    handle static Echo {\n        value => 7\n    }\n    42\n}\nprint(answer)", "static effect Echo\n    value : int => int\n\nanswer =\n    handle static Echo\n        value => 7\n    42\nprint answer\n", "expects 1 parameter(s), got 0"),
        ("static effect Echo<T> { value: fn() -> T }\nlet answer = {\n    handle static Echo<int> {\n        value => \"bad\"\n    }\n    length(perform Echo<int>.value())\n}\nprint(answer)", "static effect Echo T\n    value : Unit => T\n\nanswer =\n    handle static Echo<int>\n        value => \"bad\"\n    length (perform Echo<int>.value ())\nprint answer\n", "cannot unify int with string"),
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
fn a_static_region_is_rejected_with_a_diagnostic_naming_its_cause() {
    // Staging does not authorize source mutation outside handler arms.
    // [STAGE-STATIC-TOTAL] A static handler must answer every operation, and
    // structural validation says so before the rewrite runs: it names the
    // uncovered operation rather than leaving the request to surface later as
    // an unhandled effect ([STAGE-LOWER-ORDER-PHASE]). The diagnostic spells
    // the region the way the source does, so a reader is not told about
    // `handle` when they wrote `handle static`.
    for (source, expected) in [
        ("static effect Read { value: fn() -> int }\nfn main() = {\nmut count = 0\ncount = 1\nlet answer = {\n    handle static Read {\n        value => count\n    }\n    perform Read.value()\n}\nprint(answer)\n}", "state mutation is only allowed inside an effect handler arm"),
        ("\nstatic effect Tensor {\n    stride: fn() -> int\n    rank: fn() -> int\n}\nfn main() = {\n    handle static Tensor {\n        stride => 3\n    }\n    print(\"${perform Tensor.stride()}\")\n}\n", "static handler for `Tensor` does not cover operation `Tensor.rank`"),
        ("\nfn main() = {\n    handle static Missing {\n        get => 1\n    }\n    print(\"unreachable\")\n}\n", "handle static names unknown effect `Missing`"),
    ] {
        assert_names(source, expected);
    }
}

#[test]
fn a_static_arith_region_is_refused_without_calling_arith_unknown() {
    // `Arith` is compiler-declared ([ARITH-EFFECT-OPS]), so "unknown effect"
    // would be false; it simply has no static interpretation yet.
    for (source, flavor, region) in [
        ("fn main() = {\n    handle static Arith { overflow _ _ _ wrapped => wrapped }\n    print(\"unreachable\")\n}\n", Flavor::Default, "handle static"),
        ("main () =\n    handle static Arith\n        overflow _ _ _ wrapped => wrapped\n    print \"unreachable\"\n", Flavor::Ml, "handle static"),
        ("fn main() = {\n    let frame = kernel\n        Arith overflow _ _ _ wrapped => wrapped\n    in 2\n    print(\"${frame}\")\n}\n", Flavor::Default, "kernel"),
    ] {
        let errors = diagnostics(source, flavor);
        let refusal = format!(
            "`{region} Arith` is not available: `Arith` has runtime policies only; install one with `handle Arith`"
        );
        assert!(errors.contains(&refusal), "{flavor:?}: {errors}");
        assert!(!errors.contains("unknown effect"), "{flavor:?}: {errors}");
    }
}
