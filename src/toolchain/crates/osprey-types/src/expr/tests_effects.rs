//! Expression inference tests: effects, handlers and arms.

use crate::testutil::{bad, bad_with, ok};

#[test]
fn yield_forwards_its_value_type_and_send_checks_the_channel_element() {
    // Implements [CONCURRENCY-YIELD] and [CONCURRENCY-CHANNEL].
    ok("fn hand_off(value: int) -> int = yield value\n");
    bad_with(
        "fn wrong(ch: Channel<int>) -> Unit = send(ch, \"wrong\")\n",
        "cannot unify int with string",
    );
    let result_channel = bad(
        "fn sendFailed(ch: Channel<Result<int, string>>, value: Result<int, string>) -> Unit = send(ch, value)\n",
    );
    assert!(result_channel
        .iter()
        .any(|e| e.message.contains("Result-valued channels")));
    // The receiving end is gated for the same reason: handing the Result
    // wrapper back out of `recv` would erase it silently.
    let result_recv =
        bad("fn recvFailed(ch: Channel<Result<int, string>>) -> Result<int, string> = recv(ch)\n");
    assert!(result_recv
        .iter()
        .any(|e| e.message.contains("Result-valued channels")));
}

#[test]
fn a_let_bound_channel_or_fiber_is_monomorphic_in_its_element() {
    // The ML value restriction, narrowed to the two stateful handles.
    // A `let` generalizes, so quantifying the element of a handle gave
    // every USE its own copy and nothing tied the two ends together: this
    // program COMPILED and printed a raw pointer as an integer.
    // Implements [CONCURRENCY-CHANNEL] and [CONCURRENCY-SPAWN-AWAIT].
    let sent_string_read_as_int = bad("fn main() -> Unit = {\n\
          let ch = Channel(1)\n\
          send(ch, \"text\")\n\
          print(\"${recv(ch) + 1}\")\n\
        }\n");
    assert!(
        sent_string_read_as_int
            .iter()
            .any(|e| e.message.contains("cannot unify string with int")),
        "a channel sent a string must not receive an int: {sent_string_read_as_int:?}"
    );
    // Two sends of different types into ONE channel are the same hole seen
    // from the other side.
    let two_element_types = bad("fn main() -> Unit = {\n\
          let ch = Channel(2)\n\
          send(ch, 1)\n\
          send(ch, \"two\")\n\
        }\n");
    assert!(
        !two_element_types.is_empty(),
        "one channel may carry exactly one element type"
    );
    // A fiber handle is fixed by the thunk that produced it, so awaiting it
    // at another type is the same error.
    let fiber_at_two_types = bad("fn text() -> string = \"t\"\n\
        fn main() -> Unit = {\n\
          let f = spawn text()\n\
          print(\"${await(f) + 1}\")\n\
        }\n");
    assert!(
        !fiber_at_two_types.is_empty(),
        "an awaited fiber keeps its thunk's type"
    );
    // ...and the honest programs still type: one element type per handle,
    // used consistently at both ends, including a nested collection.
    ok("fn main() -> Unit = {\n\
          let ch = Channel(2)\n\
          send(ch, [[1, 2], [3]])\n\
          let back = recv(ch)\n\
          print(\"${listLength(back)}\")\n\
        }\n");
    // Pinning is confined to handle VALUES. An immutable constructor still
    // generalizes, and so does a FUNCTION that merely mentions a handle:
    // every call instantiates its own, so a factory or a generic drain
    // helper is not collateral damage.
    ok("fn main() -> Unit = {\n\
          let identity = |x| => x\n\
          print(\"${identity(1)} ${identity(\"two\")}\")\n\
        }\n");
    ok("fn main() -> Unit = {\n\
          let make = |n| => Channel(n)\n\
          let ints = make(1)\n\
          let words = make(1)\n\
          send(ints, 7)\n\
          send(words, \"seven\")\n\
          print(\"${recv(ints)} ${recv(words)}\")\n\
        }\n");
    ok("fn firstOf(ch) = recv(ch)\n\
        fn main() -> Unit = {\n\
          let ints = Channel(1)\n\
          let words = Channel(1)\n\
          send(ints, 7)\n\
          send(words, \"seven\")\n\
          print(\"${firstOf(ints)} ${firstOf(words)}\")\n\
        }\n");
}

#[test]
fn a_unit_operation_arm_may_not_swallow_a_result() {
    // A `Unit` operation discards its arm's value, so a `Result` produced
    // there would lose its failure with nothing left to observe it.
    // Implements [EFFECTS-RESUME].
    bad_with(
        "effect Sink { drop: fn(int) -> Unit }\n\
                    fn risky(n) = checkedAdd(n, 1)\n\
                    fn go() -> int = {\n\
                        handle Sink {\n\
                            drop v => risky(v)\n\
                        }\n\
                        0\n\
                    }\n",
        "Unit effect operation arm",
    );
    ok("effect Sink { drop: fn(int) -> Unit }\n\
        fn risky(n) = checkedAdd(n, 1)\n\
        fn go() -> int = {\n\
            handle Sink {\n\
                drop v => { let handled = risky(v) ?: 0 }\n\
            }\n\
            0\n\
        }\n");
}

#[test]
fn handling_an_undeclared_effect_names_it_and_keeps_checking_the_arms() {
    // The handler's own operations are unknown, so the arms cannot be
    // typed against a signature — but checking must continue rather than
    // abandon the body, or one typo would hide every later diagnostic.
    bad_with(
        "fn go() -> int = {\n\
                          handle Nowhere {\n\
                              chime => 0\n\
                          }\n\
                          1\n\
                      }\n",
        "unknown effect `Nowhere`",
    );
}

#[test]
fn perform_of_an_undeclared_effect_names_the_effect_it_could_not_find() {
    bad_with(
        "fn ring() -> int = perform Nowhere.chime()\n",
        "unknown effect `Nowhere`",
    );
}

#[test]
fn a_generic_constructor_checks_its_type_argument_arity() {
    // Implements [GENERICS-CTOR-ARITY]. Writing the arguments out is a
    // contract with the declaration, so a mismatch is an error rather than
    // a silently ignored annotation.
    ok("type Box<T> = { v: T }\n\
        let good = Box<int> { v: 1 }\n");
    bad_with(
        "type Box<T> = { v: T }\n\
                    let wrong = Box<int, string> { v: 1 }\n",
        "takes 1 type argument(s), got 2",
    );
}

#[test]
fn a_qualified_path_callee_is_looked_up_under_its_whole_name() {
    // `infer_callee`'s path arm looks up `a::b` as ONE name rather than
    // resolving `a` and projecting. A single file has no assembled project
    // graph to bind the qualified name, so the diagnostic must name the
    // full path — naming only `twice` would send the reader to the wrong
    // declaration.
    bad_with(
        "namespace tools;\n\
                    fn twice(n) = n * 2\n\
                    let doubled = tools::twice(21)\n",
        "unknown identifier `tools::twice`",
    );
}

#[test]
fn handler_expressions_type_their_arms() {
    ok("effect Logger { log: fn(string) -> Unit }\n\
        fn run() -> int = {\n\
            handle Logger {\n\
                log msg => 0\n\
            }\n\
            42\n\
        }\n");
    // Resume feeds the operation-result slot and the handled body feeds the
    // handler-answer slot. Guards the [EFFECTS-RESUME] fix.
    ok("effect Guard { control check: fn(int) -> int }\n\
        fn guarded() -> int = {\n\
            handle Guard {\n\
                check v => match v < 100 {\n\
                true => resume(v)\n\
                false => 0\n\
                }\n\
            }\n\
            let a = perform Guard.check(5)\n\
            a\n\
        }\n");
}

/// A region of CONTROL operations: `resume` has already supplied `b`'s
/// declared result and the arm runs on afterwards — its own value is the
/// whole `handle` result and is checked against the ANSWER. The same holds
/// for a branch of such an arm that returns WITHOUT resuming, which
/// abandons the continuation.
///
/// The mode is the OPERATION'S, declared: a sibling arm cannot change it,
/// and neither can what this arm's body happens to contain, which is what
/// [`a_value_arm_is_unaffected_by_a_sibling_control_arm`] pins.
/// [EFFECTS-HANDLER-ARMS]
fn mixed_region(
    op_ret: &str,
    supplied: &str,
    body_ret: &str,
    body_tail: &str,
    arm: &str,
) -> String {
    format!(
        "effect Mixed {{ control a: fn(int) -> int\n\
          control b: fn() -> {op_ret} }}\n\
         fn body() -> {body_ret} !Mixed = {{\n\
          let ignored = perform Mixed.b()\n\
          {body_tail}\n\
          }}\n\
         let out = {{\n\
             handle Mixed {{\n\
                 a x => resume(x)\n\
                 b => {{\n\
                 let settled = resume({supplied})\n\
                 {arm}\n\
                 }}\n\
             }}\n\
             body()\n\
         }}\n"
    )
}

#[test]
fn a_resuming_arm_must_be_able_to_be_the_answer() {
    // Both directions, and through a Result answer. Checking only one
    // direction is not a partial gate: the unchecked one reached codegen's
    // `coerce_to`, which boxed a heap address as a successful `int`.
    for (op_ret, supplied, body_ret, body_tail, arm) in [
        ("int", "0", "string", "\"done\"", "7"),
        ("string", "\"s\"", "int", "42", "\"dynamic\""),
        (
            "string",
            "\"s\"",
            "Result<int, string>",
            "Success { value: 42 }",
            "\"dynamic\"",
        ),
    ] {
        let errs = bad(&mixed_region(op_ret, supplied, body_ret, body_tail, arm));
        assert!(
            errs.iter().any(|e| e
                .message
                .contains("becomes the whole `handle` expression's result")),
            "arm `{arm}` must not be allowed to answer `{body_ret}`; got {errs:?}"
        );
    }
}

#[test]
fn an_abandoning_branch_may_disagree_with_its_own_operation_result() {
    // `b` declares `int` but the branch below never resumes, so `b`'s
    // result is never produced — the `perform` waiting for it never
    // returns — and the branch answers `string` for the whole region.
    ok("effect Mixed { control a: fn(int) -> int\n\
         control b: fn() -> int }\n\
        fn body() -> string !Mixed = {\n\
        let ignored = perform Mixed.b()\n\
        \"unreached\"\n\
        }\n\
        let out = {\n\
            handle Mixed {\n\
                a x => resume(x)\n\
                b => match 1 > 0 {\n\
                true => \"stopped\"\n\
                false => resume(0)\n\
                }\n\
            }\n\
            body()\n\
        }\n");
}

#[test]
fn a_resuming_arm_may_not_answer_an_erased_word() {
    // The arm's value IS the region's result, so an `any` arm in a `string`
    // region hands the caller an erased word to read as a pointer —
    // recovery by declared type, which [TYPE-ANY] deletes. The arm must
    // narrow structurally before it answers.
    let errs = bad(
        &mixed_region("int", "0", "string", "\"unreached\"", "erased()").replace(
            "effect Mixed {",
            "fn erased() -> any = \"x\"\neffect Mixed {",
        ),
    );
    assert!(
        errs.iter().any(|e| e
            .message
            .contains("it is `any` and that result is `string`")),
        "an `any` arm must not answer a `string` region; got {errs:?}"
    );
}

#[test]
fn a_value_arm_is_unaffected_by_a_sibling_control_arm() {
    // Mode is per OPERATION and declared. `b` is a value operation, so its
    // arm substitutes for `b`'s own declared result no matter what the
    // control sibling `a` does — an `int` arm belongs in a `string` region
    // and a `string` one does not. Reading the mode region-wide was issue
    // #177; reading it from the arm's body was its successor.
    ok("effect Mixed { control a: fn(int) -> int\n\
         b: fn() -> int }\n\
        fn body() -> string !Mixed = \"v=${perform Mixed.b()}\"\n\
        let out = {\n\
            handle Mixed {\n\
                a x => resume(x)\n\
                b => 7\n\
            }\n\
            body()\n\
        }\n");
    let errs = bad("effect Mixed { control a: fn(int) -> int\n\
          b: fn() -> int }\n\
         fn body() -> string !Mixed = \"v=${perform Mixed.b()}\"\n\
         let out = {\n\
             handle Mixed {\n\
                 a x => resume(x)\n\
                 b => \"seven\"\n\
             }\n\
             body()\n\
         }\n");
    assert!(
        !errs.is_empty(),
        "a `string` arm cannot supply an `int` result"
    );
}

#[test]
fn a_value_arm_may_not_resume() {
    // The declaration, not the body, owns the mode: an undecorated
    // operation has no continuation to give away, so `resume` in its arm is
    // a defect rather than a reclassification. [EFFECTS-HANDLER-ARMS]
    let errs = bad("effect Mixed { b: fn() -> int }\n\
         fn body() -> string !Mixed = \"v=${perform Mixed.b()}\"\n\
         let out = {\n\
             handle Mixed {\n\
                 b => resume(7)\n\
             }\n\
             body()\n\
         }\n");
    assert!(
        errs.iter().any(|e| e.message.contains("cannot `resume`")),
        "a value arm must not resume; got {errs:?}"
    );
}

#[test]
fn a_fully_non_resuming_region_still_substitutes_values() {
    // No arm resumes, so the handler is inlined at each `perform` and the
    // arm's value IS the operation's result — the rule the abandoning case
    // must not weaken, because it is what pins a generic effect's
    // instantiation. [EFFECTS-GENERIC-INSTANTIATION]
    let errs = bad("effect Mixed { b: fn() -> int }\n\
                    fn body() -> string !Mixed = \"v=${perform Mixed.b()}\"\n\
                    let out = {\n\
                        handle Mixed {\n\
                            b => \"not an int\"\n\
                        }\n\
                        body()\n\
                    }\n");
    assert!(
        !errs.is_empty(),
        "a `string` arm cannot supply an `int` result"
    );
}

#[test]
fn performed_effect_operation_must_be_declared() {
    // [EFFECTS-OP-TYPING]
    let missing = bad("effect Logger { log: fn(string) -> Unit }\n\
         fn run() -> Unit !Logger = perform Logger.missing(\"hi\")\n");
    assert!(missing
        .iter()
        .any(|e| e.message.contains("has no operation `missing`")));
}

#[test]
fn performed_effect_must_be_declared() {
    let errors = bad("fn run() -> Unit !Missing = perform Missing.log(\"hi\")\n");
    assert!(errors
        .iter()
        .any(|e| e.message.contains("unknown effect `Missing`")));
}

#[test]
fn performed_effect_operation_must_match_declared_arity() {
    let wrong_arity = bad("effect Logger { log: fn(string) -> Unit }\n\
         fn run() -> Unit !Logger = perform Logger.log()\n");
    assert!(wrong_arity
        .iter()
        .any(|e| e.message.contains("expects 1 argument(s), got 0")));
}

#[test]
fn handler_arm_operation_must_be_declared() {
    let bad_arm = bad("effect Logger { log: fn(string) -> Unit }\n\
         let result = {\n\
             handle Logger {\n\
                 missing msg => {}\n\
             }\n\
             0\n\
         }\n");
    assert!(bad_arm
        .iter()
        .any(|e| e.message.contains("has no operation `missing`")));
}

#[test]
fn handler_arm_operation_must_match_declared_arity() {
    let errors = bad("effect Logger { log: fn(string) -> Unit }\n\
         let result = {\n\
             handle Logger {\n\
                 log => {}\n\
             }\n\
             0\n\
         }\n");
    assert!(errors
        .iter()
        .any(|e| e.message.contains("expects 1 parameter(s), got 0")));
}
