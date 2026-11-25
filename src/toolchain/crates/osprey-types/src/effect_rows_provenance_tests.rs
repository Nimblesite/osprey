//! Callable provenance through records, lists, maps, success values, destructuring and branches.

use crate::effect_rows_tests::{accept_alarm, diagnostics, unwrap_success, ALARM};

/// Record fixtures of the field-provenance tests: a record with one callable
/// field, and the same record plus a label the update tests override.
const RUNNER: &str = "type Runner = { run: () -> int }\n";

const LABELLED_RUNNER: &str = "type Runner = { run: () -> int, label: string }\n\
    let runner = Runner {\n\
      run: fn() => perform Alarm.ring(),\n\
      label: \"old\"\n\
    }\n";

/// A container holding the effectful closure, and the lookup that takes it
/// back out: the closure fetched by the lookup is then called at top level.
const LOOKUPS: &[(&str, &str, &str)] = &[
    (
        "a list lookup",
        "let runners = [fn() => perform Alarm.ring()]",
        "runners[0]",
    ),
    (
        "list concatenation",
        "let runners = [fn() => perform Alarm.ring()] + [fn() => 42]",
        "listGet(runners, 0)",
    ),
    (
        "persistent list transforms",
        "let runners = listReverse(\nlistAppend([], fn() => perform Alarm.ring())\n)",
        "listGet(runners, 0)",
    ),
    (
        "persistent map transforms",
        "let runners = mapMerge(\nmapSet(Map(), \"alarm\", fn() => perform Alarm.ring()),\nMap()\n)\nlet retained = mapRemove(runners, \"other\")",
        "mapGet(retained, \"alarm\")",
    ),
    (
        "a map lookup",
        "let runners = { \"alarm\": (fn() => perform Alarm.ring()) }",
        "runners[\"alarm\"]",
    ),
    (
        "an explicit success value",
        "let wrapped = Success { value: fn() => perform Alarm.ring() }",
        "wrapped",
    ),
];

/// Every program that hides `Alarm.ring` behind a value, each named by the
/// route the closure takes; the checker must still see the effect.
fn escapes() -> Vec<(&'static str, String)> {
    let mut cases: Vec<(&'static str, String)> = LOOKUPS
        .iter()
        .map(|(route, setup, lookup)| {
            let delayed = unwrap_success(lookup);
            (
                *route,
                format!("{setup}\nlet delayed = {delayed}\nlet answer = delayed()\n"),
            )
        })
        .collect();
    let runners = unwrap_success("runners[0]");
    cases.extend([
        ("a record field alias", format!("{RUNNER}let runner = Runner {{ run: fn() => perform Alarm.ring() }}\nlet delayed = runner.run\nlet answer = delayed()\n")),
        ("a record field read in a function parameter", format!("{RUNNER}fn execute(runner: Runner) = {{\nlet delayed = runner.run\ndelayed()\n}}\nlet runner = Runner {{ run: fn() => perform Alarm.ring() }}\nlet answer = execute(runner)\n")),
        ("a record returned from a function", format!("{RUNNER}fn makeRunner() = Runner {{ run: fn() => perform Alarm.ring() }}\nlet runner = makeRunner()\nlet delayed = runner.run\nlet answer = delayed()\n")),
        ("a field a record update leaves unchanged", format!("{LABELLED_RUNNER}let updated = runner {{ label: \"new\" }}\nlet delayed = updated.run\nlet answer = delayed()\n")),
        ("a map merge", "let runners = {\"a\": fn() => perform Alarm.ring()} + {\"b\": fn() => 42}\nlet delayed = mapGet(runners, \"a\") ?: fn() => 0\nlet answer = delayed()\n".to_string()),
        ("a list element read in a function parameter", format!("fn execute(runners) = {{\nlet delayed = {runners}\ndelayed()\n}}\nlet answer = execute([fn() => perform Alarm.ring()])\n")),
        ("positional union destructuring", "type Wrapped = Empty | Has { run: () -> int }\nlet wrapped = Has { run: fn() => perform Alarm.ring() }\nlet delayed = match wrapped {\nHas(value) => value\nEmpty => fn() => 0\n}\nlet answer = delayed()\n".to_string()),
        ("a reassignment in a handler branch", "effect Switch { replace: fn() -> Unit }\nmut delayed = fn() => 0\nlet _ = {\nhandle Switch {\nreplace => { delayed = fn() => perform Alarm.ring() }\n}\nperform Switch.replace()\n}\nlet answer = delayed()\n".to_string()),
        ("one effectful branch of a function", "fn choose(flag) = match flag {\ntrue => perform Alarm.ring()\nfalse => 0\n}\nlet answer = choose(false)\n".to_string()),
    ]);
    cases
}

#[test]
fn an_effectful_closure_cannot_escape_through_any_value_route() {
    for (route, body) in escapes() {
        let rendered: Vec<String> = diagnostics(&format!("{ALARM}{body}"))
            .into_iter()
            .map(|error| error.message)
            .collect();
        assert!(
            rendered
                .iter()
                .any(|message| message.contains("Alarm.ring")),
            "Alarm.ring escaped through {route}; diagnostics: {rendered:?}"
        );
    }
}

#[test]
fn record_update_replaces_callable_provenance_in_overridden_fields() {
    accept_alarm(&format!(
        "{LABELLED_RUNNER}let updated = runner {{ run: fn() => 42 }}\n\
             let delayed = updated.run\n\
             let answer = delayed()\n"
    ));
}
