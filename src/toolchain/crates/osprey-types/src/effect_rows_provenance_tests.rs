//! Callable provenance through records, lists, maps, success values, destructuring and branches.

use crate::effect_rows_tests::{accept_alarm, reject_alarm, unwrap_success};

/// Record fixtures of the field-provenance tests: a record with one callable
/// field, and the same record plus a label the update tests override.
const RUNNER: &str = "type Runner = { run: () -> int }\n";

const LABELLED_RUNNER: &str = "type Runner = { run: () -> int, label: string }\n\
    let runner = Runner {\n\
      run: fn() => perform Alarm.ring(),\n\
      label: \"old\"\n\
    }\n";

#[test]
fn effectful_closure_cannot_escape_through_a_record_field_alias() {
    reject_alarm(
        &format!("{RUNNER}let runner = Runner {{ run: fn() => perform Alarm.ring() }}\nlet delayed = runner.run\nlet answer = delayed()\n"),
        &["Alarm.ring"],
    );
}

#[test]
fn record_field_effect_propagates_through_a_function_parameter() {
    reject_alarm(
        &format!(
            "{RUNNER}fn execute(runner: Runner) = {{\nlet delayed = runner.run\ndelayed()\n}}\n\
             let runner = Runner {{ run: fn() => perform Alarm.ring() }}\n\
             let answer = execute(runner)\n"
        ),
        &["Alarm.ring"],
    );
}

#[test]
fn record_field_effect_propagates_from_a_function_return() {
    reject_alarm(
        &format!(
            "{RUNNER}fn makeRunner() = Runner {{ run: fn() => perform Alarm.ring() }}\n\
             let runner = makeRunner()\n\
             let delayed = runner.run\n\
             let answer = delayed()\n"
        ),
        &["Alarm.ring"],
    );
}

#[test]
fn record_update_preserves_callable_provenance_in_unchanged_fields() {
    reject_alarm(
        &format!(
            "{LABELLED_RUNNER}let updated = runner {{ label: \"new\" }}\n\
             let delayed = updated.run\n\
             let answer = delayed()\n"
        ),
        &["Alarm.ring"],
    );
}

#[test]
fn record_update_replaces_callable_provenance_in_overridden_fields() {
    accept_alarm(&format!(
        "{LABELLED_RUNNER}let updated = runner {{ run: fn() => 42 }}\n\
             let delayed = updated.run\n\
             let answer = delayed()\n"
    ));
}

#[test]
fn effectful_closure_cannot_escape_through_list_lookup() {
    reject_alarm(
        &format!(
            "let runners = [fn() => perform Alarm.ring()]\nlet delayed = {}\nlet answer = delayed()\n",
            unwrap_success("runners[0]")
        ),
        &["Alarm.ring"],
    );
}

#[test]
fn list_concatenation_preserves_callable_element_provenance() {
    reject_alarm(
        &format!(
            "let runners = [fn() => perform Alarm.ring()] + [fn() => 42]\nlet delayed = {}\nlet answer = delayed()\n",
            unwrap_success("listGet(runners, 0)")
        ),
        &["Alarm.ring"],
    );
}

#[test]
fn map_merge_preserves_callable_value_provenance() {
    reject_alarm(
        "let runners = {\"a\": fn() => perform Alarm.ring()} + {\"b\": fn() => 42}\n\
         let delayed = mapGet(runners, \"a\") ?: fn() => 0\n\
         let answer = delayed()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn persistent_list_transforms_preserve_callable_element_provenance() {
    reject_alarm(
        &format!(
            "let runners = listReverse(\nlistAppend([], fn() => perform Alarm.ring())\n)\nlet delayed = {}\nlet answer = delayed()\n",
            unwrap_success("listGet(runners, 0)")
        ),
        &["Alarm.ring"],
    );
}

#[test]
fn persistent_map_transforms_preserve_callable_value_provenance() {
    reject_alarm(
        &format!(
            "let runners = mapMerge(\nmapSet(Map(), \"alarm\", fn() => perform Alarm.ring()),\nMap()\n)\nlet retained = mapRemove(runners, \"other\")\nlet delayed = {}\nlet answer = delayed()\n",
            unwrap_success("mapGet(retained, \"alarm\")")
        ),
        &["Alarm.ring"],
    );
}

#[test]
fn list_element_effect_propagates_through_a_function_parameter() {
    reject_alarm(
        &format!(
            "fn execute(runners) = {{\nlet delayed = {}\ndelayed()\n}}\nlet answer = execute([fn() => perform Alarm.ring()])\n",
            unwrap_success("runners[0]")
        ),
        &["Alarm.ring"],
    );
}

#[test]
fn effectful_closure_cannot_escape_through_map_lookup() {
    reject_alarm(
        &format!(
            "let runners = {{ \"alarm\": (fn() => perform Alarm.ring()) }}\nlet delayed = {}\nlet answer = delayed()\n",
            unwrap_success("runners[\"alarm\"]")
        ),
        &["Alarm.ring"],
    );
}

#[test]
fn effectful_closure_cannot_escape_through_an_explicit_success_value() {
    reject_alarm(
        &format!(
            "let wrapped = Success {{ value: fn() => perform Alarm.ring() }}\nlet delayed = {}\nlet answer = delayed()\n",
            unwrap_success("wrapped")
        ),
        &["Alarm.ring"],
    );
}

#[test]
fn effectful_closure_cannot_escape_through_positional_union_destructuring() {
    reject_alarm(
        "type Wrapped = Empty | Has { run: () -> int }\n\
         let wrapped = Has { run: fn() => perform Alarm.ring() }\n\
         let delayed = match wrapped {\n\
           Has(value) => value\n\
           Empty => fn() => 0\n\
         }\n\
         let answer = delayed()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn branch_reassignment_cannot_hide_an_effectful_closure() {
    reject_alarm(
        "effect Switch { replace: fn() -> Unit }\n\
         mut delayed = fn() => 0\n\
         let _ = {\n\
             handle Switch {\n\
                 replace => { delayed = fn() => perform Alarm.ring() }\n\
             }\n\
             perform Switch.replace()\n\
         }\n\
         let answer = delayed()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn one_branch_with_an_effect_makes_the_whole_function_effectful() {
    reject_alarm(
        "fn choose(flag) = match flag {\n\
           true => perform Alarm.ring()\n\
           false => 0\n\
         }\n\
         let answer = choose(false)\n",
        &["Alarm.ring"],
    );
}
