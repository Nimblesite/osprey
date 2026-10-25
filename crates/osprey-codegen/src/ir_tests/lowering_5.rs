use super::*;

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
        "fn pick(x) = match x {\n\
               1.5 => 1\n\
               _   => 0\n\
             }\n\
             fn main() -> Unit = print(\"${pick(1.5)}\")\n",
    );
    shows(&ir, &["fcmp oeq double"]);
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
    shows(&ir, &["osp_string_codepoint_width"]);
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
    shows(&ir, &["define"]);
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

#[test]
fn codegen_error_display_covers_all_variants() {
    // error.rs Display for every CodegenError variant.
    assert_eq!(
        CodegenError::unsupported("x").to_string(),
        "codegen: unsupported construct: x"
    );
    assert_eq!(
        CodegenError::unknown("n").to_string(),
        "codegen: unknown name `n`"
    );
    assert_eq!(
        CodegenError::invalid("p").to_string(),
        "codegen: invalid program: p"
    );
}

/// A handler with one RESUMING arm and one SUBSTITUTING arm continues by
/// re-entering its own dispatcher with the same arguments. A plain call
/// there grew one native frame per operation the substituting arm answered:
/// release builds happened to fold it away, debug builds compile at `-O0`
/// where nothing does, and a loop of otherwise constant-space performs
/// exhausted the host stack. The reuse must be a property of the MODULE,
/// not a hope about the optimizer. Implements [EFFECTS-HANDLER-ARMS].
#[test]
fn a_mixed_handler_dispatcher_continues_by_guaranteed_tail_call() {
    let ir = module(
        "effect Log {\n\
               note: fn(int) -> Unit\n\
               control ask: fn(int) -> int\n\
               }\n\
               fn hammer(n) = range(0, n) |> forEach(fn(i) => perform Log.note(i))\n\
               let done = {\n\
                   handle Log {\n\
                       note value => value\n\
                       ask value => resume(1)\n\
                   }\n\
                   hammer(3)\n\
               }\n\
               print(\"done\")\n",
    );
    let body = function_body(
        &ir,
        "define internal i64 @__resume_drive_Log_0(i8* %__env, i8* %__coro)",
    );
    let reentries: Vec<&str> = body
        .lines()
        .filter(|line| line.contains("call i64 @__resume_drive_Log_0"))
        .collect();
    assert_eq!(reentries.len(), 1, "one re-entry per dispatcher: {body}");
    assert!(
        reentries.iter().all(|line| line.contains("musttail call")),
        "the dispatcher must re-enter itself with a guaranteed tail call, saw: {reentries:?}"
    );
}

/// A `match` joining an object literal (`__obj_N`) with a row-typed parameter
/// (`__row#…`) names one physical block twice. The join must keep an owner,
/// or field access on the merged value guesses a record by field name: a
/// hard error here, and another record's slots read silently once a second
/// record declares the field (#272).
#[test]
fn match_join_keeps_one_owner_for_one_row_layout() {
    let ir = module(
        "type Region = { region: string, revision: int, json: string }\n\
         fn describe(next) = \"${next.value} ${next.revision}\"\n\
         fn pick(cached, flag) = describe(match flag { true => { revision: 1, value: \"a\" } false => cached })\n\
         fn named(next) = Region { region: \"r\", revision: next.revision, json: next.value }\n\
         fn relabel(cached, flag) = named(match flag { true => { revision: 2, value: \"b\" } false => cached })\n\
         print(pick({ revision: 9, value: \"c\" }, false))\n\
         print(relabel({ revision: 8, value: \"d\" }, true).json)\n",
    );
    // Each body writes slot N once building the literal arm and must read it
    // back through the same row block after the join, never through Region's.
    for header in [
        "define internal i8* @pick(i8* %$p0, i1 %$p1)",
        "define internal i8* @relabel(i8* %$p0, i1 %$p1)",
    ] {
        let body = function_body(&ir, header);
        for slot in ["i32 0, i32 1", "i32 0, i32 2"] {
            let row = body
                .lines()
                .filter(|line| {
                    line.contains("getelementptr { i64, i64, i8* }") && line.ends_with(slot)
                })
                .count();
            assert_eq!(row, 2, "{header} must write and read row {slot}:\n{body}");
        }
    }
}

/// A Result that a match takes apart on the spot is never built: matching a
/// `listGet` branches on the bounds check, reads the element only in bounds,
/// and binds the reason as a constant. A member scan is a loop of these, so a
/// box per step made every JSON field lookup allocate once per member.
#[test]
fn matching_a_list_lookup_allocates_no_result() {
    let ir = module(
        "fn pick(items, index) = match listGet(items, index) {\n\
             Success { value } => value\n\
             Error { message } => length(message)\n\
         }\n\
         fn named(items, index) = match listGet(items, index) {\n\
             Success { value } => value\n\
             Error => \"none\"\n\
         }\n\
         print(toString(pick([1, 2, 3], 1)))\n\
         print(named([\"a\"], 4))\n",
    );
    for header in [
        "define internal i64 @pick(i8* %$p0, i64 %$p1)",
        "define internal i8* @named(i8* %$p0, i64 %$p1)",
    ] {
        let body = function_body(&ir, header);
        testkit::contents(&body, &[], &["@osp_alloc"]);
        let bounds = body.find("@osprey_list_in_bounds").unwrap_or(usize::MAX);
        let read = body.find("@osprey_list_get").unwrap_or(0);
        let branch = body[bounds.min(body.len())..]
            .find("br i1")
            .map(|at| at + bounds);
        assert!(
            branch.is_some_and(|at| at < read),
            "{header} reads before the bounds branch:\n{body}"
        );
    }
}
