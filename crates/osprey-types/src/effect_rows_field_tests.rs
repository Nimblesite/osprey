//! Effect rows written on function-typed fields ([EFFECTS-FIELD-ROWS]).
use crate::effect_rows_tests::{assert_accepted, assert_rejected_with};

/// A record crossing `map` into a separate `forEach` callback keeps its
/// element provenance now, so an effect-free stored function is provable with
/// no written row. A stored function that does perform is rejected for the
/// operation it actually runs. The written row remains the one trust the
/// analyser needs where provenance really is lost (see the corpus twin).
const CROSSING: &str = "type Field = { label: string, edit: fn(string) -> string ROW }\n\
    let fields = map(range(0, 2), fn(n) => Field { label: \"f\", edit: fn(t) => t + \"!\" })\n\
    forEach(fields, fn(field) => print(field.edit(field.label)))\n";

#[test]
fn an_effect_free_stored_function_through_map_and_foreach_is_proven() {
    assert_accepted(&CROSSING.replace(" ROW", ""));
}

#[test]
fn an_effectful_stored_function_through_map_and_foreach_is_rejected() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         fn ring() = perform Alarm.ring()\n\
         type Field = { label: string, edit: fn(string) -> int }\n\
         let fields = map(range(0, 2), fn(n) => Field { label: \"f\", edit: fn(t) => ring() })\n\
         forEach(fields, fn(field) => print(field.edit(field.label)))\n",
        &["unhandled effect operations at program entry: Alarm.ring"],
    );
}

#[test]
fn a_written_row_is_trusted_where_the_stored_value_lost_its_provenance() {
    assert_accepted(&CROSSING.replace("ROW", "![]"));
}

#[test]
fn a_call_through_a_trusted_field_performs_its_whole_row() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         type Control = { press: fn(string) -> int ![Alarm] }\n\
         let controls = map(range(0, 1), fn(n) => Control { press: fn(t) => 1 })\n\
         forEach(controls, fn(control) => print(toString(control.press(\"x\"))))\n",
        &["unhandled effect operations at program entry: Alarm.ring"],
    );
}

#[test]
fn a_construction_storing_a_function_outside_the_row_is_rejected() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         type Control = { press: fn(string) -> int ![] }\n\
         fn main() = {\n\
             handle Alarm { ring => 7 }\n\
             let control = Control { press: fn(t) => perform Alarm.ring() }\n\
             print(toString(control.press(\"x\")))\n\
         }\n",
        &["field `press` of `Control` declares effect row ![], but the stored function performs Alarm.ring"],
    );
    assert_rejected_with(
        "type Control = { show: fn(string) -> Unit ![] }\n\
         let control = Control { show: fn(t) => print(t) }\n\
         let shown = control.show(\"x\")\n",
        &["but the stored function uses print"],
    );
}

#[test]
fn a_row_needs_a_function_field_and_declared_effects() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         type Count = { total: int ![Alarm] }\n\
         let count = Count { total: 1 }\n",
        &["field `total` of `Count`: an effect row needs a function type"],
    );
    assert_rejected_with(
        "type Control = { press: fn(string) -> int ![Nope] }\n\
         let control = Control { press: fn(t) => 1 }\n",
        &["names an undeclared effect or a generic one without its type arguments"],
    );
    assert_rejected_with(
        "effect Stash<T> { take: fn() -> T }\n\
         type Control = { press: fn(string) -> int ![Stash] }\n\
         let control = Control { press: fn(t) => 1 }\n",
        &["a generic one without its type arguments"],
    );
}

#[test]
fn ml_pattern_bound_fields_trust_their_row_and_reject_open_rows() {
    let source = "type Binding =\n    Tap\n        label : string\n    Edit\n        apply : (string) -> string ![]\n\n\
        bindings = range 0 2 |> map (\\n => Edit(apply = \\t => t + \"?\"))\n\
        run binding = match binding\n    Tap label => label\n    Edit apply => apply \"go\"\n\
        forEach bindings (\\binding => print (run binding))\n";
    crate::testutil::accepts(osprey_syntax::Flavor::Ml, source);
    let open = osprey_syntax::parse_program_with_flavor(
        "type F =\n    edit : (string) -> string ![|e]\n",
        osprey_syntax::Flavor::Ml,
    );
    assert!(
        open.errors
            .iter()
            .any(|error| error.message.contains("a field's effect row is closed")),
        "{:?}",
        open.errors
    );
}
