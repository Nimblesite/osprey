//! Library exports are independent effect roots. [IOS-HOST-ABI]

use crate::{check_program, check_program_exports};

/// [LSP-EFFECT-REQUIREMENTS] Expose the entry proof, not written upper bounds.
#[test]
fn published_effect_requirements_preserve_discharge_and_uncertainty() -> Result<(), String> {
    for (source, flavor) in REQUIREMENT_PROGRAMS {
        let parsed = osprey_syntax::parse_program_with_flavor(source, flavor);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        assert_eq!(
            check_program(&parsed.program),
            Vec::<crate::TypeError>::new()
        );
        let types = crate::infer_program(&parsed.program);
        for (name, operations, runtime, unresolved) in REQUIREMENT_EXPECTATIONS {
            let report = types.function_effects.get(name).ok_or(name)?;
            assert_eq!(report.operations, operations, "{flavor}: {name}");
            assert_eq!(report.runtime_builtins, runtime, "{flavor}: {name}");
            assert_eq!(report.unresolved_callbacks, unresolved, "{flavor}: {name}");
        }
    }
    Ok(())
}

const REQUIREMENT_EXPECTATIONS: [(&str, &[&str], &[&str], bool); 11] = [
    ("leaf", &["Read<int>.get"], &[], false),
    ("pair", &["Read<int>.get", "Read<string>.get"], &[], false),
    ("partial", &["Read<string>.get"], &[], false),
    ("forwarded", &["Read<int>.get"], &[], false),
    ("apply", &[], &[], true),
    ("known", &["Read<int>.get"], &[], false),
    ("arithmetic", &["Arith.overflow"], &[], false),
    ("safeArithmetic", &[], &[], false),
    ("printed", &[], &["print"], false),
    ("annotated", &[], &[], false),
    ("recursive", &["Read<int>.get"], &[], false),
];

const REQUIREMENT_PROGRAMS: [(&str, osprey_syntax::Flavor); 2] = [
    (
        r#"effect Read<T> { get: fn() -> T }
fn leaf() = perform Read<int>.get()
fn pair() = {
    let first = leaf()
    perform Read<string>.get()
}
fn partial() = {
    handle Read<int> { get => 42 }
    pair()
}
fn forwarded() = {
    handle Read<int> { get => perform Read<int>.get() }
    leaf()
}
fn apply(f) = f()
fn known() = apply(leaf)
fn arithmetic(x) = x + 1
fn safeArithmetic(x) = {
    handle Arith { overflow op left right wrapped => wrapped }
    arithmetic(x)
}
fn printed() = print("hello")
fn annotated() -> int ![Read<int>] = 42
fn recursive(n) = if n == 0 { leaf() } else { recursive(wrapSub(n, 1)) }
"#,
        osprey_syntax::Flavor::Default,
    ),
    (
        r#"effect Read T
    get : Unit => T
leaf () = perform Read<int>.get ()
pair () =
    first = leaf ()
    perform Read<string>.get ()
partial () =
    handle Read<int>
        get => 42
    pair ()
forwarded () =
    handle Read<int>
        get => perform Read<int>.get ()
    leaf ()
apply f = f ()
known () = apply leaf
arithmetic x = x + 1
safeArithmetic x =
    handle Arith
        overflow op left right wrapped => wrapped
    arithmetic x
printed () = print "hello"
annotated : Unit -> int ![Read<int>]
annotated () = 42
recursive n = match n == 0
    true => leaf ()
    false => recursive (wrapSub n 1)
"#,
        osprey_syntax::Flavor::Ml,
    ),
];

/// [TYPE-RENDER-HOLES] Inference IDs are not source names; nominal names survive.
#[test]
fn published_effect_instances_never_expose_private_inference_names() -> Result<(), String> {
    for (source, flavor) in [
        ("effect Read<T> { get: fn() -> T }\ntype t123 = Tag\nfn generic() = perform Read.get()\nfn nominal() = perform Read<t123>.get()\n", osprey_syntax::Flavor::Default),
        ("effect Read T\n    get : Unit => T\ntype t123 = Tag\ngeneric () = perform Read.get ()\nnominal () = perform Read<t123>.get ()\n", osprey_syntax::Flavor::Ml),
    ] {
        let parsed = osprey_syntax::parse_program_with_flavor(source, flavor);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        assert_eq!(check_program(&parsed.program), Vec::<crate::TypeError>::new());
        let types = crate::infer_program(&parsed.program);
        for (name, expected) in [("generic", "Read<_>.get"), ("nominal", "Read<t123>.get")] {
            let report = types.function_effects.get(name).ok_or(name)?;
            assert_eq!(report.operations, [expected], "{flavor}: {name}");
        }
    }
    Ok(())
}

fn program(source: &str) -> osprey_ast::Program {
    let parsed = osprey_syntax::parse_program(source);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    assert_eq!(
        check_program(&parsed.program),
        Vec::<crate::error::TypeError>::new()
    );
    parsed.program
}

#[test]
fn every_export_requires_its_own_handlers() {
    let p = program("effect Alarm { ring: fn() -> int }\nfn ring() = perform Alarm.ring()\nfn relay() = ring()\nfn safe() = {\n    handle Alarm {\n        ring => 7\n    }\n    relay()\n}\n");
    assert_eq!(
        check_program_exports(&p, &["safe"]),
        Vec::<crate::error::TypeError>::new()
    );
    let errors = check_program_exports(&p, &["ring", "relay", "safe"]);
    assert_eq!(errors.len(), 2, "{errors:?}");
    for name in ["ring", "relay"] {
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains(&format!("library export `{name}`"))
                    && e.message.contains("Alarm.ring")),
            "{errors:?}"
        );
    }
}

#[test]
fn missing_exports_are_reported_even_without_any_effects() {
    let p = program("fn answer() = 42\n");
    assert_eq!(
        check_program_exports(&p, &["answer"]),
        Vec::<crate::error::TypeError>::new()
    );
    let errors = check_program_exports(&p, &["missing"]);
    assert!(
        errors.iter().any(|e| e
            .message
            .contains("library export `missing` is not a defined function")),
        "{errors:?}"
    );
}

#[test]
fn exported_callbacks_with_unresolved_effects_are_rejected() {
    let p = program("effect Alarm { ring: fn() -> int }\nfn invoke(f) = f()\n");
    let errors = check_program_exports(&p, &["invoke"]);
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("library export `invoke`")
                && e.message
                    .contains("callback whose effects cannot be discharged")),
        "{errors:?}"
    );
}

#[test]
fn reactive_snapshots_preserve_escaped_callback_effects() {
    for (flavor, source) in [
        (osprey_syntax::Flavor::Default, "effect Audit { write: fn() -> int }\nlet cached = signalSnapshot(|| => || => perform Audit.write())\nprint(cached.value())\n"),
        (osprey_syntax::Flavor::Ml, "effect Audit\n    write : Unit => int\ncached = signalSnapshot (\\() => \\() => perform Audit.write ())\nprint (cached.value ())\n"),
    ] {
        let parsed = osprey_syntax::parse_program_with_flavor(source, flavor);
        assert!(parsed.errors.is_empty(), "{flavor:?}: {:?}", parsed.errors);
        let errors = check_program(&parsed.program);
        assert!(errors.iter().any(|error| error.message.contains("Audit.write") && error.message.contains("unhandled")), "{flavor:?}: {errors:?}");
    }
}

#[test]
fn dependency_analysis_rejects_invalid_staging_before_reporting() {
    let source = "static effect Signal { read: fn() -> int }\nfn work() = { handle Signal { read => 42 }\nperform Signal.read() }\n";
    let parsed = osprey_syntax::parse_program_with_flavor(source, osprey_syntax::Flavor::Default);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let report = crate::signal_dependencies(&parsed.program);
    assert!(report.is_err_and(|errors| errors
        .iter()
        .any(|error| error.message.contains("Signal") && error.message.contains("static"))));
}

#[test]
fn unresolved_generic_signal_metadata_keeps_an_unknown_remainder() {
    let source = "static effect Signal<T> { read: fn(T) -> T }\nfn generic(value) = perform Signal.read(value)\nfn dependencies(value) = signalDependencies(|| => generic(value))\n";
    let parsed = osprey_syntax::parse_program_with_flavor(source, osprey_syntax::Flavor::Default);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let report = crate::signal_dependencies(&parsed.program).expect("valid generic signal source");
    assert!(
        report
            .get("generic")
            .is_some_and(|operations| operations.iter().any(|operation| operation == "<unknown>")),
        "{report:?}"
    );
}

#[test]
fn static_snapshot_callables_never_capture_the_construction_handler_silently() {
    for (flavor, source) in [
        (osprey_syntax::Flavor::Default, "static effect Signal { read: fn() -> int }\nfn count() = perform Signal.read()\nlet cached = { handle static Signal { read => 1 }\nsignalSnapshot(|| => || => count()) }\nlet answer = { handle static Signal { read => 2 }\ncached.value() }\nprint(answer)\n"),
        (osprey_syntax::Flavor::Ml, "static effect Signal\n    read : Unit => int\ncount () = perform Signal.read ()\ncached =\n    handle static Signal\n        read => 1\n    signalSnapshot (\\() => \\() => count ())\nanswer =\n    handle static Signal\n        read => 2\n    cached.value ()\nprint answer\n"),
    ] {
        assert_snapshot_rejected(flavor, source);
    }
}

#[test]
fn static_snapshot_iterators_never_capture_the_construction_handler_silently() {
    for (flavor, source) in [
        (osprey_syntax::Flavor::Default, "static effect Signal { read: fn() -> int }\nlet cached = { handle static Signal { read => 1 }\nsignalSnapshot(|| => range(0, 2) |> map(fn(_i) => perform Signal.read())) }\nlet values = { handle static Signal { read => 2 }\ncached.value |> toList() }\nprint(length(values))\n"),
        (osprey_syntax::Flavor::Ml, "static effect Signal\n    read : Unit => int\ncached =\n    handle static Signal\n        read => 1\n    signalSnapshot (\\() => range 0 2 |> map (\\_i => perform Signal.read ()))\nvalues =\n    handle static Signal\n        read => 2\n    cached.value |> toList\nprint (length values)\n"),
    ] {
        assert_snapshot_rejected(flavor, source);
    }
}

fn assert_snapshot_rejected(flavor: osprey_syntax::Flavor, source: &str) {
    let parsed = osprey_syntax::parse_program_with_flavor(source, flavor);
    assert!(parsed.errors.is_empty(), "{flavor:?}: {:?}", parsed.errors);
    let errors = check_program(&parsed.program);
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("Signal.read")
                && error.message.contains("invocation-time specialization")
                && error.position.is_some()),
        "{flavor:?}: {errors:?}"
    );
}
