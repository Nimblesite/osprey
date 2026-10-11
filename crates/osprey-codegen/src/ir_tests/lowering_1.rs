use super::*;
#[test]
fn emits_main_and_puts_for_hello() {
    let ir = module("print(\"hello\")\n");
    shows(
        &ir,
        &[
            "define i32 @main() #0",
            "declare i32 @puts(i8*)",
            "call i32 @puts",
            "hello\\00",
        ],
    );
    // Every function keeps frame pointers so the sampling profiler's
    // FP-chain walk is valid from any pc [PROF-CODEGEN-FP].
    assert!(ir.contains("attributes #0 = { \"frame-pointer\"=\"all\" }"));
    assert!(ir.trim_end().ends_with('}'));
}

#[test]
fn emits_arithmetic_function() {
    // A monomorphic (annotated) function is emitted as a real definition and
    // called directly; a generic one would instead inline at its call sites.
    let ir = module(
        "fn add(a: int, b: int) -> int = {\n\
               handle Arith { overflow _ _ _ wrapped => wrapped }\n\
               a + b\n\
             }\n\
             let r = add(2, 3)\n",
    );
    // Parameters are named positionally, not after their source
    // identifier, so an ML/Default twin pair stays byte-identical
    // ([FLAVOR-IR-EQUIV]).
    assert!(ir.contains("define i64 @add(i64 %$p0, i64 %$p1)"));
    assert!(
        ir.contains("call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %$p0, i64 %$p1)"),
        "integer addition must lower through LLVM's checked intrinsic:\n{ir}"
    );
    assert!(ir.contains("call i64 @add(i64 2, i64 3)"));
}

// Testing built-ins lower to the TAP runtime and re-route main's exit
// status through the epilogue. [TESTING-CODEGEN][TESTING-EXIT]
#[test]
fn testing_builtins_lower_to_tap_runtime_calls() {
    let ir = module("test(\"adds\", fn() => expect(1 + 1, 2))\n");
    shows(
        &ir,
        &[
            "call i32 @osp_test_begin(i8*",
            "call void @osp_test_end(i8*",
            "call void @osp_test_assert(i8* null, i32",
            "call i32 @osp_test_finalize()",
            "call i32 @strcmp(i8*",
        ],
    );
}

#[test]
fn check_lowers_with_label_and_named_body_calls_through() {
    let ir = module("fn body() = check(\"sum\", 4, 2 + 2)\ntest(\"named\", body)\n");
    // The label operand is a real string pointer, not the expect null.
    assert!(ir.contains("@osp_test_assert(i8* %"));
    assert!(!ir.contains("@osp_test_assert(i8* null"));
    assert!(ir.contains("call i32 @osp_test_begin(i8*"));
}

#[test]
fn boolean_assertion_shortcuts_lower_with_expected_values_and_labels() {
    let ir = module(
        "test(\"predicates\", fn() => {\n\
               expectTrue(2 < 3)\n\
               expectFalse(3 < 2)\n\
               checkTrue(\"ordered\", 4 <= 4)\n\
               checkFalse(\"different\", 4 == 5)\n\
             })\n",
    );
    assert_eq!(ir.matches("call void @osp_test_assert").count(), 4);
    assert_eq!(ir.matches("@osp_test_assert(i8* null").count(), 2);
}

#[test]
fn grouped_assertions_emit_one_soft_assertion_per_condition() {
    let ir = module(
        "test(\"batch\", fn() => {\n\
               expectAll([1 == 1, 2 < 3, true])\n\
               checkAll(\"state\", [4 == 4, 5 != 6])\n\
             })\n",
    );
    assert_eq!(ir.matches("call void @osp_test_assert").count(), 5);
    assert_eq!(ir.matches("@osp_test_assert(i8* null").count(), 3);
    assert!(compile_err("let checks = [true]\nexpectAll(checks)\n")
        .to_string()
        .contains("must be a list literal"));
    assert!(compile_err("expectAll([])\n")
        .to_string()
        .contains("at least one condition"));
}

#[test]
fn programs_without_tests_keep_the_plain_exit_path() {
    let ir = module("print(\"hi\")\n");
    assert!(!ir.contains("osp_test_finalize"));
    assert!(ir.contains("ret i32 0"));
}

#[test]
fn user_functions_shadow_testing_builtins() {
    // [TESTING-SHADOWING] a user `check` compiles as an ordinary call.
    let ir = module("fn check(t: int) -> int = satAdd(t, 1)\nlet r = check(4)\nprint(r)\n");
    assert!(!ir.contains("osp_test_assert"));
    assert!(ir.contains("call i64 @check(i64 4)"));
    // …and so does an extern declaration of the same name.
    let ir = module(
        "extern fn check(a: int, b: int, c: int) -> int\nlet r = check(1, 2, 3)\nprint(r)\n",
    );
    assert!(!ir.contains("osp_test_assert"));
}

#[test]
fn error_results_render_visibly_and_handles_are_rejected() {
    // [TESTING-EQUALITY] a Result operand branches on its discriminant:
    // Success renders bare, Error renders as Error(<message>). Overflow
    // reaches a value only through `checkedAdd` ([ARITH-EFFECT]).
    let ir = module("expect(checkedAdd(9223372036854775807, 1), 2)\n");
    shows(
        &ir,
        &["call void @osp_test_assert(i8* null, i32", "Error(%s)"],
    );
    // A list/map/record operand has no canonical rendering — loud error.
    assert!(compile_err("expect([1, 2], [1, 3])\n")
        .to_string()
        .contains("list, map, or record"));
}

#[test]
fn assertion_operands_render_success_payloads_without_hiding_errors() {
    // [TESTING-EQUALITY] checkedAdd returns Result<int, Error>; expect
    // compares a Success payload canonically, while the sibling test proves
    // Error remains visible rather than being read as a fabricated payload.
    let ir = module("expect(checkedAdd(1, 1), 2)\n");
    assert!(ir.contains("call void @osp_test_assert(i8* null, i32"));
}

#[test]
fn testing_builtin_arity_errors_are_loud() {
    // The type gate rejects these in the CLI; codegen still fails loudly
    // on its own rather than emitting a broken call.
    assert!(compile_err("test(\"only name\")\n")
        .to_string()
        .contains("test needs"));
    assert!(compile_err("expect(1)\n")
        .to_string()
        .contains("expect needs"));
    assert!(compile_err("check(\"l\", 1)\n")
        .to_string()
        .contains("check needs"));
}

#[test]
fn debug_compile_emits_source_level_metadata() {
    let ir =
        debug_module("fn add(a: int, b: int) -> int = satAdd(a, b)\nlet x = add(1, 2)\nprint(x)\n");
    let expected_dwarf_version = if cfg!(target_os = "macos") { 4 } else { 5 };

    shows(
        &ir,
        &[
            "source_filename = \"/tmp/debug.osp\"",
            "!llvm.dbg.cu = !{!",
            "!llvm.module.flags = !{!",
            "!DICompileUnit(",
            "!DIFile(filename: \"debug.osp\", directory: \"/tmp\")",
            &format!("!\"Dwarf Version\", i32 {expected_dwarf_version}"),
            "!DISubprogram(name: \"add\"",
            "!DISubprogram(name: \"main\"",
            "!DILocalVariable(name: \"a\", arg: 1,",
            "!DILocalVariable(name: \"b\", arg: 2,",
            "!DILocalVariable(name: \"x\"",
        ],
    );
    // Parameters (a, b) retain their formal argument metadata and use the
    // same addressable storage as local x, so register reuse cannot erase
    // a source value at a later breakpoint. [DEBUGGER-DBG-DECLARE]
    shows(
        &ir,
        &[
            "@llvm.dbg.value",
            "call void @llvm.dbg.declare(metadata",
            "!DILocation(line: 2, column: 1",
            ", !dbg !",
        ],
    );
}

#[test]
fn a_breakpoint_inside_a_handler_arm_body_has_a_line_to_bind_to() {
    // A handler arm is emitted as its own LLVM function, and a nested
    // function starts with NO debug scope. Nothing reopened one, so every
    // instruction lowered from the arm carried no `!dbg` and the arm got no
    // `DISubprogram` — the arm's source lines were simply absent from the
    // line table. A debugger has nothing to bind a breakpoint to there, so
    // one set inside the arm never fires even though the arm runs.
    // [DEBUGGER-DBG-DECLARE]
    let ir = debug_module(
        "effect State { get: fn() -> int }\n\
             fn bump() -> int !State = perform State.get()\n\
             fn main() -> int {\n\
             let r = {\n\
             handle State {\n\
             get => {\n\
             let inner = 41\n\
             inner\n\
             }\n\
             }\n\
             bump()\n\
             }\n\
             print(\"r=${toString(r)}\")\n\
             0 }\n",
    );

    assert!(
        ir.contains("!DISubprogram(name: \"__handler_State_get_"),
        "the arm function needs its own subprogram to be a debuggable scope"
    );
    // `let inner = 41` is line 7, inside the arm.
    assert!(
        ir.contains("!DILocation(line: 7,"),
        "the arm body's own statement lines must reach the line table"
    );

    // A `resume`-using arm is emitted down a separate path, and it is the
    // same construct to the author — so it must be just as debuggable.
    let resuming = debug_module(
        "effect State { control get: fn() -> int }\n\
             fn bump() -> int !State = perform State.get()\n\
             fn main() -> int {\n\
             let r = {\n\
             handle State {\n\
             get => {\n\
             let inner = 41\n\
             resume(inner)\n\
             }\n\
             }\n\
             bump()\n\
             }\n\
             print(\"r=${toString(r)}\")\n\
             0 }\n",
    );

    assert!(
        resuming.contains("!DILocation(line: 7,"),
        "a resuming arm's body lines must reach the line table too"
    );
}

#[test]
fn generic_function_inlines_at_call_site() {
    // A polymorphic function is specialised by inlining, so no monomorphic
    // definition is emitted; the call computes directly at the use site.
    let ir = module("fn identity(x) = x\nlet a = identity(7)\nprint(\"v=${a}\")\n");
    assert!(!ir.contains("@identity"));
}

#[test]
fn generic_function_as_an_iterator_callback_inlines_not_calls_a_missing_symbol() {
    // [BUILTIN-ITER-CALLBACK] A reducer with unannotated params is generic,
    // so it has NO `@name` definition. Passed to `fold` it must be
    // beta-reduced per element (inlined), not lowered to `call @choose` — a
    // call to a symbol that was never emitted (invalid IR).
    let ir = module(
        "fn choose(a, b) = match a == b { true => a false => b }\n\
             let t = range(1, 4) |> fold(0, choose)\n\
             print(\"t=${t}\")\n",
    );
    assert!(
        !ir.contains("@choose"),
        "generic reducer must inline, not call @choose:\n{ir}"
    );
    assert!(
        ir.contains("icmp eq i64") && ir.contains("phi i64"),
        "the reducer body must be emitted inline"
    );
}

#[test]
fn fold_with_a_record_accumulator_lowers_and_recovers_the_pointer() {
    // [MEM-BACKENDS] A record-typed fold accumulator lives in the uniform
    // i64 slot; the combine (inlined) must see it as a tagged pointer so its
    // record update type-checks, and the result must be `inttoptr`'d back so
    // the following field access reads a real pointer. Before the fix this
    // panicked in codegen ("`p` is not a record").
    let ir = module(
        "type Box = { n: int }\n\
             fn bump(b, s) = b { n: b.n }\n\
             let r = range(1, 3) |> fold(Box { n: 7 }, bump)\n\
             print(\"n=${r.n}\")\n",
    );
    assert!(
        ir.contains("inttoptr i64"),
        "record fold result must be unboxed to a pointer:\n{ir}"
    );
    assert!(
        !ir.contains("@bump"),
        "generic combine must inline, not call @bump"
    );
}

#[test]
fn spawn_lowers_to_a_per_instance_closure_cell() {
    // `spawn` lowers its expression as a zero-parameter closure: the thunk
    // takes its heap cell as env (so two in-flight spawns from one site
    // never alias captures) and goes to `fiber_spawn_env_owned`; `await`
    // maps to `fiber_await`. No module globals are involved.
    let ir = module(
        "fn work(n: int) -> int = satMul(n, 2)\n\
             fn main() -> Unit = {\n\
               let x = 21\n\
               let f = spawn work(x)\n\
               print(\"got ${await(f)}\")\n\
             }\n",
    );
    assert!(ir.contains("call i64 @fiber_spawn_env_owned(i64 (i8*)* @__fiber_thunk_"));
    assert!(
        ir.lines()
            .any(|line| line.contains("@fiber_spawn_env_owned") && line.ends_with(", i64 0)")),
        "an erased scalar fiber result must not be probed as a managed pointer:\n{ir}"
    );
    assert!(ir.contains("define i64 @__fiber_thunk_0(i8* %__env)"));
    assert!(!ir.contains("@__fiber_cap_"));
    assert!(ir.contains("call i64 @fiber_await(i64"));
}

#[test]
fn inline_lambda_argument_becomes_a_closure_cell() {
    // An inline lambda flowing into a function-typed parameter becomes a
    // closure cell `{ fnptr, captures… }`: the emitted function takes a
    // hidden `i8* %__env`, and the indirect call inside `apply` loads the
    // fnptr from the cell and passes the cell back as the env.
    let ir = module(
        "fn apply(value: int, f: (int) -> int) -> int = f(value)\n\
             let r = apply(value: 10, f: fn(x: int) => satAdd(x, 1))\n\
             print(\"r=${r}\")\n",
    );
    shows(
        &ir,
        &[
            "define i64 @__closure_fn_0(i8* %__env, i64 %$p0)",
            "@__closure_cell_0 = private unnamed_addr constant { i8* }",
            "call i64 %",
        ],
    );
}

#[test]
fn escaping_closure_captures_its_makers_state() {
    // The headline closure case [TYPE-FN-CLOSURE]: a returned lambda
    // capturing its maker's parameter stays callable — the capture is
    // stored in a malloc'd cell and reloaded from `%__env` inside the
    // lifted function.
    let ir = module(
        "fn makeAdder(n: int) -> (int) -> int = fn(x: int) => satAdd(x, n)\n\
             fn main() -> Unit = {\n\
               let add5 = makeAdder(5)\n\
               print(\"r=${add5(3)}\")\n\
             }\n",
    );
    shows(
        &ir,
        &[
            "define i8* @makeAdder(i64 %$p0)",
            "bitcast i8* %__env to { i8*, i64 }*",
            "call i8* @osp_alloc",
        ],
    );
}

#[test]
fn interpolation_uses_sprintf() {
    let ir = module("let x = 7\nprint(\"x=${x}\")\n");
    shows(&ir, &["@sprintf", "@osp_alloc"]);
}

/// An erasure builds a shape-carrying box, and ownership crosses it in
/// exactly one direction ([TYPE-ANY], plan 0027 §8): the erasing frame's
/// fresh reference MOVES into the box (no compensating retain), while an
/// `any` → `any` pass-through copies the box pointer and BORROWS — the
/// epilogue retains the borrowed return instead of inventing an owner.
/// The reverted transfer-on-erase rule (§6) failed precisely because the
/// bare word could not distinguish those two flows; the box's descriptor
/// is what makes the distinction real, so #208's over-release cannot
/// re-appear without breaking these shapes. [GC-ARC-PERCEUS]
#[test]
fn erasure_boxes_with_one_way_ownership() {
    let ir = module(
        "fn dynamic() -> any = \"a\" + \"b\"\n\
             fn identity(x: any) -> any = x\n\
             print(\"${dynamic()} ${identity(7)}\")\n",
    );
    // The erasing producer returns the BOX (a pointer), not a raw word,
    // and its payload slot is masked managed so every backend's drop walk
    // releases the referent through the box: `{ i8* desc, i64 payload }`
    // with word 1 marked is meta 513.
    let producer = function_body(&ir, "define i8* @dynamic()");
    assert!(
        producer.contains("@osp_alloc_tagged_noinit(") && producer.contains(", i64 513)"),
        "an erasing return must box its referent with a managed payload:\n{producer}"
    );
    assert!(
        producer.contains("store i8* null, i8** %arc.s0"),
        "the fresh referent must MOVE into the box, not gain a second owner:\n{producer}"
    );
    assert!(
        producer.contains("@osp.any.desc.string"),
        "the box must carry the string shape descriptor:\n{producer}"
    );
    // The pass-through function receives and returns the box UNTOUCHED:
    // no new box, no descriptor reference — and the borrowed return is
    // retained (+1) rather than entered in the ledger as a fresh owner.
    let pass = function_body(&ir, "define i8* @identity(i8* %$p0)");
    assert!(
        !pass.contains("@osp.any.desc"),
        "an `any` -> `any` pass-through must not re-box:\n{pass}"
    );
    assert!(
        pass.contains("@osp_retain"),
        "a borrowed erased return must be retained for the caller:\n{pass}"
    );
}
