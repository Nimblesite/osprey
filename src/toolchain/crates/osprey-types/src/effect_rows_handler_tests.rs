//! Handlers and declared rows: partial and nested handlers, generic instantiations, arms that forward to outer handlers, and recursive unions.

use crate::effect_rows_tests::{
    assert_accepted, assert_rejected_with, handled, reject_alarm, PAIR, STASH,
};

#[test]
fn partial_handler_leaves_unhandled_operation_in_row() {
    assert_rejected_with(
        &format!(
            "{PAIR}fn both() = [perform Pair.first(), perform Pair.second()]\n{}",
            handled("answer", "Pair", "first => 20", "both()")
        ),
        &["Pair.second"],
    );
}

#[test]
fn complementary_nested_partial_handlers_discharge_each_operation() {
    assert_accepted(&format!(
        "{PAIR}fn both() = [perform Pair.first(), perform Pair.second()]\n{}",
        handled(
            "answer",
            "Pair",
            "second => 22",
            "handle Pair {\nfirst => 20\n}\nboth()"
        )
    ));
}

#[test]
fn generic_effect_handler_must_match_the_performed_instantiation() {
    assert_rejected_with(
        &format!(
            "{STASH}fn storeNumber() = perform Stash.put(42)\n{}",
            handled(
                "done",
                "Stash",
                "put value => {}\ntake => \"cached text\"",
                "storeNumber()"
            )
        ),
        &["Stash<int>.put"],
    );
}

#[test]
fn generic_effect_handler_discharges_matching_instantiation() {
    assert_accepted(&format!(
        "{STASH}fn storeText() = perform Stash.put(\"fresh text\")\n{}",
        handled(
            "done",
            "Stash",
            "put value => {}\ntake => \"cached text\"",
            "storeText()"
        )
    ));
}

#[test]
fn declared_generic_effect_contract_must_match_the_required_instantiation() {
    assert_rejected_with(
        "effect Stash<T> { put: fn(T) -> Unit }\n\
         fn bad() -> Unit !Stash<string> = perform Stash.put(42)\n\
         let answer = 0\n",
        &["outside its declared row", "Stash<int>.put"],
    );
}

#[test]
fn handler_arm_performing_its_own_operation_forwards_to_an_outer_handler() {
    // An arm's own requirements belong to the region AROUND its handler, so
    // performing the operation it answers delegates that request outward —
    // the ordinary way a handler implements part of an interface in terms of
    // an enclosing one. [EFFECTS-STATIC-DISCHARGE]
    assert_accepted(&format!(
        "effect Loop {{ again: fn() -> int }}\n{}",
        handled(
            "answer",
            "Loop",
            "again => 7",
            "handle Loop {\nagain => perform Loop.again()\n}\nperform Loop.again()"
        )
    ));
}

#[test]
fn a_self_forwarding_arm_with_no_outer_handler_is_unhandled_at_entry() {
    // The same forwarding with nothing to forward TO is not a special
    // recursion rule: the requirement simply survives to program entry and is
    // reported there, naming the operation that escaped. [EFFECTS-STATIC-DISCHARGE]
    assert_rejected_with(
        &format!(
            "effect Loop {{ again: fn() -> int }}\n{}",
            handled(
                "answer",
                "Loop",
                "again => perform Loop.again()",
                "perform Loop.again()"
            )
        ),
        &["Loop.again"],
    );
}

#[test]
fn handler_arm_may_perform_a_different_generic_instance_for_an_outer_handler() {
    assert_accepted(&format!(
        "effect Relay<T> {{ fire: fn(T) -> int }}\n{}",
        handled(
            "answer",
            "Relay",
            "fire value => wrapAdd(value, 0)",
            "handle Relay {\nfire text => perform Relay.fire(42)\n}\nperform Relay.fire(\"start\")"
        )
    ));
}

#[test]
fn partial_handler_arm_may_perform_an_uncovered_operation_for_an_outer_handler() {
    assert_accepted(&format!(
        "{PAIR}{}",
        handled(
            "answer",
            "Pair",
            "second => 22",
            "handle Pair {\nfirst => perform Pair.second()\n}\nperform Pair.first()"
        )
    ));
}

#[test]
fn handler_arm_secondary_effect_requires_an_outer_handler() {
    assert_rejected_with(
        &format!(
            "effect Primary {{ ask: fn() -> int }}\neffect Audit {{ record: fn() -> int }}\n{}",
            handled(
                "answer",
                "Primary",
                "ask => perform Audit.record()",
                "perform Primary.ask()"
            )
        ),
        &["Audit.record"],
    );

    assert_accepted(&format!(
        "effect Primary {{ ask: fn() -> int }}\neffect Audit {{ record: fn() -> int }}\n{}",
        handled(
            "answer",
            "Audit",
            "record => 42",
            "handle Primary {\nask => perform Audit.record()\n}\nperform Primary.ask()"
        )
    ));
}

#[test]
fn declared_effect_row_is_a_contract_not_a_handler() {
    reject_alarm(
        "fn ring() -> int !Alarm = perform Alarm.ring()\n\
         let answer = ring()\n",
        &["Alarm.ring"],
    );
}

#[test]
fn declared_row_cannot_hide_another_inferred_effect() {
    assert_rejected_with(
        "effect Alarm { ring: fn() -> Unit }\n\
         effect Audit { record: fn() -> Unit }\n\
         fn bad() -> Unit !Alarm = perform Audit.record()\n",
        &["outside its declared row", "Audit.record"],
    );
}

#[test]
fn unknown_declared_effect_is_rejected() {
    assert_rejected_with(
        "fn bad() -> Unit !Imaginary = {}\n",
        &["declares unknown effects", "Imaginary"],
    );
}

/// The recursive-aggregate walk of #275: a closure stored in a union is
/// collected through `fold` over the children list, projected back out with
/// `listGet`, and invoked. [EFFECTS-PROVENANCE] requires the analysis of such
/// an aggregate to reach a finite fixed point, so the stored closure's effect
/// provenance survives the whole traversal instead of being widened away.
const RECURSIVE_UNION_WALK: &str = "effect Log { write: fn(string) -> string }\n\
    type Node = Stack { children: List<Node> } | Field { edit: fn(string) -> string }\n\
    type Binding = Edit { apply: fn(string) -> string }\n\
    fn collect(found, node) = match node {\n\
        Stack { children } => range(0, length(children)) |> fold(found, fn(acc, index) => match listGet(children, index) {\n\
            Success { value } => collect(acc, value)\n\
            Error(_) => acc\n\
        })\n\
        Field { edit } => listAppend(found, Edit { apply: edit })\n\
    }\n\
    fn dispatch(text) = match listGet(collect([], screen(\"\")), 0) {\n\
        Success { value } => match value { Edit { apply } => apply(text) }\n\
        Error(_) => \"rejected\"\n\
    }\n";

fn recursive_union_program(screen: &str, main: &str) -> String {
    format!("{RECURSIVE_UNION_WALK}{screen}\n{main}\n")
}

const PURE_SHALLOW_SCREEN: &str =
    "fn screen(query) = Stack { children: [Field { edit: fn(text) => \"query:\" + text }] }";

const EFFECTFUL_SHALLOW_SCREEN: &str =
    "fn screen(query) = Stack { children: [Field { edit: fn(text) => perform Log.write(text) }] }";

const PRINT_MAIN: &str = "fn main() = print(dispatch(\"codesign\"))";

#[test]
fn recursive_union_closure_keeps_its_provenance() {
    assert_accepted(&recursive_union_program(PURE_SHALLOW_SCREEN, PRINT_MAIN));
}

#[test]
fn effectful_closure_in_a_shallow_recursive_union_fails_closed() {
    assert_rejected_with(
        &recursive_union_program(EFFECTFUL_SHALLOW_SCREEN, PRINT_MAIN),
        &["Log.write"],
    );
}

#[test]
fn a_handler_discharges_a_stored_recursive_union_closure() {
    assert_accepted(&recursive_union_program(
        EFFECTFUL_SHALLOW_SCREEN,
        "fn main() = {\n\
    handle Log { write message => \"logged\" }\n\
    print(dispatch(\"codesign\"))\n\
}",
    ));
}

/// Forty `Stack` levels exceed the provenance depth budget, so the deep
/// aggregate is summarized fail-closed: an effectful closure nested that deep
/// and invoked at entry must never be treated as pure. Acceptance would only
/// be correct if the analysis could prove purity at that depth.
#[test]
fn effectful_closure_deep_in_a_recursive_union_fails_closed() {
    assert_rejected_with(
        &recursive_union_program(
            &format!(
                "fn screen(query) = {}Field {{ edit: fn(text) => perform Log.write(text) }} {}",
                "Stack { children: [".repeat(40),
                "] }".repeat(40)
            ),
            PRINT_MAIN,
        ),
        &["provenance cannot be proven"],
    );
}

/// Mutually recursive generics whose `fold` accumulator carries the callback's
/// return feed a function's return provenance back into itself. One fixed-point
/// pass is then a single unrolling of the recursion, not a complete answer;
/// `fold_value` stabilises that contribution so the iteration terminates
/// instead of oscillating between the unrolled levels (the #275 analysis
/// hung here at 100% CPU). [EFFECTS-PROVENANCE]
#[test]
fn mutually_recursive_fold_accumulators_reach_a_fixed_point() {
    assert_accepted(
        "type Tree = Leaf { item: int } | Node { kids: List<Tree> }\n\
         fn kid(out, kids, index) = listConcat(out, all(listGet(kids, index) ?: Node { kids: [] }))\n\
         fn all(tree) = match tree {\n\
             Leaf { item } => [item]\n\
             Node { kids } => range(0, length(kids)) |> fold([], fn(out, index) => kid(out, kids, index)) }\n\
         fn main() = print(toString(listLength(all(Node { kids: [Leaf { item: 1 }, Leaf { item: 2 }] }))))",
    );
}
