//! TypeScript effect declarations, performs and handlers lower to the shared
//! effects core. Implements [FLAVOR-TS-EFFECTS].
mod common;

use osprey_syntax::{parse_program_with_flavor, Flavor};

/// Asserts the TypeScript `source` has the canonical AST of the Default program.
fn same(default: &str, source: &str) {
    let expected = common::canonical(default, Flavor::Default);
    let actual = common::canonical(source, Flavor::TypeScript);
    assert!(
        actual == expected,
        "{source}\n--- typescript\n{actual}\n--- default\n{expected}"
    );
}

/// Asserts `source` is rejected by a diagnostic containing `needle` at the
/// one-based `line` and zero-based `column`.
fn rejected_at(source: &str, needle: &str, line: u32, column: u32) {
    let parsed = parse_program_with_flavor(source, Flavor::TypeScript);
    let found = parsed
        .errors
        .iter()
        .find(|error| error.message.contains(needle))
        .map(|error| (error.position.line, error.position.column));
    assert_eq!(found, Some((line, column)), "{source}\n{:?}", parsed.errors);
}

#[test]
fn typescript_effect_interfaces_equal_default_declarations() {
    same(
        "effect Log {\n    write: fn(string) -> Unit\n}\neffect Choice {\n    control pick: fn(int, int) -> int\n    control many flip: fn() -> bool\n    control abort stop: fn(string) -> int\n    control once take: fn() -> int\n    replayable count: fn() -> int\n}\nstatic effect Clock {\n    now: fn() -> int\n}\neffect Store<T> {\n    get: fn() -> T\n    put: fn(T) -> Unit\n}\n",
        "interface Log extends Effect {\n  write(message: string): void;\n}\ninterface Choice extends Effect {\n  pick(a: int, b: int): Control<int>;\n  flip(): Many<boolean>;\n  stop(reason: string): Abort<int>;\n  take(): Once<int>;\n  count(): Replayable<int>;\n}\ninterface Clock extends StaticEffect {\n  now(): int;\n}\ninterface Store<T> extends Effect {\n  get(): T;\n  put(value: T): void;\n}\n",
    );
}

#[test]
fn typescript_perform_handle_and_handler_equal_default_ast() {
    for (default, source) in [
        (
            "fn work() -> int ![Log, Choice] = {\n    perform Log.write(\"loaded\")\n    perform Choice.pick(1, 2)\n}\n",
            "function work(): Requires<int, [Log, Choice]> {\n  perform<Log>().write(\"loaded\");\n  return perform<Choice>().pick(1, 2);\n}\n",
        ),
        (
            "let console = handler Log { write message => print(message) }\nlet quiet = handler static Log { write message => {} }\n",
            "const console = handler<Log>({ write: (message) => print(message) });\nconst quiet = staticHandler<Log>({ write: (message) => {} });\n",
        ),
        (
            "fn run() = {\n    handle Choice {\n        pick a b => resume(a)\n        return n => n + 1\n        finally => print(\"done\")\n    }\n    work()\n}\n",
            "function run() {\n  return handle<Choice>({ pick: (a, b) => resume(a), return: (n) => n + 1, finally: () => print(\"done\") }, () => work());\n}\n",
        ),
        (
            "fn fixed() = {\n    handle static Clock { now => 42 }\n    let t = perform Clock.now()\n    t + 1\n}\n",
            "function fixed() {\n  return handleStatic<Clock>({ now: () => 42 }, () => {\n    const t = perform<Clock>().now();\n    return t + 1;\n  });\n}\n",
        ),
        (
            "fn stored() = {\n    handle Store<int> { get => 1\n put value => {} }\n    perform Store<int>.get()\n}\n",
            "function stored() {\n  return handle<Store<int>>({ get: () => 1, put: (value) => {} }, () => perform<Store<int>>().get());\n}\n",
        ),
    ] {
        same(default, source);
    }
}

/// [EFFECTS-HANDLER-STATE]: a handler arm's assignment to an enclosing `let`
/// is handler-owned state, so that `let` is Osprey `mut` and the arm writes it.
#[test]
fn typescript_handler_arms_write_enclosing_state() {
    same(
        "fn count() = {\n    mut total = 10\n    let step = 1\n    handle Counter {\n        tick amount => { total = total + amount }\n    }\n    perform Counter.tick(step)\n    total\n}\n",
        "function count() {\n  let total = 10;\n  const step = 1;\n  return handle<Counter>({ tick: (amount) => { total = total + amount; } }, () => {\n    perform<Counter>().tick(step);\n    return total;\n  });\n}\n",
    );
}

#[test]
fn typescript_effects_reject_malformed_forms_at_their_positions() {
    for (source, needle, line, column) in [
        (
            "interface Log extends Effect {\n  level: int;\n}\n",
            "an effect operation is a method signature",
            2,
            2,
        ),
        (
            "interface Log extends Effect {\n  write(message): void;\n}\n",
            "a signature parameter needs a type",
            2,
            8,
        ),
        (
            "interface Log extends Effect, Other {\n  write(m: string): void;\n}\n",
            "an effect extends `Effect` or `StaticEffect` alone",
            1,
            0,
        ),
        (
            "const x = perform<Log>().write;\n",
            "`perform<E>()` must be called as an operation",
            1,
            10,
        ),
        (
            "const x = perform().write(1);\n",
            "`perform` names its effect: `perform<E>().op(…)`",
            1,
            10,
        ),
        (
            "const h = handler<Log>(arms);\n",
            "a handler takes an object literal of arms",
            1,
            23,
        ),
        (
            "const h = handler<Log>({ write: 1 });\n",
            "a handler arm is an arrow function",
            1,
            25,
        ),
        (
            "const h = handle<Log>({ write: (m) => m });\n",
            "`handle<E>(arms, () => body)`",
            1,
            10,
        ),
        (
            "const h = handler<Log>({ write: (m: string) => m });\n",
            "a handler arm's parameters take their types from the operation",
            1,
            32,
        ),
        (
            "let t = 0;\nconst h = handler<Log>({ write: (m) => { t = 1; } });\nt = 2;\n",
            "after a function captured",
            3,
            0,
        ),
    ] {
        rejected_at(source, needle, line, column);
    }
}
