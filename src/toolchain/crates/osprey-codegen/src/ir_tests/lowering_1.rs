use super::*;
use crate::testkit::contents;

/// `Type::con` without the namespace noise, so a fixture's shape reads.
fn con(name: &str, args: Vec<osprey_types::Type>) -> osprey_types::Type {
    osprey_types::Type::con(name, args)
}

/// A fresh builder plus a handle typed `word_base<elements>` — the
/// word-shape tests' fixture. The outer constructor is the handle's own
/// word, before any `#` payload tag.
fn typed_word(
    reg: &str,
    word: &str,
    elements: Vec<osprey_types::Type>,
) -> (builder::Codegen, Value) {
    let mut value = Value::handle(reg, word);
    let base = word.split('#').next().unwrap_or(word);
    value.inferred_type = Some(con(base, elements));
    (builder::Codegen::new(), value)
}

/// `src` must fail codegen with an error whose text mentions `needle`.
pub(super) fn compile_err_contains(src: &str, needle: &str) {
    assert!(
        compile_err(src).to_string().contains(needle),
        "{src:?} must be rejected mentioning {needle:?}"
    );
}

#[test]
fn map_words_recover_nested_values_from_inference_when_join_erased_the_owner() {
    let nested = con("List", vec![con("float", vec![])]);
    let (mut cg, map) = typed_word("%map", "Map", vec![con("string", vec![]), nested]);
    let value = collections::runtime_map_get(&mut cg, &map, &Value::new("0", LType::I64))
        .expect("a checked map lookup");
    assert_eq!(value.result_inner, Some(LType::Ptr));
    assert_eq!(value.payload_owner.as_deref(), Some("List#double"));
}

#[test]
fn list_words_use_concrete_inference_over_an_empty_literal_element_tag() {
    let integer = con("int", vec![]);
    let (mut cg, list) = typed_word("%list", "List#i8*", vec![integer.clone()]);
    let value = collections::elem_value(&mut cg, &list, "%word");
    assert_eq!(value.ty, LType::I64);
    assert_eq!(value.inferred_type, Some(integer));
}

#[test]
fn list_words_recover_nested_handles_from_inference_when_owner_is_untyped() {
    let nested = con("List", vec![con("int", vec![])]);
    let (mut cg, rows) = typed_word("%rows", "List", vec![nested.clone()]);
    let value = collections::elem_value(&mut cg, &rows, "%word");
    assert_eq!(value.ty, LType::Ptr);
    assert_eq!(value.osp_ty.as_deref(), Some("List#i64"));
    assert_eq!(value.inferred_type, Some(nested));
}

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
    shows(&ir, &["attributes #0 = { \"frame-pointer\"=\"all\" }"]);
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
    shows(
        &ir,
        &[
            "define internal i64 @add(i64 %$p0, i64 %$p1)",
            "call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %$p0, i64 %$p1)",
            "call i64 @add(i64 2, i64 3)",
        ],
    );
}

// Testing built-ins lower to the TAP runtime and re-route main's exit
// status through the epilogue. [TESTING-CODEGEN][TESTING-EXIT]
#[test]
fn testing_builtins_lower_to_tap_runtime_calls() {
    lowers(
        "test(\"adds\", fn() => expect(1 + 1, 2))\n",
        &[
            "call i32 @osp_test_begin(i8*",
            "call void @osp_test_end(i8*",
            "call void @osp_test_assert(i8* null, i32",
            "call i32 @osp_test_finalize()",
            "call i32 @osp_string_compare(i8*",
        ],
    );
}

#[test]
fn check_lowers_with_label_and_named_body_calls_through() {
    let ir = module("fn body() = check(\"sum\", 4, 2 + 2)\ntest(\"named\", body)\n");
    // The label operand is a real string pointer, not the expect null.
    shows(&ir, &["@osp_test_assert(i8* %"]);
    contents(&ir, &[], &["@osp_test_assert(i8* null"]);
    shows(&ir, &["call i32 @osp_test_begin(i8*"]);
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
    compile_err_contains("expectAll([])\n", "at least one condition");
}

#[test]
fn programs_without_tests_keep_the_plain_exit_path() {
    let ir = module("print(\"hi\")\n");
    contents(&ir, &["ret i32 0"], &["osp_test_finalize"]);
}

#[test]
fn user_functions_shadow_testing_builtins() {
    // [TESTING-SHADOWING] a user `check` compiles as an ordinary call.
    let ir = module("fn check(t: int) -> int = satAdd(t, 1)\nlet r = check(4)\nprint(r)\n");
    contents(&ir, &["call i64 @check(i64 4)"], &["osp_test_assert"]);
    // …and so does an extern declaration of the same name.
    let ir = module(
        "extern fn check(a: int, b: int, c: int) -> int\nlet r = check(1, 2, 3)\nprint(r)\n",
    );
    contents(&ir, &[], &["osp_test_assert"]);
}

#[test]
fn extern_declarations_keep_their_identity_when_named_like_intrinsics() {
    for name in ["print", "toString", "length", "abs"] {
        let ir = module(&format!(
            "extern fn {name}(value: int) -> int\nlet answer = {name}(41)\n"
        ));
        shows(&ir, &[&format!("call i64 @{name}(i64 41)")]);
    }
}

#[test]
fn extern_callbacks_and_iterators_keep_their_declared_identity() {
    for name in ["print", "toString", "length", "abs"] {
        for body in [
            "let values = mapList([41], foreign)\n",
            "let retained = foreign\nlet values = mapList([41], retained)\n",
        ] {
            let ir = module(&format!(
                "extern fn {name}(value: int) -> int\n{}",
                body.replace("foreign", name)
            ));
            shows(&ir, &[&format!("call i64 @{name}(i64")]);
        }
    }
}

#[test]
fn error_results_render_visibly_and_lists_compare_by_rendering() {
    // [TESTING-EQUALITY] a Result operand branches on its discriminant:
    // Success renders bare, Error renders as Error(<message>). Overflow
    // reaches a value only through `checkedAdd` ([ARITH-EFFECT]).
    let ir = module("expect(checkedAdd(9223372036854775807, 1), 2)\n");
    shows(
        &ir,
        &["call void @osp_test_assert(i8* null, i32", "Error(%s)"],
    );
    // A list or record operand compares by its rendering [BUILTIN-PRINT-FORM];
    // the checker rejects an operand with none (`print_record_unsupported.ospo`).
    lowers(
        "expect([1, 2], [1, 3])\n",
        &[
            "@osp_string_join_list",
            "call void @osp_test_assert(i8* null, i32",
        ],
    );
}

#[test]
fn assertion_operands_render_success_payloads_without_hiding_errors() {
    // [TESTING-EQUALITY] checkedAdd returns Result<int, string>; expect
    // compares a Success payload canonically, while the sibling test proves
    // Error remains visible rather than being read as a fabricated payload.
    lowers(
        "expect(checkedAdd(1, 1), 2)\n",
        &["call void @osp_test_assert(i8* null, i32"],
    );
}

#[test]
fn testing_builtin_arity_errors_are_loud() {
    // The type gate rejects these in the CLI; codegen still fails loudly
    // on its own rather than emitting a broken call.
    compile_err_contains("test(\"only name\")\n", "test needs");
    compile_err_contains("expect(1)\n", "expect needs");
    compile_err_contains("check(\"l\", 1)\n", "check needs");
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

/// The breakpoint fixture's program: `decl` names the State effect (plain
/// or `control`), `tail` is the arm's result expression — `let inner = 41`
/// lands on line 7 either way.
fn arm_debug_module(decl: &str, tail: &str) -> String {
    debug_module(&format!(
        "{decl}\n\
             fn bump() -> int !State = perform State.get()\n\
             fn main() -> int {{\n\
             let r = {{\n\
             handle State {{\n\
             get => {{\n\
             let inner = 41\n\
             {tail}\n\
             }}\n\
             }}\n\
             bump()\n\
             }}\n\
             print(\"r=${{toString(r)}}\")\n\
             0 }}\n"
    ))
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
    let ir = arm_debug_module("effect State { get: fn() -> int }", "inner");

    shows(&ir, &["!DISubprogram(name: \"__handler_State_get_"]);
    // `let inner = 41` is line 7, inside the arm.
    shows(&ir, &["!DILocation(line: 7,"]);

    // A `resume`-using arm is emitted down a separate path, and it is the
    // same construct to the author — so it must be just as debuggable.
    let resuming = arm_debug_module("effect State { control get: fn() -> int }", "resume(inner)");

    shows(&resuming, &["!DILocation(line: 7,"]);
}

#[test]
fn generic_list_default_keeps_each_call_sites_element_depth() {
    let program = parsed(
        "SPECS = [[0.0]]\nBATCHES = [[[1.0], [2.0]]]\n\
         def atl(xs, i):\n    return unwrap_or(listGet(xs, i), [])\n\
         def pair(xs):\n    return listConcat(atl(xs, 0), atl(xs, 1))\n\
         def main():\n    row = atl(SPECS, 0)\n    return print(f\"{len(row)}:{len(pair(atl(BATCHES, 0)))}\")\n",
        Flavor::Python,
    );
    let ir = compile_program(&program).expect("nested list defaults must compile");
    contents(&ir, &[], &["call i64 @osp_strlen"]);
}

#[test]
fn generic_function_inlines_at_call_site() {
    // A polymorphic function is specialised by inlining, so no monomorphic
    // definition is emitted; the call computes directly at the use site.
    let ir = module("fn identity(x) = x\nlet a = identity(7)\nprint(\"v=${a}\")\n");
    contents(&ir, &[], &["@identity"]);
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
    contents(&ir, &[], &["@choose"]);
    shows(&ir, &["icmp eq i64", "phi i64"]);
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
    contents(&ir, &["inttoptr i64"], &["@bump"]);
}

#[test]
fn fold_transfers_owned_callback_results_but_retains_borrowed_accumulators() {
    let ir = module("type Box = { n: int }\nfn fresh() = fold(range(0, 3), Box { n: 7 }, fn(b, _) => Box { n: b.n })\nfn borrowed() = fold(range(0, 3), Box { n: 7 }, fn(b, _) => b)\n");
    let fresh = function_body(&ir, "define internal i8* @fresh()");
    let borrowed = function_body(&ir, "define internal i8* @borrowed()");
    assert_eq!(
        fresh.matches("call void @osp_retain(").count(),
        0,
        "a fresh seed and each newly owned callback result move into the loop slot:\n{fresh}"
    );
    assert_eq!(borrowed.matches("call void @osp_retain(").count(), 1, "the fresh seed moves, but a borrowed callback result must be retained before the old accumulator is released:\n{borrowed}");
}

#[test]
fn list_builder_moves_fresh_records_and_retains_borrowed_records() {
    let ir = module("type Box = { n: int }\nfn made(items: List<int>) = mapList(items, fn(i) => Box { n: i })\nfn kept(items: List<Box>) = mapList(items, fn(item) => item)\n");
    let made = function_body(&ir, "define internal i8* @made(");
    let kept = function_body(&ir, "define internal i8* @kept(");
    contents(
        &made,
        &["@osprey_list_builder_push_of"],
        &["call void @osp_retain("],
    );
    contents(
        &kept,
        &["call void @osp_retain(", "@osprey_list_builder_push_of"],
        &[],
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
    shows(
        &ir,
        &["call i64 @fiber_spawn_env_owned(i64 (i8*)* @__fiber_thunk_"],
    );
    assert!(
        ir.lines()
            .any(|line| line.contains("@fiber_spawn_env_owned") && line.ends_with(", i64 0)")),
        "an erased scalar fiber result must not be probed as a managed pointer:\n{ir}"
    );
    shows(&ir, &["define internal i64 @__fiber_thunk_0(i8* %__env)"]);
    contents(&ir, &["call i64 @fiber_await(i64"], &["@__fiber_cap_"]);
}

#[test]
fn inline_lambda_argument_becomes_a_closure_cell() {
    // An inline lambda flowing into a function-typed parameter becomes a
    // closure cell `{ fnptr, captures… }`: the emitted function takes a
    // hidden `i8* %__env`, and the indirect call inside `apply` loads the
    // fnptr from the cell and passes the cell back as the env.
    lowers(
        "fn apply(value: int, f: (int) -> int) -> int = f(value)\n\
             let r = apply(value: 10, f: fn(x: int) => satAdd(x, 1))\n\
             print(\"r=${r}\")\n",
        &[
            "define internal i64 @__closure_fn_0(i8* %__env, i64 %$p0)",
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
    lowers(
        "fn makeAdder(n: int) -> (int) -> int = fn(x: int) => satAdd(x, n)\n\
             fn main() -> Unit = {\n\
               let add5 = makeAdder(5)\n\
               print(\"r=${add5(3)}\")\n\
             }\n",
        &[
            "define internal i8* @makeAdder(i64 %$p0)",
            "bitcast i8* %__env to { i8*, i64 }*",
            "call i8* @osp_alloc",
        ],
    );
}

#[test]
fn interpolation_uses_sprintf() {
    lowers(
        "let x = 7\nprint(\"x=${x}\")\n",
        &["@sprintf", "@osp_alloc"],
    );
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
    let producer = function_body(&ir, "define internal i8* @dynamic()");
    shows(&producer, &["@osp_alloc_tagged_noinit(", ", i64 513)"]);
    shows(
        &producer,
        &["store i8* null, i8** %arc.s0", "@osp.any.desc.string"],
    );
    // The pass-through function receives and returns the box UNTOUCHED:
    // no new box, no descriptor reference. It can return its parameter, so
    // it owns it [MEM-OWNERSHIP-STATIC-PARAMETERS]: the caller gives the
    // reference and the function hands the same one back, counting nothing.
    let pass = function_body(&ir, "define internal i8* @identity(i8* %$p0)");
    contents(
        &pass,
        &["store i8* %$p0, i8** %arc.s0", "ret i8* %$p0"],
        &["@osp_retain", "@osp_release", "@osp.any.desc"],
    );
}
