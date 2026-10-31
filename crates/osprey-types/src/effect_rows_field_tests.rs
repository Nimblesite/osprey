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

const EMPTY_GATE: &str = "type Gate<in T> = Gate { admit: fn(T) -> bool } | Open\n\
    fn runGate(gate) = match gate { Gate(callback) => callback(7) Open => false }\n";
const RESULT_GATE: &str = "fn runResult(result) = match result { Success { value } => runGate(value) Error { message } => message != \"skipped\" }\n";
const FACTORY_GATE: &str = "type Factory = { make: fn() -> Gate<int> }\n\
    fn openGate() = Open\n\
    fn chooseFactory(flag, holder) = match flag { true => openGate false => holder.make }\n";

#[test]
fn a_known_nullary_variant_does_not_invoke_an_absent_callback() {
    assert_accepted(&format!("{EMPTY_GATE}print(runGate(Open))\n"));
    assert_accepted(&format!(
        "{EMPTY_GATE}\
        type Pair = {{ gate: Gate<int> }}\nfn box(gate) = Pair {{ gate: gate }}\n\
        print(runGate(box(Open).gate))\n"
    ));
}

#[test]
fn known_handler_results_preserve_the_absent_callback_fact() {
    assert_accepted(&format!(
        "{EMPTY_GATE}\
        effect Read {{ get: fn() -> Gate<int> }}\n\
        fn readGate() = perform Read.get()\n\
        let answer = {{ handle Read {{ get => Open }}\nrunGate(readGate()) }}\n"
    ));
    assert_rejected_with(&format!("{EMPTY_GATE}\
        effect Alarm {{ ring: fn() -> bool }}\n\
        effect Read {{ get: fn() -> Gate<int> }}\n\
        fn readGate() = perform Read.get()\n\
        let answer = {{ handle Read {{ get => Gate {{ admit: fn(value) => perform Alarm.ring() }} }}\n\
            runGate(readGate()) }}\n"),
        &["unhandled effect operations at program entry: Alarm.ring"]);
}

#[test]
fn known_handler_results_survive_calls_through_record_fields() {
    assert_accepted(&format!(
        "{EMPTY_GATE}\
        effect Read {{ get: fn() -> Gate<int> }}\n\
        type Reader = {{ get: fn() -> Gate<int> }}\n\
        fn readGate() = perform Read.get()\n\
        let reader = Reader {{ get: readGate }}\n\
        let answer = {{ handle Read {{ get => Open }}\nrunGate(reader.get()) }}\n"
    ));
}

#[test]
fn a_known_error_does_not_invoke_a_success_payload_callback() {
    assert_accepted(&format!(
        "{EMPTY_GATE}{RESULT_GATE}\
        print(runResult(Error {{ message: \"skipped\" }}))\n"
    ));
}

#[test]
fn effects_in_a_factory_returning_error_are_still_required() {
    assert_rejected_with(
        &format!(
            "{EMPTY_GATE}{RESULT_GATE}\
        effect Alarm {{ ring: fn() -> string }}\n\
        fn failed() = Error {{ message: perform Alarm.ring() }}\n\
        print(runResult(failed()))\n"
        ),
        &["unhandled effect operations at program entry: Alarm.ring"],
    );
}

#[test]
fn joining_error_and_live_callback_keeps_the_callback_effect() {
    assert_rejected_with(&format!("{EMPTY_GATE}{RESULT_GATE}\
        effect Alarm {{ ring: fn() -> bool }}\n\
        fn outcome(flag) = if flag {{ Error {{ message: \"skipped\" }} }} else {{ Success {{ value: Gate {{ admit: fn(x) => perform Alarm.ring() }} }} }}\n\
        print(runResult(outcome(false)))\n"),
        &["unhandled effect operations at program entry: Alarm.ring"]);
}

#[test]
fn joining_an_empty_variant_with_an_opaque_variant_is_not_proven_empty() {
    for choice in [
        "if flag { Open } else { foreignGate() }",
        "match flag { true => Open false => foreignGate() }",
    ] {
        assert_rejected_with(
            &format!(
                "{EMPTY_GATE}\
        extern fn foreignGate() -> Gate<int>\n\
        fn choose(flag) = {choice}\n\
        print(runGate(choose(false)))\n"
            ),
            &["dynamic callable whose effect provenance cannot be proven"],
        );
    }
}

#[test]
fn an_unmodelled_field_cannot_prove_a_joined_callback_absent() {
    for choice in [
        "match flag { true => Open false => makeBox().gate }",
        "match flag { true => makeBox().gate false => Open }",
        "if flag { Open } else { makeBox().gate }",
    ] {
        assert_rejected_with(
            &format!(
                "{EMPTY_GATE}\
        type Pair = {{ gate: Gate<int> }}\nextern fn makeBox() -> Pair\n\
        fn choose(flag) = {choice}\nprint(runGate(choose(false)))\n"
            ),
            &["dynamic callable whose effect provenance cannot be proven"],
        );
    }
}

#[test]
fn an_opaque_record_join_cannot_retain_a_nested_absence_fact() {
    for known in ["Open", "Gate { admit: fn(value) => true }"] {
        assert_rejected_with(
            &format!(
                "{EMPTY_GATE}\
        type Pair = {{ gate: Gate<int> }}\nextern fn makeBox() -> Pair\n\
        fn choose(flag) = match flag {{ true => Pair {{ gate: {known} }} false => makeBox() }}\n\
        print(runGate(choose(false).gate))\n"
            ),
            &["dynamic callable whose effect provenance cannot be proven"],
        );
    }
}

#[test]
fn joined_returned_callbacks_keep_their_original_parameter_frame() {
    assert_rejected_with("type Gate = Gate { next: fn(int, Gate) -> Gate } | Open\n\
        type Holder = { next: fn(int, Gate) -> Gate }\n\
        effect Alarm { ring: fn() -> bool }\n\
        fn openGate(x, y) = Open\n\
        fn choose(flag, holder, pad) = match flag { true => openGate false => holder.next }\n\
        let callback = choose(false, Holder { next: fn(a, b) => { let _ = perform Alarm.ring()\nOpen } }, Open)\n\
        let gate = callback(0, Open)\n",
        &["unhandled effect operations at program entry: Alarm.ring"]);
}

#[test]
fn joined_factories_preserve_the_actual_returned_callback() {
    assert_accepted(&format!(
        "{EMPTY_GATE}{FACTORY_GATE}\
        let factory = chooseFactory(false, Factory {{ make: fn() => Open }})\n\
        print(runGate(factory()))\n"
    ));
    assert_rejected_with(&format!("{EMPTY_GATE}{FACTORY_GATE}\
        effect Alarm {{ ring: fn() -> bool }}\n\
        let factory = chooseFactory(false, Factory {{ make: fn() => Gate {{ admit: fn(value) => perform Alarm.ring() }} }})\n\
        print(runGate(factory()))\n"),
        &["unhandled effect operations at program entry: Alarm.ring"]);
}

#[test]
fn an_empty_factory_branch_cannot_erase_the_other_returned_callback() {
    assert_rejected_with(
        &format!(
            "{EMPTY_GATE}{FACTORY_GATE}\
        effect Alarm {{ ring: fn() -> bool }}\n\
        fn alarmGate() = Gate {{ admit: fn(value) => perform Alarm.ring() }}\n\
        fn chooseAlarm(flag, holder) = match flag {{ true => alarmGate false => holder.make }}\n\
        let factory = chooseAlarm(true, Factory {{ make: fn() => Open }})\n\
        print(runGate(factory()))\n"
        ),
        &["unhandled effect operations at program entry: Alarm.ring"],
    );
}

#[test]
fn an_unmodelled_list_element_cannot_prove_all_callbacks_absent() {
    assert_rejected_with(
        &format!(
            "{EMPTY_GATE}\
        type Pair = {{ gate: Gate<int> }}\nextern fn makeBox() -> Pair\n\
        let answers = mapList([Open, makeBox().gate], fn(gate) => runGate(gate))\n"
        ),
        &["dynamic callable whose effect provenance cannot be proven"],
    );
}

#[test]
fn unmodelled_map_and_handler_values_do_not_prove_callbacks_absent() {
    for source in [
        "let gate = match mapGet({ \"open\": Open, \"opaque\": makeBox().gate }, \"opaque\") { Success { value } => value Error { message } => Open }\n",
        "effect Choice { control pick: fn() -> Gate<int> }\nlet gate = { handle Choice { pick => Open }\nmakeBox().gate }\n",
    ] {
        assert_rejected_with(&format!("{EMPTY_GATE}\
            type Pair = {{ gate: Gate<int> }}\nextern fn makeBox() -> Pair\n\
            {source}print(runGate(gate))\n"),
            &["dynamic callable whose effect provenance cannot be proven"]);
    }
}

#[test]
fn opaque_collection_concatenation_keeps_callback_provenance_unknown() {
    for collection in [
        "[Open] + foreignGates()",
        "listConcat([Open], foreignGates())",
        "listConcat(foreignGates(), [Open])",
    ] {
        assert_rejected_with(
            &format!(
                "{EMPTY_GATE}\
        extern fn foreignGates() -> List<Gate<int>>\n\
        let answers = mapList({collection}, fn(gate) => runGate(gate))\n"
            ),
            &["dynamic callable whose effect provenance cannot be proven"],
        );
    }
}

#[test]
fn empty_collection_operands_preserve_known_absent_callbacks() {
    for collection in [
        "[Open] + []",
        "listConcat([Open], [])",
        "listConcat([], [Open])",
        "[Open] + ([] + [])",
    ] {
        assert_accepted(&format!(
            "{EMPTY_GATE}\
            let answers = mapList({collection}, fn(gate) => runGate(gate))\n"
        ));
    }
}

#[test]
fn handler_assignments_do_not_keep_stale_callback_absence() {
    for (value, rejected) in [("Open", false), ("makeBox().gate", true)] {
        let source = format!(
            "{EMPTY_GATE}\
            type Pair = {{ gate: Gate<int> }}\nextern fn makeBox() -> Pair\n\
            effect Replace {{ replace: fn() -> Unit }}\n\
            let answer = {{ mut gate = Open\n\
                handle Replace {{ replace => {{ gate = {value} }} }}\n\
                perform Replace.replace()\nrunGate(gate) }}\n"
        );
        if rejected {
            assert_rejected_with(
                &source,
                &["dynamic callable whose effect provenance cannot be proven"],
            );
        } else {
            assert_accepted(&source);
        }
    }
}

#[test]
fn arm_local_mutations_do_not_escape_their_scope() {
    assert_accepted(&format!(
        "{EMPTY_GATE}\
        type Pair = {{ gate: Gate<int> }}\nextern fn makeBox() -> Pair\n\
        effect Replace {{ replace: fn() -> Unit }}\n\
        let answer = {{ mut gate = Open\n\
            handle Replace {{ replace => {{ mut gate = Open\ngate = makeBox().gate }} }}\n\
            perform Replace.replace()\nrunGate(gate) }}\n"
    ));
}

#[test]
fn appending_an_unmodelled_element_keeps_its_callback_unknown() {
    for operation in ["listAppend", "listPrepend"] {
        assert_rejected_with(&format!("{EMPTY_GATE}\
            type Pair = {{ gate: Gate<int> }}\nextern fn makeBox() -> Pair\n\
            let answers = mapList({operation}([Open], makeBox().gate), fn(gate) => runGate(gate))\n"),
            &["dynamic callable whose effect provenance cannot be proven"]);
    }
}

#[test]
fn joining_function_returns_does_not_drop_an_opaque_callback_value() {
    assert_rejected_with(
        &format!(
            "{EMPTY_GATE}\
        type Pair = {{ gate: Gate<int> }}\nextern fn makeBox() -> Pair\n\
        fn choose(flag) = match flag {{ true => fn() => Open false => fn() => makeBox().gate }}\n\
        print(runGate(choose(false)()))\n"
        ),
        &["dynamic callable whose effect provenance cannot be proven"],
    );
}

#[test]
fn opaque_perform_arguments_do_not_inherit_an_empty_callback_fact() {
    assert_rejected_with(
        &format!(
            "{EMPTY_GATE}\
        type Pair = {{ gate: Gate<int> }}\nextern fn makeBox() -> Pair\n\
        effect Relay {{ gate: fn(Gate<int>) -> bool }}\n\
        let answer = {{ handle Relay {{ gate value => runGate(value) }}\n\
            let _ = perform Relay.gate(Open)\nperform Relay.gate(makeBox().gate) }}\n"
        ),
        &["dynamic callable whose effect provenance cannot be proven"],
    );
}

#[test]
fn live_and_opaque_callback_variants_keep_their_effect_requirements() {
    assert_rejected_with(
        &format!(
            "{EMPTY_GATE}\
        effect Alarm {{ ring: fn() -> bool }}\n\
        print(runGate(Gate {{ admit: fn(x) => perform Alarm.ring() }}))\n"
        ),
        &["unhandled effect operations at program entry: Alarm.ring"],
    );
    assert_rejected_with(
        &format!(
            "{EMPTY_GATE}\
        extern fn foreignGate() -> Gate<int>\nprint(runGate(foreignGate()))\n"
        ),
        &["dynamic callable whose effect provenance cannot be proven"],
    );
}

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
    // The row is the field's contract, so it is performed in full even where
    // the stored function's own provenance is still known and empty.
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         type Control = { press: fn(string) -> int ![Alarm] }\n\
         let control = Control { press: fn(t) => 1 }\n\
         print(toString(control.press(\"x\")))\n",
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
fn updates_cannot_store_a_function_outside_the_fields_written_row() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         type Control = { press: fn(string) -> int ![] }\n\
         let control = Control { press: fn(text) => 1 }\n\
         let changed = control { press: fn(text) => perform Alarm.ring() }\n",
        &["field `press` of `Control` declares effect row ![]"],
    );
    // An update is held to the same row a construction is: the row admits a
    // stored function that stays within it, and a field declaring no row is
    // bounded only by that function's own effects.
    assert_accepted(
        "effect Alarm { ring: fn() -> int }\n\
         type Control = { press: fn(string) -> int ![], show: fn() -> int }\n\
         let control = Control { press: fn(text) => 1, show: fn() => 1 }\n\
         let changed = control { press: fn(text) => 1, show: fn() => perform Alarm.ring() }\n",
    );
    // A declared parameter type names the same declaration, so an update on a
    // parameter is held to the row too.
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         type Control = { press: fn(string) -> int ![] }\n\
         fn change(control: Control) = control { press: fn(text) => perform Alarm.ring() }\n",
        &["field `press` of `Control` declares effect row ![]"],
    );
    // A record that merely shares the field name owns no row, so its update is
    // judged by the stored function itself rather than by `Control`'s row.
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         type Control = { press: fn(string) -> int ![] }\n\
         let other = { press: fn(text) => 1 }\n\
         let changed = other { press: fn(text) => perform Alarm.ring() }\n\
         let answer = changed.press(\"x\")\n",
        &["unhandled effect operations at program entry: Alarm.ring"],
    );
}

#[test]
fn unrelated_anonymous_fields_do_not_inherit_a_named_types_row() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         type Control = { press: fn(string) -> int ![] }\n\
         let other = { press: fn(text) => perform Alarm.ring() }\n\
         let answer = other.press(\"x\")\n",
        &["unhandled effect operations at program entry: Alarm.ring"],
    );
    assert_accepted(
        "effect Alarm { ring: fn() -> int }\n\
         type Control = { press: fn(string) -> int ![] }\n\
         let other = { press: fn(text) => perform Alarm.ring() }\n\
         let answer = { handle Alarm { ring => 1 }\nother.press(\"x\") }\n",
    );
    // Extracting the field instead of calling through it reaches the same
    // value, so the row still cannot replace the effects it proves.
    assert_rejected_with(
        "effect Alarm { ring: fn() -> int }\n\
         type Control = { press: fn(string) -> int ![] }\n\
         let other = { press: fn(text) => perform Alarm.ring() }\n\
         let callback = other.press\n\
         let answer = callback(\"x\")\n",
        &["unhandled effect operations at program entry: Alarm.ring"],
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
