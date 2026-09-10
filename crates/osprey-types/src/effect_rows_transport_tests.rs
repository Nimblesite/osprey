//! [EFFECTS-STATIC-DISCHARGE] Preserve effects through generic dispatch,
//! higher-order arguments and handler results without capturing authority.

use crate::effect_rows_tests::{assert_accepted, diagnostics};
use crate::TypeError;

const PROBE: &str = "effect Probe { value: fn() -> int }\n\
    fn fetch() -> int !Probe = perform Probe.value()\n\
    fn pure() -> int = 22\n";
const ACTION: &str = "type Action = Action { m: () -> int }\n";
const DISPATCH: &str = "fn m<T>(x:T) -> int = 44\nfn dispatch<T>(x:T) = x.m()\n";
const UNHANDLED: &str =
    "unhandled effect operations at program entry: Probe.value; add a matching `handle`";
const GENERIC_UNHANDLED: &str =
    "unhandled effect operations at program entry: Probe<int>.value; add a matching `handle`";

fn assert_errors(source: &str, expected: &[&str]) {
    assert_eq!(
        diagnostics(source),
        expected
            .iter()
            .map(|message| TypeError::new(*message))
            .collect::<Vec<_>>(),
        "{source}"
    );
}

fn probe_call(declarations: &str, expression: &str, required: bool) {
    let source = format!("{PROBE}{declarations}\nlet answer = {expression}\n");
    assert_errors(&source, if required { &[UNHANDLED] } else { &[] });
    assert_accepted(&format!(
        "{PROBE}{declarations}\nlet answer = handle Probe\n    value => 33\nin {expression}\n"
    ));
}

#[test]
fn only_the_selected_method_branch_contributes_effects() {
    for (field, fallback, field_effectful) in [("fetch", "22", true), ("pure", "fetch()", false)] {
        let declarations = format!(
            "{ACTION}fn m<T>(x:T) -> int = {fallback}\n\
             fn dispatch<T>(x:T) = x.m()\nlet r = Action{{m:{field}}}\n"
        );
        for (call, required) in [
            ("r.m()", field_effectful),
            ("dispatch(r)", field_effectful),
            ("dispatch(5)", !field_effectful),
            ("dispatch(length(\"abc\"))", !field_effectful),
            ("dispatch(intDiv(8,2) ?: 0)", !field_effectful),
        ] {
            probe_call(&declarations, call, required);
        }
    }
}

#[test]
fn deferred_method_arguments_keep_positional_and_named_callback_effects() {
    for invocation in ["x.m(cb)", "x.m(cb:cb)"] {
        let declarations = format!(
            "type Action = Action {{m: (() -> int) -> int}}\n\
             fn field(cb: () -> int) -> int = cb()\n\
             fn m<T>(x:T, cb: () -> int) -> int = 22\n\
             fn dispatch<T>(x:T, cb: () -> int) = {invocation}\n"
        );
        probe_call(&declarations, "dispatch(Action{m:field},fetch)", true);
        probe_call(&declarations, "dispatch(5,fetch)", false);
    }
}

#[test]
fn deferred_methods_preserve_captured_receivers_and_returned_callables() {
    let captured = format!("{ACTION}{DISPATCH}fn later<T>(x:T) = fn() -> int => x.m()\n");
    probe_call(&captured, "later(Action{m:fetch})()", true);
    probe_call(&captured, "later(5)()", false);
    let returned = "type Action = Action {m: () -> () -> int}\n\
        fn field() -> () -> int = fetch\n\
        fn m<T>(x:T) -> () -> int = pure\n\
        fn dispatch<T>(x:T) = x.m()\n";
    for expression in [
        "dispatch(Action{m:field})()",
        "Action{m:field}.m()()",
        "{let f=dispatch(Action{m:field})\nf()}",
    ] {
        probe_call(returned, expression, true);
    }
    probe_call(returned, "dispatch(5)()", false);
}

#[test]
fn a_handler_inside_a_generic_helper_discharges_only_its_invocation() {
    let declarations = format!(
        "{ACTION}fn m<T>(x:T) -> int = 22\n\
         fn dispatch<T>(x:T) = handle Probe\n    value => 33\nin x.m()\n"
    );
    probe_call(&declarations, "dispatch(Action{m:fetch})", false);
    probe_call(&declarations, "dispatch(5)", false);
    probe_call(
        &declarations,
        "{let ignored=dispatch(Action{m:fetch})\nfetch()}",
        true,
    );
}

#[test]
fn higher_order_invocation_retains_named_and_curried_arguments() {
    for (declarations, invoked, ignored) in [
        (
            "fn call(callback: ()->int) -> int = callback()\n\
          fn ignore(callback: ()->int) -> int = 22\n\
          fn through(f: (()->int)->int, callback: ()->int) -> int = f(callback)\n",
            "through(call,fetch)",
            "through(ignore,fetch)",
        ),
        (
            "fn call(callback: ()->int) -> int = callback()\n\
          fn ignore(callback: ()->int) -> int = 22\n\
          fn through(f: (()->int)->int, callback: ()->int) -> int = f(callback:callback)\n",
            "through(callback:fetch,f:call)",
            "through(callback:fetch,f:ignore)",
        ),
        (
            "fn call(callback: ()->int) -> (int)->int = fn(x:int) => callback()\n\
          fn ignore(callback: ()->int) -> (int)->int = fn(x:int) => 22\n\
          fn through(f: (()->int)->(int)->int, callback: ()->int) -> int = f(callback)(1)\n",
            "through(call,fetch)",
            "through(ignore,fetch)",
        ),
    ] {
        probe_call(declarations, invoked, true);
        probe_call(declarations, ignored, false);
    }
}

#[test]
fn a_parameter_returning_a_record_keeps_actual_argument_provenance() {
    for (maker, forwarding, call, required) in [
        (
            "fn make(cb: ()->int) -> Action = Action{m:cb}",
            "fn through(makeFn: (()->int)->Action) = dispatch(makeFn(fetch))",
            "through(make)",
            true,
        ),
        (
            "fn identity(x:Action) -> Action = x",
            "fn through(makeFn: (Action)->Action) = dispatch(makeFn(Action{m:fetch}))",
            "through(identity)",
            true,
        ),
        (
            "fn identity(x:Action) -> Action = x",
            "fn through(makeFn: (Action)->Action) = dispatch(makeFn(x:Action{m:fetch}))",
            "through(identity)",
            true,
        ),
        (
            "fn make(x:int) -> (int)->Action = fn(y:int) => Action{m:fetch}",
            "fn through(makeFn: (int)->(int)->Action) = dispatch(makeFn(1)(2))",
            "through(make)",
            true,
        ),
        (
            "fn make(cb: ()->int) -> Action = Action{m:pure}",
            "fn through(makeFn: (()->int)->Action) = dispatch(makeFn(fetch))",
            "through(make)",
            false,
        ),
    ] {
        probe_call(
            &format!("{ACTION}{DISPATCH}{maker}\n{forwarding}\n"),
            call,
            required,
        );
    }
}

#[test]
fn builtin_shadowing_obeys_the_actual_function_local_or_parameter() {
    for (declarations, call) in [
        (
            "fn test(x:int) -> Action = Action{m:fetch}",
            "dispatch(test(1))",
        ),
        ("let test = |x| => Action{m:fetch}", "dispatch(test(1))"),
        (
            "fn make(x:int) -> Action = Action{m:fetch}\n\
          fn through(test: (int)->Action) = dispatch(test(1))",
            "through(make)",
        ),
        (
            "fn make(x:int) -> Action = Action{m:fetch}\n\
          fn through(makeFn: (int)->Action) = dispatch(makeFn(1))",
            "through(make)",
        ),
    ] {
        probe_call(&format!("{ACTION}{DISPATCH}{declarations}\n"), call, true);
    }
    for (declarations, call) in [
        (
            "fn test(name:string, f:()->int) -> int = 22",
            "test(\"ignored\",fetch)",
        ),
        ("let test = |name,f| => 22", "test(\"ignored\",fetch)"),
        (
            "fn ignore(name:string, f:()->int) -> int = 22\n\
          fn through(test: (string,()->int)->int) = test(\"ignored\",fetch)",
            "through(ignore)",
        ),
    ] {
        probe_call(declarations, call, false);
    }
    probe_call(
        "fn callback() = print(fetch())\n",
        "test(\"eager\",callback)",
        true,
    );
}

const GENERIC_FACTORY: &str = "type Action = Action {m: () -> int}\n\
    effect Probe<T> {value: fn() -> T}\n\
    effect Factory<T> {make: fn() -> T}\n\
    fn fetch() -> int !Probe<int> = perform Probe.value()\n\
    fn pure() -> int = 22\nfn extract<T>(x:T) = x.m\n";

#[test]
fn generic_handler_results_cannot_be_rebound_to_an_unrelated_caller_argument() {
    for body in [
        "extract(perform Factory.make())",
        "{let record=perform Factory.make()\nfn() => extract(record)()}",
    ] {
        for unrelated in ["pure", "fetch"] {
            let declarations = format!(
                "{GENERIC_FACTORY}fn get(dummy:Action) = handle Factory\n\
                 make => Action{{m:fetch}}\nin {body}\nlet f=get(Action{{m:{unrelated}}})\n"
            );
            assert_errors(
                &format!("{declarations}let answer=f()\n"),
                &[GENERIC_UNHANDLED],
            );
            assert_accepted(&format!(
                "{declarations}let answer=handle Probe\nvalue=>33\nin f()\n"
            ));
            assert_errors(
                &format!("{declarations}let answer=handle Probe\nvalue=>\"wrong\"\nin f()\n"),
                &[GENERIC_UNHANDLED],
            );
        }
    }
}

#[test]
fn handler_result_provenance_respects_local_shadowing_and_pure_results() {
    for (local, returned) in [("", "pure"), ("let fetch=fn() -> int => 22", "fetch")] {
        assert_accepted(&format!(
            "{GENERIC_FACTORY}fn get(dummy:Action) = {{\n{local}\n\
             handle Factory\nmake => Action{{m:{returned}}}\nin extract(perform Factory.make())\n\
             }}\nlet f=get(Action{{m:fetch}})\nlet answer=f()\n"
        ));
    }
}

#[test]
fn escaped_closures_capture_values_but_never_active_handler_bindings() {
    assert_errors(
        &format!("{ACTION}{PROBE}effect Factory {{make: fn() -> Action}}\n\
            fn extract<T>(x:T) = x.m\n\
            let f=handle Factory\nmake=>Action{{m:pure}}\n\
            in fn()=>extract(perform Factory.make())()\nlet answer=f()\n"),
        &[
            "unhandled effect operations at program entry: Factory.make; add a matching `handle`",
            "program entry invokes a dynamic callable whose effect provenance cannot be proven; preserve the callable through a statically tracked value path",
        ],
    );
}

#[test]
fn merging_results_and_fibers_keeps_every_callback_requirement() {
    for callback in ["fetch", "pure"] {
        for (declarations, expression) in [
            (
                format!(
                    "fn choose(flag) -> Result<()->int, Error> = match flag {{\n\
                    true => Success{{value:{callback}}}\nfalse => Success{{value:pure}}\n}}\n"
                ),
                "(choose(true) ?: pure)()",
            ),
            (
                format!(
                    "fn choose(flag) = match flag {{\n\
                    true => spawn {callback}\nfalse => spawn pure\n}}\n\
                    fn unpack<T>(fiber: Fiber<T>) -> T = await(fiber)\n"
                ),
                "unpack(choose(true))()",
            ),
        ] {
            probe_call(&declarations, expression, callback == "fetch");
        }
    }
}

#[test]
fn computed_builtin_containers_select_the_generic_free_function() {
    for expression in [
        "dispatch(mapKeys(mapSet(Map(),\"x\",1)))",
        "dispatch(toGpu([1,2]))",
        "dispatch(fromGpu(toGpu([1,2])))",
        "dispatch(gpuIota(2))",
    ] {
        probe_call(DISPATCH, expression, false);
        probe_call(
            "fn m<T>(x:T) -> int = fetch()\nfn dispatch<T>(x:T) = x.m()\n",
            expression,
            true,
        );
    }
}

#[test]
fn a_unit_field_update_preserves_other_callable_fields() {
    for callback in ["fetch", "pure"] {
        probe_call(
            &format!(
                "type Entry = Entry {{m: ()->int, flag: Unit}}\n\
                let original=Entry{{m:{callback},flag:print(\"\")}}\n\
                let updated=original{{flag:yield}}\n"
            ),
            "updated.m()",
            callback == "fetch",
        );
    }
}

#[test]
fn an_opaque_iterator_element_cannot_select_a_pure_method_fallback() {
    let declarations = format!(
        "{PROBE}{ACTION}{DISPATCH}\n\
         let items=map(range(0,1),fn(n)=>Action{{m:fetch}})\n"
    );
    for invocation in ["item.m()", "dispatch(item)"] {
        assert_errors(
            &format!("{declarations}forEach(items,fn(item)=>print({invocation}))\n"),
            &["program entry invokes a dynamic callable whose effect provenance cannot be proven; preserve the callable through a statically tracked value path"],
        );
    }
    probe_call(
        &format!("{ACTION}{DISPATCH}let items=[Action{{m:fetch}}]\n"),
        "dispatch(listGet(items,0) ?: Action{m:pure})",
        true,
    );
}

#[test]
fn a_callback_parameter_field_keeps_its_invocation_effects() {
    let declarations = format!(
        "{ACTION}{DISPATCH}\n\
         fn forward<T>(dummy:T,cb:()->int)=dispatch(Action{{m:cb}})\n"
    );
    probe_call(&declarations, "forward(0,pure)", false);
    probe_call(&declarations, "forward(0,fetch)", true);
}

#[test]
fn a_callback_parameter_field_preserves_its_returned_callable() {
    for declarations in [
        "type Factory=Factory{m:()->()->int}\n\
        fn pureFactory()->()->int=pure\nfn effectFactory()->()->int=fetch\n\
        fn m<T>(x:T)->()->int=pure\nfn dispatch<T>(x:T)=x.m()\n\
        fn forward<T>(dummy:T,cb:()->()->int)=dispatch(Factory{m:cb})()\n",
        "type Factory=Factory{m:(()->int,()->int)->()->int}\n\
        fn pureFactory(callback:()->int,ignored:()->int)->()->int=callback\n\
        fn effectFactory(callback:()->int,ignored:()->int)->()->int=ignored\n\
        fn m<T>(x:T,callback:()->int,ignored:()->int)->()->int=pure\n\
        fn dispatch<T>(x:T)=x.m(ignored:pure,callback:fetch)\n\
        fn forward<T>(dummy:T,cb:(()->int,()->int)->()->int)=dispatch(Factory{m:cb})()\n",
    ] {
        probe_call(declarations, "forward(0,pureFactory)", false);
        probe_call(declarations, "forward(0,effectFactory)", true);
    }
}

#[test]
fn named_function_value_calls_bind_written_slots_before_callback_substitution() {
    let declarations = "fn first(callback:()->int,ignored:()->int)->int=callback()\n\
        fn second(callback:()->int,ignored:()->int)->int=ignored()\n\
        fn through(f:(()->int,()->int)->int)=f(ignored:pure,callback:fetch)\n\
        fn throughGeneric<T>(dummy:T,f:(()->int,()->int)->int)=f(ignored:pure,callback:fetch)\n\
        type Action=Action{m:(()->int,()->int)->int}\n\
        fn m<T>(x:T,callback:()->int,ignored:()->int)->int=callback()\n\
        fn dispatch<T>(x:T)=x.m(ignored:pure,callback:fetch)\n";
    for (expression, required) in [
        ("first(ignored:pure,callback:fetch)", true),
        ("second(ignored:pure,callback:fetch)", false),
        ("through(first)", false),
        ("through(second)", true),
        ("throughGeneric(0,first)", false),
        ("throughGeneric(0,second)", true),
        ("{let f=first\nf(ignored:pure,callback:fetch)}", false),
        ("{let f=second\nf(ignored:pure,callback:fetch)}", true),
        ("Action{m:first}.m(ignored:pure,callback:fetch)", false),
        ("Action{m:second}.m(ignored:pure,callback:fetch)", true),
        ("dispatch(Action{m:first})", false),
        ("dispatch(Action{m:second})", true),
        ("dispatch(0)", true),
    ] {
        probe_call(declarations, expression, required);
    }
}

#[test]
fn rejected_named_performs_still_report_the_handlers_callback_effects() {
    let declarations = "effect Callback {go: fn(()->int)->int}\n";
    let unsupported = "perform `Callback.go` does not support named arguments";
    for (callback, expected) in [
        ("pure", vec![unsupported]),
        ("fetch", vec![unsupported, UNHANDLED]),
    ] {
        let invocation =
            format!("handle Callback\n go cb => cb()\nin perform Callback.go(cb:{callback})");
        assert_errors(
            &format!("{PROBE}{declarations}let answer={invocation}\n"),
            &expected,
        );
        assert_errors(
            &format!(
                "{PROBE}{declarations}let answer=handle Probe\n value => 33\nin {invocation}\n"
            ),
            &[unsupported],
        );
    }
    probe_call(
        declarations,
        "handle Callback\n go cb => cb()\nin perform Callback.go(fetch)",
        true,
    );
}
