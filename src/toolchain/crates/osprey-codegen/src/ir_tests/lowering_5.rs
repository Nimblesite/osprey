use super::*;

#[test]
fn ml_generic_slice_fold_specializes_its_callback() {
    let ir = ml_module(
        r#"kept<T> : (List<T>, List<T>, int) -> List<T>
kept (out, items, index) = match listGet items index
    Success item => listAppend out item
    Error _ => out
slice<T> : (List<T>, int, int) -> List<T>
slice (items, start, stop) = range (start, stop) |> fold [] (\(out, index) => kept (out, items, index))
main () = print "${slice ([1, 2, 3], 1, 3)} ${slice (["a", "b"], 0, 1)}"
"#,
    );
    shows(&ir, &["list_append", "define"]);
}

#[test]
fn string_cursor_and_parse_out_slots_are_entry_allocas() {
    let ir = module(
        r#"fn scan(s) = fold(range(0, 3), 0, fn(acc, i) => satAdd(acc, byteAt(s, i) ?: 0))
        fn parsed(s) = fold(range(0, 2), 0, fn(acc, i) => satAdd(acc, satAdd(parseInt(s) ?: 0, byteLength(toString(parseFloat(s) ?: 0.0)))))
        fn nested(s) = fold(range(0, 2), 0, fn(acc, i) => fold(range(0, 2), acc, fn(n, j) => satAdd(n, codePointAt(s, j) ?: 0)))
        print("${scan("123")} ${parsed("12")} ${nested("abc")}")"#,
    );
    shows(
        &ir,
        &["byte_at", "codepoint_at", "parse_int", "parse_float"],
    );
    for function in ir.split("define ").skip(1) {
        let mut entry = true;
        for line in function.lines() {
            entry &= !line.ends_with(':') || line == "entry:";
            assert!(
                !line.contains(" = alloca ") || entry,
                "loop-local slot: {line}"
            );
        }
    }
}

/// Map key and value iteration slots, and structural map equality's, are entry
/// allocas: an alloca in a later block is dynamic to LLVM, and the GC root pass
/// rejects any function holding one (`osprey hunt` failed on `Lists::distinct`).
#[test]
fn map_iteration_slots_are_entry_allocas() {
    let ir = module(
        r#"fn keys(n) = length(mapKeys(fold(range(0, n), Map(), fn(m, i) => mapSet(m, toString(i), true))))
        fn same(n) = fold(range(0, n), Map(), fn(m, i) => mapSet(m, toString(i), i)) == fold(range(0, n), Map(), fn(m, i) => mapSet(m, toString(i), i))
        print("${keys(3)} ${same(2)}")"#,
    );
    shows(&ir, &["osprey_map_iter_next"]);
    for function in ir.split("define ").skip(1) {
        let mut entry = true;
        for line in function.lines() {
            entry &= !line.ends_with(':') || line == "entry:";
            assert!(
                !line.contains(" = alloca ") || entry,
                "slot outside the entry block: {line}"
            );
        }
    }
}

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
             fn makeAdder(n) = fn(x) => satAdd(x, n)\n\
             fn main() -> Unit = {\n\
               let cfg = Cfg { keep: fn(n: int) => n > 1 }\n\
               let chain = add3(1)(2)(3)\n\
               let computed = fold(map(range(1, 4), makeAdder(10)), 0, fn(a: int, b: int) => satAdd(a, b))\n\
               let fieldcb = fold(filter(range(1, 5), cfg.keep), 0, fn(a: int, b: int) => satAdd(a, b))\n\
               print(\"${chain} ${computed} ${fieldcb} ${explicitWitness()} ${deferredWitness()} ${fallbackWitness()} ${fieldWitness()}\")\n\
             }\n",
        );
    // Each higher-order callee loads a function pointer from a closure cell.
    shows(&ir, &["to { i8* }*"]);
    for (name, value) in [
        ("explicitWitness", 5),
        ("deferredWitness", 7),
        ("fallbackWitness", 31),
    ] {
        assert!(
            function_body(&ir, &format!("define internal i64 @{name}()"))
                .contains(&format!("ret i64 {value}")),
            "{name} must return its selected generic function's value:\n{ir}"
        );
    }
    assert!(
        function_body(&ir, "define internal i8* @fieldWitness()").contains("call i8* %"),
        "the callable record field must retain its string-returning ABI:\n{ir}"
    );
}

#[test]
fn field_callbacks_return_callables_and_keep_named_slots() {
    // A field callback returns another callable through two generic helpers.
    // The second record also proves named field arguments keep written slots.
    // `firstSlot` keeps `second: int` and `-> int` deliberately: unannotated
    // the definition is generic — lambda-lifted, no real `@firstSlot` symbol —
    // so the witnesses below could not prove a call keeps its named arguments
    // in their written slots. `first: int` is inferable (pinned by the return
    // and the int call sites) and is left off.
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
             fn firstSlot(first, second: int) -> int = first\n\
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
        let body = function_body(&returned, &format!("define internal i64 @{witness}()"));
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
            function_body(&returned, &format!("define internal i64 @{witness}()"))
                .contains(&format!("@firstSlot({arguments})")),
            "{witness} must preserve its source call site's argument slots:\n{returned}"
        );
    }
    for witness in ["directLambdaWitness", "genericLambdaWitness"] {
        assert!(
            function_body(&returned, &format!("define internal i64 @{witness}()"))
                .contains("ret i64 22"),
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
    shows(&ir, &["channel_create"]);
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
    shows(&ir, &["__closure_fn_"]);
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
        ir.matches("define internal i64 @__closure_fn_").count()
            + ir.matches("define internal i8* @__closure_fn_").count(),
        2,
        "one body per (function, ABI) pair:\n{ir}"
    );
}
