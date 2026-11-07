//! Static discharge preserves lexical bindings and call evaluation order.
//! Implements [STAGE-LOWER] and [STAGE-LOWER-ORDER].

#[path = "common/effect_execution.rs"]
mod effect_execution;

use effect_execution::assert_flavored_output;

/// Runs every `(name, source, expected)` case through the default flavor.
fn assert_cases(cases: &[(&str, &str, &str)]) {
    for (name, source, expected) in cases {
        assert_flavored_output(name, "osp", source, expected);
    }
}

/// Lexical scoping cases: shadowing, captures, recursion and namespaces.
const LEXICAL_CASES: &[(&str, &str, &str)] = &[
    (
        "lexical",
        include_str!("../../../../osprey/examples/handlers/staging-scope.osp"),
        "static=1 dynamic=1\n",
    ),
    (
        "helper_shadow",
        r#"
static effect Read { get: fn() -> int }
fn value() = perform Read.get()
let first = {
    handle static Read {
        get => 1
    }
    value()
}
let second = {
    handle static Read {
        get => 2
    }
    {
}
    let value = || => 3
    value()
}
print("${first}:${second}")
"#,
        "1:3\n",
    ),
    (
        "helper_capture",
        r#"
static effect Read { get: fn() -> int }
fn value() = perform Read.get()
fn run(outer) = {
    handle static Read {
        get => outer
    }
    value()
}
print("${run(1)}:${run(2)}")
"#,
        "1:2\n",
    ),
    (
        "nested_scopes",
        r#"
static effect Read { get: fn() -> int }
type Box = { outer: int }
let outer = 1
let value = {
    handle static Read {
        get => outer
    }
    {
}
    let f = |outer| => match Box { outer: outer } {
        Box { outer } => satAdd(perform Read.get(), outer)
    }
    f(2)
}
print("${value}")
"#,
        "3\n",
    ),
    (
        "global_capture",
        r#"
static effect Read { get: fn() -> int }
let amount = 2
fn work() = satAdd(perform Read.get(), amount)
let value = {
    handle static Read {
        get => 1
    }
    work()
}
print("${value}")
"#,
        "3\n",
    ),
    (
        "recursive_capture",
        r#"
static effect Read { get: fn() -> int }
fn readAfterSteps(n) = match n == 0 {
    true => perform Read.get()
    false => readAfterSteps(satSub(n, 1))
}
fn run(outer) = {
    handle static Read {
        get => outer
    }
    readAfterSteps(3)
}
print("${run(1)}:${run(2)}")
"#,
        "1:2\n",
    ),
    (
        "namespaced",
        r#"
static effect Read { get: fn() -> int }
namespace app {
    module Math {
        export fn double(n) = satMul(n, 2)
    }
}
import app::Math
fn value() = perform Read.get()
let total = {
    handle static Read {
        get => 21
    }
    Math::double(value())
}
print("${total}")
"#,
        "42\n",
    ),
];

#[test]
fn lexical_bindings_survive_static_discharge() {
    assert_cases(LEXICAL_CASES);
}

/// Binder, argument-order and signal cases.
const BINDER_CASES: &[(&str, &str, &str)] = &[
        (
            "binders",
            r#"
static effect Shape { pick: fn() -> int }
type Point = { x: int, y: int }
type Tier = Epic { rank: int, bonus: int } | Starter
fn reading() = perform Shape.pick()
let report = {
    handle static Shape {
        pick => 3
    }
    {
        let base = Point { x: reading(), y: 1 }
        let moved = base { y: 4 }
        let ranked = match Epic { rank: reading(), bonus: 2 } {
            Epic { rank } => rank
            Starter => 0
        }
        let spread = match moved {
            { x, y } => satAdd(x, y)
        }
        let tail = match [moved.x, moved.y, ranked] {
            [head, ...rest] => satAdd(head, length(rest))
            [] => 0
        }
        "${base.x}:${moved.y}:${ranked}:${spread}:${tail}"
    }
}
print(report)
"#,
            "3:4:3:7:5\n",
        ),
        (
            "result_binders",
            r#"
static effect Parse { outcome: fn() -> Result<int, string> }
fn decode() = perform Parse.outcome()
let score = {
    handle static Parse {
        outcome => Success { value: 7 }
    }
    match decode() {
        Success { value } => value
        Error { message } => length(message)
    }
}
print("${score}")
"#,
            "7\n",
        ),
        (
            "handle_identity",
            "let first = [\"count\"]\nlet second = [\"cursor\"]\nprint(\"${structuralEquals(first, first)}:${structuralEquals(first, second)}:${!structuralEquals(first, second)}\")\n",
            "true:false:true\n",
        ),
        (
            "arguments",
            r#"
static effect Pair { join: fn(int, int) -> string }
fn mark(value) = {
    print("${value}")
    value
}
let x = 2
let answer = {
    handle static Pair {
        join x y => "${x}:${y}"
    }
    perform Pair.join(mark(1), mark(x))
}
print(answer)
"#,
            "1\n2\n1:2\n",
        ),
        (
            "named_args",
            r#"
static effect Read { get: fn() -> int }
fn add(first, second) = satAdd(first, second)
let value = {
    handle static Read {
        get => 2
    }
    add(second: 3, first: perform Read.get())
}
print("${value}")
"#,
            "5\n",
        ),
        (
            "named_signals",
            "static effect Signal { read: fn() -> int }\nfn count() = perform Signal.read()\nlet first = { handle static Signal { read => 42 }\nsignalSnapshot(work: count) }\nlet next = { handle static Signal { read => 43 }\nsignalRefresh(work: count, changed: signalDependencies(work: count), cached: first) }\nprint(\"${first.value}:${next.value}:${next.revision}\")\n",
            "42:43:2\n",
        ),
        (
            "signal_metadata",
            r#"
static effect Signal<T> { read: fn() -> T }
fn count() = satAdd(perform Signal<int>.read(), 1)
fn title() = perform Signal<string>.read()
fn choose(flag) = match flag { true => toString(count()) false => title() }
fn describe(callback) = signalDependencies(callback)
fn picked(flag) = match flag { true => count false => count }
let first = signalDependencies(count)
let second = signalDependencies(|| => title())
let selected = signalDependencies(|| => choose(true))
let forwarded = describe(count)
let unknown = describe(picked(true))
print("${join(first, ",")}|${join(second, ",")}|${join(selected, ",")}|${join(forwarded, ",")}|${join(unknown, ",")}")
"#,
            "Signal<int>.read|Signal<string>.read|Signal<int>.read,Signal<string>.read|Signal<int>.read|<unknown>\n",
        ),
        (
            "signal_rebuild",
            r#"
static effect Signal<T> { read: fn() -> T }
fn count() = perform Signal<int>.read()
let first = {
    handle static Signal<int> { read => 42 }
    signalSnapshot(count)
}
let changed = {
    handle static Signal<int> { read => 43 }
    signalRefresh(first, signalDependencies(count), count)
}
let unchanged = {
    handle static Signal<int> { read => 44 }
    signalRefresh(changed, [], count)
}
print("${first.value}:${first.revision}|${changed.value}:${changed.revision}|${unchanged.value}:${unchanged.revision}")
"#,
            "42:1|43:2|43:2\n",
        ),
];

#[test]
fn binders_arguments_and_signals_survive_static_discharge() {
    assert_cases(BINDER_CASES);
}

#[test]
fn captured_mutable_state_is_shared_with_specialized_helpers() {
    for (extension, source) in [
        (
            "osp",
            r#"
static effect Counter { next: fn() -> int }
fn twice() = "${perform Counter.next()}:${perform Counter.next()}"
fn run(initial) = {
    mut count = initial
    let result = {
        handle static Counter {
            next => {
                count = satAdd(count, 1)
                count
            }
        }
        twice()
    }
    "${result}:${count}"
}
print("${run(0)}|${run(10)}")
"#,
        ),
        (
            "ospml",
            r#"
static effect Counter
    next : Unit => int
twice () = "${perform Counter.next ()}:${perform Counter.next ()}"
run initial =
    mut count = initial
    result =
        handle static Counter
            next =>
                count := satAdd count 1
                count
        twice ()
    "${result}:${count}"
print "${run 0}|${run 10}"
"#,
        ),
    ] {
        assert_flavored_output("mutable_capture", extension, source, "1:2:2|11:12:12\n");
    }
}

// [STAGE-SIGNALS-EXACT] Dependency consumers must use resolved call targets.
fn reported_dependencies(name: &str, source: &str) -> std::io::Result<std::process::Output> {
    let path = std::env::temp_dir().join(format!("osprey_deps_{}_{name}.osp", std::process::id()));
    std::fs::write(&path, source)?;
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_osprey"))
        .args([path.as_os_str(), "--deps".as_ref()])
        .output();
    std::fs::remove_file(path)?;
    output
}

/// Reports dependencies and returns the stdout of a successful run.
fn dependency_report(name: &str, source: &str) -> std::io::Result<String> {
    let output = reported_dependencies(name, source)?;
    assert!(output.status.success(), "{output:?}");
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Asserts every `expected` line appears verbatim in a dependency `report`.
fn assert_report_lines(report: &str, expected: &[&str]) {
    let present = |wanted: &&str| report.lines().any(|found| found == *wanted);
    assert!(expected.iter().all(present), "{report}");
}

#[test]
fn dependency_reports_follow_resolved_and_inferred_bindings() -> std::io::Result<()> {
    let resolved = "static effect Signal { read: fn() -> int }\nfn reading() = perform Signal.read()\nfn invoke(callback) = callback()\nfn widget() = invoke(reading)\nfn shadowed() = { let reading = || => 42\nreading() }\n";
    let generic = "static effect Signal<T> { read: fn(T) -> T }\nfn count() = perform Signal.read(42)\nfn name() = perform Signal.read(\"Ada\")\nfn status() = \"${count()}:${name()}\"\n";
    let report = dependency_report("resolved", resolved)?;
    assert_report_lines(&report, &["widget: Signal.read", "invoke: <unknown>"]);
    assert!(
        !report.lines().any(|line| line.starts_with("shadowed:")),
        "{report}"
    );
    assert_report_lines(
        &dependency_report("generic", generic)?,
        &[
            "count: Signal<int>.read",
            "name: Signal<string>.read",
            "status: Signal<int>.read, Signal<string>.read",
        ],
    );
    Ok(())
}

#[test]
fn invalid_dependency_source_never_publishes_a_partial_report() -> std::io::Result<()> {
    let output = reported_dependencies("invalid", "static effect Signal { read: fn() -> int }\nfn reading() = perform Signal.read()\nfn invalid() = missing()\n")?;
    assert!(!output.status.success());
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("missing"));
    Ok(())
}

#[test]
fn dependency_reports_assemble_modules_and_keep_source_names() -> std::io::Result<()> {
    let source = "namespace signals;\nmodule Views {\nexport static effect Signal { read: fn() -> int }\nexport fn count() = perform Signal.read()\n}\n";
    let report = dependency_report("module", source)?;
    assert_report_lines(
        &report,
        &["signals::Views::count: signals::Views::Signal.read"],
    );
    assert!(!report.contains("__osp_"), "{report}");
    Ok(())
}

#[test]
fn reactive_programs_match_both_flavors_and_release_all_arc_values() {
    for (name, sources, expected) in [
        (
            "reactive_views",
            [
                include_str!("../../../../osprey/tests/effects/staged/reactive_signals.test.osp"),
                include_str!("../../../../osprey/tests/effects/staged/reactive_signals.test.ospml"),
            ],
            include_str!(
                "../../../../osprey/tests/effects/staged/reactive_signals.test.osp.expectedoutput"
            ),
        ),
        (
            "reactive_fibers",
            [
                include_str!("../../../../osprey/tests/regressions/effects/staged_shared.test.osp"),
                include_str!(
                    "../../../../osprey/tests/regressions/effects/staged_shared.test.ospml"
                ),
            ],
            include_str!(
                "../../../../osprey/tests/regressions/effects/staged_shared.test.osp.expectedoutput"
            ),
        ),
    ] {
        for (extension, source) in ["osp", "ospml"].into_iter().zip(sources) {
            assert_flavored_output(name, extension, source, expected);
        }
    }
}

#[test]
fn reactive_static_reads_leave_no_runtime_effect_dispatch() {
    let source = "static effect Signal { read: fn() -> int }\nfn count() = perform Signal.read()\nlet first = { handle static Signal { read => 42 }\nsignalSnapshot(count) }\nlet next = { handle static Signal { read => 43 }\nsignalRefresh(first, signalDependencies(count), count) }\nprint(next.value)\n";
    let parsed = osprey_syntax::parse_program_with_flavor(source, osprey_syntax::Flavor::Default);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let compiled = osprey_codegen::compile_program(&parsed.program);
    assert!(compiled.is_ok(), "{compiled:?}");
    let Ok(ir) = compiled else { return };
    for symbol in [
        "__osprey_handler_push",
        "__osprey_handler_lookup",
        "__osprey_coro_suspend",
    ] {
        assert!(
            !ir.contains(symbol),
            "static reactive reads retained {symbol}"
        );
    }
}

#[test]
fn signal_iterators_consumed_inside_the_handler_remain_valid() {
    for (extension, source) in [
        ("osp", "static effect Signal { read: fn() -> int }\nlet cached = { handle static Signal { read => 1 }\nsignalSnapshot(|| => range(0, 2) |> map(fn(_i) => perform Signal.read()) |> toList()) }\nprint(\"${listGet(cached.value, 0) ?: 0}:${listGet(cached.value, 1) ?: 0}\")\n"),
        ("ospml", "static effect Signal\n    read : Unit => int\ncached =\n    handle static Signal\n        read => 1\n    signalSnapshot (\\() => range 0 2 |> map (\\_i => perform Signal.read ()) |> toList)\nprint \"${listGet cached.value 0 ?: 0}:${listGet cached.value 1 ?: 0}\"\n"),
    ] {
        assert_flavored_output("collected_signals", extension, source, "1:1\n");
    }
}
