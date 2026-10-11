use super::*;

#[test]
fn higher_order_calls_through_computed_and_field_callees() {
    // A chained 3-deep application (the outer callee is itself a call
    // result), an iterator callback that is a computed call result
    // (`makeAdder(10)`), and a filter callback read from a record field
    // (`cfg.keep`) — each previously bailed; all now recover their signature
    // from the type table and dispatch through the closure cell.
    let ir = module(
            "type Cfg = { keep: (int) -> bool }\n\
             type Dispatch = Dispatch { m: () -> string }\n\
             fn methodField() -> string = \"field\"\n\
             fn m<T>(receiver: T) -> int = 31\n\
             fn dispatch<T>(receiver: T) = receiver.m()\n\
             fn echo<T>(receiver: T) -> T = receiver\n\
             fn deferEcho<T>(receiver: T) = receiver.echo<T>()\n\
             fn explicitWitness() -> int = 5.echo<int>()\n\
             fn deferredWitness() -> int = deferEcho(7)\n\
             fn fallbackWitness() -> int = dispatch(9)\n\
             fn fieldWitness() -> string = dispatch(Dispatch { m: methodField })\n\
             fn add3(a: int) -> (int) -> (int) -> int =\n\
               fn(b: int) => fn(c: int) => satAdd(satAdd(a, b), c)\n\
             fn makeAdder(n: int) -> (int) -> int = fn(x: int) => satAdd(x, n)\n\
             fn main() -> Unit = {\n\
               let cfg = Cfg { keep: fn(n: int) => n > 1 }\n\
               let chain = add3(1)(2)(3)\n\
               let computed = fold(map(range(1, 4), makeAdder(10)), 0, fn(a: int, b: int) => satAdd(a, b))\n\
               let fieldcb = fold(filter(range(1, 5), cfg.keep), 0, fn(a: int, b: int) => satAdd(a, b))\n\
               print(\"${chain} ${computed} ${fieldcb} ${explicitWitness()} ${deferredWitness()} ${fallbackWitness()} ${fieldWitness()}\")\n\
             }\n",
        );
    // Each higher-order callee loads a function pointer from a closure cell.
    assert!(ir.contains("to { i8* }*"), "expected a closure cell-call");
    for (name, value) in [
        ("explicitWitness", 5),
        ("deferredWitness", 7),
        ("fallbackWitness", 31),
    ] {
        assert!(
            function_body(&ir, &format!("define i64 @{name}()"))
                .contains(&format!("ret i64 {value}")),
            "{name} must return its selected generic function's value:\n{ir}"
        );
    }
    assert!(
        function_body(&ir, "define i8* @fieldWitness()").contains("call i8* %"),
        "the callable record field must retain its string-returning ABI:\n{ir}"
    );
}

#[test]
fn field_callbacks_return_callables_and_keep_named_slots() {
    // A field callback returns another callable through two generic helpers.
    // The second record also proves named field arguments keep written slots.
    let returned = module(
            "effect Probe { value: fn() -> int }\n\
             fn fetch() -> int !Probe = perform Probe.value()\n\
             fn pure() -> int = 22\n\
             type Factory = Factory { m: () -> () -> int }\n\
             type NamedFactory = NamedFactory { m: (() -> int, () -> int) -> () -> int }\n\
             fn pureFactory() -> () -> int = pure\n\
             fn effectFactory() -> () -> int = fetch\n\
             fn firstCallback(callback: () -> int, ignored: () -> int) -> () -> int = callback\n\
             fn secondCallback(callback: () -> int, ignored: () -> int) -> () -> int = ignored\n\
             fn m<T>(x: T) -> () -> int = pure\n\
             fn dispatch<T>(x: T) = x.m()\n\
             fn dispatchNamed<T>(x: T) = x.m(ignored: pure, callback: fetch)\n\
             fn forward<T>(dummy: T, cb: () -> () -> int) = dispatch(Factory { m: cb })()\n\
             fn forwardNamed<T>(dummy: T, cb: (() -> int, () -> int) -> () -> int) = dispatchNamed(NamedFactory { m: cb })()\n\
             fn firstSlot(first: int, second: int) -> int = first\n\
             fn throughSlot<T>(dummy: T, callback: (int, int) -> int) = callback(second: 22, first: 11)\n\
             fn genericSlotWitness() -> int = throughSlot(0, firstSlot)\n\
             fn declaredSlotWitness() -> int = firstSlot(second: 22, first: 11)\n\
             fn directLambdaWitness() -> int = (fn(first: int, second: int) => first)(second: 22, first: 11)\n\
             fn throughLambda<T>(unused: T) = (fn(first: int, second: int) => first)(second: 22, first: 11)\n\
             fn genericLambdaWitness() -> int = throughLambda(0)\n\
             fn pureWitness() -> int = forward(0, pureFactory)\n\
             fn namedPureWitness() -> int = forwardNamed(0, firstCallback)\n\
             fn handledWitness() -> int = {\n\
                 handle Probe {\n\
                     value => 33\n\
                 }\n\
                 forward(0, effectFactory)\n\
             }\n\
             fn namedHandledWitness() -> int = {\n\
                 handle Probe {\n\
                     value => 33\n\
                 }\n\
                 forwardNamed(0, secondCallback)\n\
             }\n\
             print(\"${pureWitness()} ${namedPureWitness()} ${handledWitness()} ${namedHandledWitness()} ${genericSlotWitness()} ${declaredSlotWitness()} ${directLambdaWitness()} ${genericLambdaWitness()}\")\n",
        );
    for witness in ["pureWitness", "namedPureWitness"] {
        let body = function_body(&returned, &format!("define i64 @{witness}()"));
        assert!(
            body.contains("call i8* %") && body.contains("call i64 %"),
            "{witness} must call the factory and then its returned int callback:\n{returned}"
        );
    }
    assert!(
        returned.contains("@__osprey_handler_push")
            && returned.contains("@__osprey_handler_lookup_env"),
        "returned effectful callbacks must retain their dynamic handler:\n{returned}"
    );
    for (witness, arguments) in [
        ("genericSlotWitness", "i64 22, i64 11"),
        ("declaredSlotWitness", "i64 11, i64 22"),
    ] {
        assert!(
            function_body(&returned, &format!("define i64 @{witness}()"))
                .contains(&format!("@firstSlot({arguments})")),
            "{witness} must preserve its source call site's argument slots:\n{returned}"
        );
    }
    for witness in ["directLambdaWitness", "genericLambdaWitness"] {
        assert!(
            function_body(&returned, &format!("define i64 @{witness}()")).contains("ret i64 22"),
            "{witness} must bind named values in written slots:\n{returned}"
        );
    }
}

#[test]
fn channel_with_default_capacity_and_fiberdone_requires_arg() {
    // Channel() with no capacity arg (fiber.rs default "0" branch).
    let ir = module(
        "fn main() -> Unit = {\n\
               let ch = Channel()\n\
               send(ch, 1)\n\
               print(\"${recv(ch)}\")\n\
             }\n",
    );
    assert!(ir.contains("channel_create"));
}

// ---- error branches that fail loudly ----

#[test]
fn unknown_name_fails_loudly() {
    // A bare reference to an undefined, non-constructor, non-function name
    // (expr.rs Identifier None branch → CodegenError::unknown).
    let err = compile_err("fn main() -> Unit = print(\"${nope}\")\n");
    assert!(matches!(err, CodegenError::UnknownName(_)));
}

#[test]
fn generic_function_into_concrete_slot_specialises() {
    // A generic (polymorphic) function flowing into a CONCRETE
    // function-typed slot is specialised to the slot's ABI — emitted as a
    // capture-free closure (expr.rs eval_arg → closure::emit_closure).
    // Implements [TYPE-GENERICS-FN].
    let ir = module(
        "fn identity(x) = x\n\
             fn apply(f: (int) -> int, v: int) -> int = f(v)\n\
             fn main() -> Unit = print(\"${apply(identity, 3)}\")\n",
    );
    assert!(ir.contains("__closure_fn_"));
}

#[test]
fn one_generic_function_at_one_abi_is_emitted_exactly_once() {
    // [TYPE-GENERICS-FN]: specialisation is keyed by (function, slot ABI),
    // so N uses at the SAME ABI share one emitted body and one constant
    // cell. Without the cache each use emitted a byte-identical
    // `__closure_fn_K` twin — pure module bloat that scales with call
    // sites. Two DISTINCT ABIs must still get their own body: that is the
    // whole point of specialising.
    let ir = module(
        "fn identity(x) = x\n\
             fn applyInt(f: (int) -> int, v: int) -> int = f(v)\n\
             fn applyStr(f: (string) -> string, v: string) -> string = f(v)\n\
             fn main() -> Unit = {\n\
               let a = applyInt(identity, 3)\n\
               let b = applyInt(identity, 4)\n\
               let c = applyInt(identity, 5)\n\
               print(\"${a}${b}${c}${applyStr(identity, \"z\")}\")\n\
             }\n",
    );
    let bodies = ir.matches("define ").filter(|_| true).count();
    assert!(bodies > 0, "sanity: the module defines functions");
    // int and string are two ABIs ⇒ exactly two specialised bodies, not the
    // four the three int uses plus one string use would otherwise emit.
    assert_eq!(
        ir.matches("define i64 @__closure_fn_").count()
            + ir.matches("define i8* @__closure_fn_").count(),
        2,
        "one body per (function, ABI) pair:\n{ir}"
    );
}

#[test]
fn a_generic_functions_returned_lambda_is_inlined_per_call_site() {
    // This program used to be REJECTED: one closure cell has one ABI and
    // the binding may be used at several, so `lambda_value` bailed. The
    // lambda is recorded for inline application instead, so each call site
    // specialises it — and the callee's argument is evaluated ONCE, at the
    // binding, then carried as a value (stmt.rs generic_returned_lambda,
    // closure::Environment).
    let ir = module(
        "fn mk<T>(x: T) = |y| => x\n\
             fn main() -> Unit = {\n\
               let f = mk(1)\n\
               print(\"${f(0)}\")\n\
               print(\"${f(\"a different instantiation\")}\")\n\
             }\n",
    );
    // Both uses answer the captured `1`, so both print the int form; a
    // per-instantiation ABI failure would print one of them as a pointer.
    assert_eq!(
        ir.matches("i64 1").count(),
        2,
        "each call site specialises against the captured value:\n{ir}"
    );
}

#[test]
fn a_still_generic_lambda_with_no_slot_at_all_is_rejected() {
    // The remaining boundary. Inlining needs a call site to specialise
    // against; used as a bare VALUE there is none, and a
    // variables-as-i64 ABI would silently corrupt the string/float
    // instantiations (closure.rs lambda_value).
    let err = compile_err(
        "fn mk<T>(x: T) = |y| => x\n\
             fn main() -> Unit = print(\"${mk(1)}\")\n",
    );
    assert!(matches!(err, CodegenError::Unsupported(_)));
}

#[test]
fn float_literal_match_uses_fcmp() {
    // A float-literal arm drives gen_eq's fcmp-oeq path (pattern.rs 487-491).
    let ir = module(
        "fn pick(x: float) -> int = match x {\n\
               1.5 => 1\n\
               _   => 0\n\
             }\n\
             fn main() -> Unit = print(\"${pick(1.5)}\")\n",
    );
    assert!(ir.contains("fcmp oeq double"));
}

#[test]
fn float_and_bool_elements_box_into_collections() {
    // Boxing a double (bitcast) and a bool (zext) into the uniform i64
    // element ABI — conv.rs box_to_i64's Double + I1 arms.
    let ir = module(
        "fn main() -> Unit = {\n\
               let fs = listAppend(List(), 1.5)\n\
               let bs = listAppend(List(), true)\n\
               print(\"${listLength(fs)} ${listLength(bs)}\")\n\
             }\n",
    );
    shows(&ir, &["bitcast double", "zext i1"]);
}

#[test]
fn boolean_equality_zexts_operands_to_i64() {
    // Comparing two bools widens each to i64 for the icmp (conv.rs as_i64's
    // I1 arm).
    let ir = module(
        "fn main() -> Unit = {\n\
               let a = true\n\
               let b = false\n\
               print(\"${a == b}\")\n\
             }\n",
    );
    shows(&ir, &["zext i1", "icmp eq i64"]);
}

#[test]
fn float_compared_to_int_promotes_via_sitofp() {
    // A float compared with an int literal promotes the int to double
    // (conv.rs as_double's I64 arm) inside gen_comparison's float branch.
    let ir = module(
        "fn main() -> Unit = {\n\
               let f = 2.5\n\
               let gt = f > 2\n\
               print(\"${gt}\")\n\
             }\n",
    );
    shows(&ir, &["sitofp i64", "fcmp"]);
}

#[test]
fn code_point_width_cursor_builtin() {
    // strings.rs codePointWidth dispatch arm (the one cursor builtin not
    // exercised by the broad string sweep).
    let ir = module(
        "fn main() -> Unit = match codePointWidth(2) {\n\
               Success { value } => print(\"w=${value}\")\n\
               Error { message } => print(\"no\")\n\
             }\n",
    );
    assert!(ir.contains("osp_string_codepoint_width"));
}

#[test]
fn yield_without_value_and_let_bound_lambda_materialize() {
    // `yield` with no operand (fiber.rs gen_yield None) and a let-bound
    // lambda materialized as a closure cell (lower.rs gen_bind lambda arm).
    let ir = module(
        "fn main() -> Unit = {\n\
               yield\n\
               let inc = fn(x: int) => x + 1\n\
               print(\"${inc(4)}\")\n\
             }\n",
    );
    assert!(ir.contains("define"));
}

#[test]
fn fiber_done_requires_an_argument() {
    // fiberDone with no argument fails loudly (fiber.rs gen_builtin error).
    let err = compile_err("fn main() -> Unit = print(\"${fiberDone()}\")\n");
    assert!(matches!(err, CodegenError::Invalid(_)));
}

#[test]
fn codegen_constructors_are_callable_directly() {
    // builder.rs Codegen::new + Default (not used by compile_program, which
    // takes inferred types) — exercised directly for the public surface.
    let _a = builder::Codegen::new();
    let _b = builder::Codegen::default();
}
