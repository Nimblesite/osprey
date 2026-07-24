//! LLVM IR (text) code generation for Osprey.
//!
//! The backend walks the AST and prints LLVM assembly that clang compiles and
//! links against libc and the prebuilt C runtime archives in `compiler/bin/`
//! (`libfiber_runtime.a` / `libhttp_runtime.a`). Two anchors define correct
//! output: the C runtime ABI (those archives' symbols and conventions) and the
//! golden outputs in `examples/tested`, exercised end-to-end by
//! `crates/diff_examples.sh`. Constructs the backend does not lower return
//! [`CodegenError::Unsupported`] — it never emits a placeholder.
//!
//! Public surface: [`compile_program`] turns a parsed [`osprey_ast::Program`]
//! into a module string.

mod aggregate;
mod arc;
mod builder;
mod call;
mod cast;
mod closure;
mod collections;
mod conv;
mod coverage;
mod effect_generics;
mod effects;
mod error;
mod expr;
mod extern_call;
mod fiber;
mod freevars;
mod genfn;
mod iter;
mod listlit;
mod llty;
mod loops;
mod lower;
mod meta;
mod pattern;
mod result;
mod runtime;
mod strings;
mod testing;
mod types;

pub use error::{CodegenError, Result};
pub use llty::{LType, Value};
pub use lower::{compile_program, compile_program_coverage, compile_program_debug};
pub use osprey_debug::DebugSource;

/// Every identifier referenced anywhere in `program` — function bodies, lets,
/// nested modules. The CLI's capability sandbox uses this to detect gated
/// builtins (`httpGet`, `readFile`, …) without compiling.
#[must_use]
pub fn referenced_idents(program: &osprey_ast::Program) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    for s in &program.statements {
        stmt_idents(s, &mut out);
    }
    out
}

fn stmt_idents(s: &osprey_ast::Stmt, out: &mut std::collections::BTreeSet<String>) {
    use osprey_ast::Stmt;
    match s {
        Stmt::Let { value, .. } | Stmt::Assignment { value, .. } => {
            freevars::free_idents(value, out);
        }
        Stmt::Expr { value: e, .. } | Stmt::Function { body: e, .. } => {
            freevars::free_idents(e, out);
        }
        Stmt::Module { body, .. } => {
            for item in body {
                stmt_idents(&item.declaration, out);
            }
        }
        Stmt::Namespace { body, .. } => {
            for inner in body {
                stmt_idents(inner, out);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use osprey_syntax::{parse_program, parse_program_with_flavor, Flavor};

    fn module(src: &str) -> String {
        let parsed = parse_program(src);
        assert!(
            parsed.errors.is_empty(),
            "syntax errors: {:?}",
            parsed.errors
        );
        compile_program(&parsed.program).expect("codegen should succeed")
    }

    fn ml_module(src: &str) -> String {
        let parsed = parse_program_with_flavor(src, Flavor::Ml);
        assert!(
            parsed.errors.is_empty(),
            "syntax errors: {:?}",
            parsed.errors
        );
        compile_program(&parsed.program).expect("ML codegen should succeed")
    }

    fn debug_module(src: &str) -> String {
        let parsed = parse_program(src);
        assert!(
            parsed.errors.is_empty(),
            "syntax errors: {:?}",
            parsed.errors
        );
        compile_program_debug(
            &parsed.program,
            DebugSource {
                filename: "debug.osp".to_string(),
                directory: "/tmp".to_string(),
            },
        )
        .expect("debug codegen should succeed")
    }

    /// Compile `src` and assert codegen rejected it (used for the loud-failure
    /// branches that have no surface syntax of their own).
    fn compile_err(src: &str) -> CodegenError {
        let parsed = parse_program(src);
        assert!(
            parsed.errors.is_empty(),
            "syntax errors: {:?}",
            parsed.errors
        );
        compile_program(&parsed.program).unwrap_err()
    }

    #[test]
    fn emits_main_and_puts_for_hello() {
        let ir = module("print(\"hello\")\n");
        assert!(ir.contains("define i32 @main() #0"));
        assert!(ir.contains("declare i32 @puts(i8*)"));
        assert!(ir.contains("call i32 @puts"));
        assert!(ir.contains("hello\\00"));
        // Every function keeps frame pointers so the sampling profiler's
        // FP-chain walk is valid from any pc [PROF-CODEGEN-FP].
        assert!(ir.contains("attributes #0 = { \"frame-pointer\"=\"all\" }"));
        assert!(ir.trim_end().ends_with('}'));
    }

    #[test]
    fn emits_arithmetic_function() {
        // A monomorphic (annotated) function is emitted as a real definition and
        // called directly; a generic one would instead inline at its call sites.
        let ir = module("fn add(a: int, b: int) -> int = a + b\nlet r = add(2, 3)\n");
        // Parameters are named positionally, not after their source
        // identifier, so an ML/Default twin pair stays byte-identical
        // ([FLAVOR-IR-EQUIV]).
        assert!(ir.contains("define i64 @add(i64 %$p0, i64 %$p1)"));
        assert!(ir.contains("add i64 %$p0, %$p1"));
        assert!(ir.contains("call i64 @add(i64 2, i64 3)"));
    }

    // Testing built-ins lower to the TAP runtime and re-route main's exit
    // status through the epilogue. [TESTING-CODEGEN][TESTING-EXIT]
    #[test]
    fn testing_builtins_lower_to_tap_runtime_calls() {
        let ir = module("test(\"adds\", fn() => expect(1 + 1, 2))\n");
        assert!(ir.contains("call i32 @osp_test_begin(i8*"));
        assert!(ir.contains("call void @osp_test_end(i8*"));
        assert!(ir.contains("call void @osp_test_assert(i8* null, i32"));
        assert!(ir.contains("call i32 @osp_test_finalize()"));
        assert!(ir.contains("call i32 @strcmp(i8*"));
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
    fn programs_without_tests_keep_the_plain_exit_path() {
        let ir = module("print(\"hi\")\n");
        assert!(!ir.contains("osp_test_finalize"));
        assert!(ir.contains("ret i32 0"));
    }

    #[test]
    fn user_functions_shadow_testing_builtins() {
        // [TESTING-SHADOWING] a user `check` compiles as an ordinary call.
        let ir = module("fn check(t: int) -> int = t + 1\nlet r = check(4)\nprint(r)\n");
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
        // Success renders bare, Error renders as Error(<message>).
        let ir = module("expect(intDiv(1, 0), 2)\n");
        assert!(ir.contains("call void @osp_test_assert(i8* null, i32"));
        assert!(ir.contains("Error(%s)"));
        // A list/map/record operand has no canonical rendering — loud error.
        assert!(compile_err("expect([1, 2], [1, 3])\n")
            .to_string()
            .contains("list, map, or record"));
    }

    #[test]
    fn assertion_operands_unwrap_results_before_comparing() {
        // [TESTING-EQUALITY] intDiv returns Result<int, _>; expect compares
        // its unwrapped payload's canonical string.
        let ir = module("expect(intDiv(4, 2), 2)\n");
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
            debug_module("fn add(a: int, b: int) -> int = a + b\nlet x = add(1, 2)\nprint(x)\n");
        let expected_dwarf_version = if cfg!(target_os = "macos") { 4 } else { 5 };

        assert!(ir.contains("source_filename = \"/tmp/debug.osp\""));
        assert!(ir.contains("!llvm.dbg.cu = !{!"));
        assert!(ir.contains("!llvm.module.flags = !{!"));
        assert!(ir.contains("!DICompileUnit("));
        assert!(ir.contains("!DIFile(filename: \"debug.osp\", directory: \"/tmp\")"));
        assert!(ir.contains(&format!("!\"Dwarf Version\", i32 {expected_dwarf_version}")));
        assert!(ir.contains("!DISubprogram(name: \"add\""));
        assert!(ir.contains("!DISubprogram(name: \"main\""));
        assert!(ir.contains("!DILocalVariable(name: \"x\""));
        // Parameters (a, b) use dbg.value — SSA args live for the whole
        // function. `let` locals (x) use dbg.declare over a stack slot, the
        // robust -O0 representation that keeps the line table free of stray
        // line-0 rows. [DEBUGGER-DBG-DECLARE]
        assert!(ir.contains("@llvm.dbg.value"));
        assert!(ir.contains("call void @llvm.dbg.declare(metadata"));
        assert!(ir.contains("!DILocation(line: 2, column: 1"));
        assert!(ir.contains(", !dbg !"));
    }

    #[test]
    fn generic_function_inlines_at_call_site() {
        // A polymorphic function is specialised by inlining, so no monomorphic
        // definition is emitted; the call computes directly at the use site.
        let ir = module("fn identity(x) = x\nlet a = identity(7)\nprint(\"v=${a}\")\n");
        assert!(!ir.contains("@identity"));
    }

    #[test]
    fn spawn_lowers_to_a_per_instance_closure_cell() {
        // `spawn` lowers its expression as a zero-parameter closure: the thunk
        // takes its heap cell as env (so two in-flight spawns from one site
        // never alias captures) and goes to `fiber_spawn_env`; `await` maps to
        // `fiber_await`. No module globals are involved.
        let ir = module(
            "fn work(n: int) -> int = n * 2\n\
             fn main() -> Unit = {\n\
               let x = 21\n\
               let f = spawn work(x)\n\
               print(\"got ${await(f)}\")\n\
             }\n",
        );
        assert!(ir.contains("call i64 @fiber_spawn_env(i64 (i8*)* @__fiber_thunk_"));
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
             let r = apply(value: 10, f: fn(x: int) => x + 1)\n\
             print(\"r=${r}\")\n",
        );
        assert!(ir.contains("define i64 @__closure_fn_0(i8* %__env, i64 %$p0)"));
        assert!(ir.contains("@__closure_cell_0 = private unnamed_addr constant { i8* }"));
        assert!(ir.contains("call i64 %"));
    }

    #[test]
    fn escaping_closure_captures_its_makers_state() {
        // The headline closure case [TYPE-FN-CLOSURE]: a returned lambda
        // capturing its maker's parameter stays callable — the capture is
        // stored in a malloc'd cell and reloaded from `%__env` inside the
        // lifted function.
        let ir = module(
            "fn makeAdder(n: int) -> (int) -> int = fn(x: int) => x + n\n\
             fn main() -> Unit = {\n\
               let add5 = makeAdder(5)\n\
               print(\"r=${add5(3)}\")\n\
             }\n",
        );
        assert!(ir.contains("define i8* @makeAdder(i64 %$p0)"));
        assert!(ir.contains("bitcast i8* %__env to { i8*, i64 }*"));
        assert!(ir.contains("call i8* @osp_alloc"));
    }

    #[test]
    fn interpolation_uses_sprintf() {
        let ir = module("let x = 7\nprint(\"x=${x}\")\n");
        assert!(ir.contains("@sprintf"));
        assert!(ir.contains("@osp_alloc"));
    }

    #[test]
    fn match_lowers_to_phi() {
        let ir = module("fn pick(a: int, b: int) -> int = match a < b { true => a false => b }\n");
        assert!(ir.contains("icmp"));
        assert!(ir.contains("br i1"));
        assert!(ir.contains("phi i64"));
    }

    #[test]
    fn named_arguments_are_ordered_by_declaration() {
        // Call sites pass b before a; the emitted call must follow declared order.
        // `sub`'s `a - b` body infers `Result<int, MathError>`, so the call's
        // return type is `{ i64, i8 }*`; what matters here is the argument order.
        let ir = module("fn sub(a, b) = a - b\nlet r = sub(b: 1, a: 9)\n");
        assert!(ir.contains("@sub(i64 9, i64 1)"));
    }

    #[test]
    fn unsupported_construct_fails_loudly() {
        // A construct the backend cannot lower must fail loudly, never
        // silently (CLAUDE.md: no placeholders, fail hard). A method call on a
        // value reaches codegen only through the UFCS rewrite, so a synthetic
        // raw MethodCall node is unsupported.
        let program = osprey_ast::Program {
            statements: vec![osprey_ast::Stmt::Expr {
                value: osprey_ast::Expr::MethodCall {
                    target: Box::new(osprey_ast::Expr::Integer(1)),
                    method: String::from("frobnicate"),
                    arguments: Vec::new(),
                    named_arguments: Vec::new(),
                },
                position: None,
            }],
        };
        let err = compile_program(&program).unwrap_err();
        assert!(matches!(err, CodegenError::Unsupported(_)));
    }

    // ---- referenced_idents / stmt_idents (lib.rs 62-65) ----

    #[test]
    fn referenced_idents_walks_lets_funcs_and_nested_modules() {
        let parsed = parse_program(
            "type Ignored = A | B\n\
             module M {\n\
               fn helper(x) = httpGet(url)\n\
               let y = readFile(path)\n\
             }\n\
             let z = spawnProcess(cmd)\n",
        );
        assert!(parsed.errors.is_empty(), "syntax: {:?}", parsed.errors);
        let idents = referenced_idents(&parsed.program);
        // Module body (a Stmt::Module) recurses; its inner fn + let contribute.
        // The `type` declaration hits stmt_idents' catch-all arm.
        assert!(idents.contains("httpGet"));
        assert!(idents.contains("readFile"));
        assert!(idents.contains("spawnProcess"));
    }

    // ---- iterators: range / map / filter / fold over ranges + lists ----

    #[test]
    fn range_pipeline_map_filter_foreach_and_fold() {
        // range → map (record stage) → filter (record stage) → forEach (replay
        // both stages, counted loop), plus a fold accumulator. Exercises iter.rs
        // callback_of (named + lambda), replay, for_each, fold, acc_*.
        let ir = module(
            "fn dbl(x: int) -> int = x * 2\n\
             fn big(x: int) -> bool = x > 4\n\
             fn add(a: int, b: int) -> int = a + b\n\
             fn main() -> Unit = {\n\
               range(1, 6) |> map(dbl) |> filter(big) |> forEach(print)\n\
               let s = range(1, 6) |> fold(0, add)\n\
               print(\"sum=${s}\")\n\
             }\n",
        );
        assert!(ir.contains("@osp_alloc"));
        assert!(ir.contains("icmp ne i64"));
        assert!(ir.contains("alloca i64"));
    }

    #[test]
    fn list_builders_map_filter_fold_and_foreach() {
        // mapList / filterList / foldList / forEachList over a runtime list.
        // Exercises iter.rs list_builder (both branches), fold_list,
        // for_each_list and collections list-builder protocol.
        let ir = module(
            "fn dbl(x: int) -> int = x * 2\n\
             fn keep(x: int) -> bool = x > 1\n\
             fn add(a: int, b: int) -> int = a + b\n\
             fn main() -> Unit = {\n\
               let xs = listAppend(listAppend(List(), 1), 2)\n\
               let m = mapList(xs, dbl)\n\
               let f = filterList(xs, keep)\n\
               let t = foldList(xs, 0, add)\n\
               forEachList(xs, print)\n\
               print(\"len=${listLength(m)} f=${listLength(f)} t=${t}\")\n\
             }\n",
        );
        assert!(ir.contains("osprey_list_builder_new"));
        assert!(ir.contains("osprey_list_builder_push"));
        assert!(ir.contains("osprey_list_builder_seal"));
    }

    #[test]
    fn iterator_lambda_callbacks_inline_and_let_bound() {
        // An inline lambda and a let-bound lambda both serve as iterator
        // callbacks — iter.rs callback_of's Lambda arms + the lambdas cache.
        let ir = module(
            "fn main() -> Unit = {\n\
               let f = fn(x: int) => x + 1\n\
               range(0, 3) |> map(f) |> forEach(fn(x: int) => print(\"v=${x}\"))\n\
             }\n",
        );
        assert!(ir.contains("call"));
    }

    #[test]
    fn iterator_callback_must_be_fn_or_lambda() {
        // A non-identifier, non-lambda expression in callback position fails
        // loudly (iter.rs callback_of's catch-all Err).
        let err = compile_err("fn main() -> Unit = forEach(range(0, 3), 1 + 1)\n");
        assert!(matches!(err, CodegenError::Unsupported(_)));
    }

    // ---- closures / free variables ----

    #[test]
    fn closure_captures_map_list_object_and_interpolation_free_vars() {
        // A returned closure capturing several outer locals exercises
        // freevars.rs Map/Object/List/interpolation walks and closure.rs
        // capture_list + reload_captures + cell_value (malloc cell).
        let ir = module(
            "fn make(a: int, b: int) -> () -> int = fn() => {\n\
               let m = { \"k\": a }\n\
               let o = { x: b, y: a }\n\
               let xs = [a, b]\n\
               print(\"${a} ${b} ${listLength(xs)}\")\n\
               a + b\n\
             }\n\
             fn main() -> Unit = {\n\
               let g = make(1, 2)\n\
               print(\"r=${g()}\")\n\
             }\n",
        );
        assert!(ir.contains("call i8* @osp_alloc"));
        assert!(ir.contains("bitcast i8* %__env"));
    }

    #[test]
    fn nested_closure_returns_closure_value_in_block_tail() {
        // A closure value as a block tail (closure.rs lambda_value path) and a
        // capture-free closure (the constant-global cell branch in cell_value).
        let ir = module(
            "fn outer() -> () -> int = {\n\
               let k = 9\n\
               fn() => k\n\
             }\n\
             fn pure() -> () -> int = fn() => 7\n\
             fn main() -> Unit = {\n\
               let a = outer()\n\
               let b = pure()\n\
               print(\"${a()} ${b()}\")\n\
             }\n",
        );
        assert!(ir.contains("private unnamed_addr constant { i8* }"));
    }

    #[test]
    fn named_function_used_as_a_value_emits_forwarder() {
        // A bare top-level function name in value position becomes its closure
        // forwarder cell (closure.rs named_fn_cell + emit_forwarder), then is
        // called through the cell.
        let ir = module(
            "fn dbl(x: int) -> int = x * 2\n\
             fn apply(f: (int) -> int, v: int) -> int = f(v)\n\
             fn main() -> Unit = {\n\
               let r = apply(dbl, 21)\n\
               print(\"r=${r}\")\n\
             }\n",
        );
        assert!(ir.contains("@__fnval_") || ir.contains("@__closure"));
    }

    // ---- pattern matching: union variants, list patterns, result, literals ----

    #[test]
    fn union_match_binds_variant_fields_and_catch_all() {
        // User-union match: tag load + per-variant branch + field binding
        // (pattern.rs gen_union_match, bind_variant_fields) and a catch-all arm.
        let ir = module(
            "type Shape =\n\
               Circle { r: int }\n\
               | Square { s: int }\n\
               | Blank\n\
             fn area(sh: Shape) -> int = match sh {\n\
               Circle { r } => r * r\n\
               Square { s } => s * s\n\
               _ => 0\n\
             }\n\
             fn main() -> Unit = print(\"a=${area(Circle { r: 3 })}\")\n",
        );
        assert!(ir.contains("load i64, i64*"));
        assert!(ir.contains("icmp eq i64"));
    }

    #[test]
    fn list_pattern_match_binds_head_tail_and_fixed_lengths() {
        // List-pattern match: length guards (eq / sge), prefix + rest binding,
        // wildcard element, trailing catch-all (pattern.rs gen_list_match,
        // bind_list_arm).
        let ir = module(
            "fn classify(xs) = match xs {\n\
               []                  => \"empty\"\n\
               [only]              => \"one(${only})\"\n\
               [_, second, ...rest] => \"rest=${listLength(rest)}\"\n\
               other               => \"other(${listLength(other)})\"\n\
             }\n\
             fn main() -> Unit = {\n\
               let xs = listAppend(listAppend(List(), 1), 2)\n\
               print(classify(xs))\n\
             }\n",
        );
        assert!(ir.contains("osprey_list_length"));
        assert!(ir.contains("osprey_list_get"));
        assert!(ir.contains("osprey_list_drop"));
        assert!(ir.contains("icmp sge i64"));
    }

    #[test]
    fn result_match_success_and_error_arms() {
        // Result discrimination: branch on the i8 disc, bind Success value /
        // Error message (pattern.rs gen_result_match, emit_result_arm).
        let ir = module(
            "fn main() -> Unit = {\n\
               let xs = listAppend(List(), 10)\n\
               match listGet(xs, 0) {\n\
                 Success { value } => print(\"v=${value}\")\n\
                 Error { message } => print(\"e=${message}\")\n\
               }\n\
             }\n",
        );
        assert!(ir.contains("icmp eq i8"));
    }

    #[test]
    fn result_match_on_a_scalar_discriminant() {
        // Matching a bare scalar against Success/Error arms falls back to the
        // `disc >= 0 ⇒ Success` rule (pattern.rs gen_result_match scalar branch).
        let ir = module(
            "fn main() -> Unit = {\n\
               let n = 5\n\
               match n {\n\
                 Success { value } => print(\"v=${value}\")\n\
                 Error { message } => print(\"e=${message}\")\n\
               }\n\
             }\n",
        );
        assert!(ir.contains("icmp sge i64"));
    }

    #[test]
    fn union_match_with_named_catch_all_binds_the_scrutinee() {
        // A union match whose catch-all is a binding (not `_`) binds the whole
        // scrutinee (pattern.rs gen_union_match's Binding catch-all arm).
        let ir = module(
            "type Shape = Circle { r: int } | Square { s: int } | Blank\n\
             fn name(sh: Shape) -> int = match sh {\n\
               Circle { r } => r\n\
               other        => 0\n\
             }\n\
             fn main() -> Unit = print(\"${name(Blank)}\")\n",
        );
        assert!(ir.contains("load i64, i64*"));
    }

    #[test]
    fn string_literal_match_chain() {
        // A string-literal compare/branch chain ending in a catch-all
        // (pattern.rs gen_literal_match + gen_eq's strcmp path).
        let ir = module(
            "fn route(p: string) -> string = match p {\n\
               \"/a\" => \"A\"\n\
               \"/b\" => \"B\"\n\
               _     => \"404\"\n\
             }\n\
             fn main() -> Unit = print(route(\"/a\"))\n",
        );
        assert!(ir.contains("@strcmp"));
    }

    // ---- strings ----

    #[test]
    fn string_builtins_total_and_fallible() {
        // A broad sweep of string builtins: total transforms, predicates,
        // fallible parse/substring/split, cursor ops, fromCodePoint, join.
        let ir = module(
            "fn main() -> Unit = {\n\
               let s = \"  Hello World  \"\n\
               print(\"${length(s)} ${isEmpty(s)} ${contains(s, \"World\")}\")\n\
               print(\"${startsWith(s, \"  \")} ${endsWith(s, \"  \")}\")\n\
               print(\"${toUpperCase(s)} ${toLowerCase(s)} ${trim(s)}\")\n\
               print(\"${trimStart(s)} ${trimEnd(s)} ${reverse(s)}\")\n\
               print(\"${take(s, 2)} ${drop(s, 2)}\")\n\
               print(\"${repeat(\"ab\", 3)} ${padStart(\"x\", 3, \"-\")} ${padEnd(\"x\", 3, \"-\")}\")\n\
               print(\"${replace(\"aaa\", \"a\", \"b\")} ${byteLength(s)}\")\n\
               match indexOf(s, \"World\") { Success { value } => print(\"i=${value}\") Error { message } => print(\"no\") }\n\
               match substring(s, 2, 5) { Success { value } => print(\"sub=${value}\") Error { message } => print(\"no\") }\n\
               match parseInt(\"42\") { Success { value } => print(\"n=${value}\") Error { message } => print(\"no\") }\n\
               match parseFloat(\"4.5\") { Success { value } => print(\"f=${value}\") Error { message } => print(\"no\") }\n\
               match split(\"a,b,c\", \",\") { Success { value } => print(\"parts=${listLength(value)}\") Error { message } => print(\"no\") }\n\
               match byteAt(s, 0) { Success { value } => print(\"b=${value}\") Error { message } => print(\"no\") }\n\
               match codePointAt(s, 0) { Success { value } => print(\"cp=${value}\") Error { message } => print(\"no\") }\n\
               match fromCodePoint(65) { Success { value } => print(\"c=${value}\") Error { message } => print(\"no\") }\n\
               let ws = words(s)\n\
               let ls = lines(s)\n\
               print(\"${join(ws, \"-\")} ${listLength(ls)}\")\n\
             }\n",
        );
        assert!(ir.contains("@osp_strlen"));
        assert!(ir.contains("osp_string_to_upper"));
        assert!(ir.contains("osp_parse_int_strict"));
        assert!(ir.contains("osp_string_codepoint_at"));
        assert!(ir.contains("osp_string_join"));
    }

    // ---- collections: map literals, map operations ----

    #[test]
    fn map_literal_and_map_operations() {
        // Map literal build, set/get/contains/remove/merge, keys/values lists,
        // indexing — collections.rs gen_map_literal, map_* and map_to_list.
        let ir = module(
            "fn main() -> Unit = {\n\
               let m = { \"a\": 1, \"b\": 2 }\n\
               let m2 = mapSet(m, \"c\", 3)\n\
               let m3 = mapRemove(m2, \"a\")\n\
               let merged = m2 + m3\n\
               print(\"len=${mapLength(merged)} has=${mapContains(m2, \"a\")}\")\n\
               print(\"keys=${listLength(mapKeys(m2))} vals=${listLength(mapValues(m2))}\")\n\
               match mapGet(m2, \"b\") { Success { value } => print(\"g=${value}\") Error { message } => print(\"no\") }\n\
               match m[\"a\"] { Success { value } => print(\"i=${value}\") Error { message } => print(\"no\") }\n\
             }\n",
        );
        assert!(ir.contains("osprey_map_builder_new"));
        assert!(ir.contains("osprey_map_set"));
        assert!(ir.contains("osprey_map_remove"));
        assert!(ir.contains("osprey_map_iter_new"));
    }

    #[test]
    fn bare_length_and_is_empty_dispatch_on_the_receiver_type() {
        // [BUILTIN-COLLECTION-LENGTH], [BUILTIN-COLLECTION-ISEMPTY]: the bare
        // spec names are receiver-directed. Routing a List/Map handle into the
        // string runtime reads an `i8*` heap pointer as a NUL-terminated string
        // — a wrong answer AND an out-of-bounds read.
        let ir = module(
            "fn main() -> Unit = {\n\
               let xs = listAppend(listAppend(List(), 1), 2)\n\
               let m = mapSet(Map(), \"a\", 1)\n\
               print(\"${length(xs)} ${isEmpty(xs)} ${length(m)} ${isEmpty(m)} ${length(\"ab\")}\")\n\
             }\n",
        );
        assert!(
            ir.contains("osprey_list_length"),
            "List length must use the list runtime"
        );
        assert!(
            ir.contains("osprey_map_length"),
            "Map length must use the map runtime"
        );
        assert!(
            ir.contains("osp_strlen"),
            "string length must still use the string runtime"
        );
        // Exactly one `osp_strlen` call site: the sole string receiver.
        assert_eq!(
            ir.matches("call i64 @osp_strlen").count(),
            1,
            "only the string receiver may reach osp_strlen"
        );
        assert_eq!(
            ir.matches("@osp_string_is_empty").count(),
            0,
            "no collection receiver may reach the string isEmpty"
        );
    }

    #[test]
    fn list_get_and_contains_runtime_calls() {
        // listGet (bounds-checked Result) and listContains (linear scan with
        // both the int and string equality paths).
        let ir = module(
            "fn main() -> Unit = {\n\
               let xs = listAppend(listAppend(List(), 1), 2)\n\
               let ss = listAppend(List(), \"hi\")\n\
               print(\"c=${listContains(xs, 2)} s=${listContains(ss, \"hi\")}\")\n\
               match listGet(xs, 0) { Success { value } => print(\"v=${value}\") Error { message } => print(\"no\") }\n\
             }\n",
        );
        assert!(ir.contains("osprey_list_in_bounds"));
        assert!(ir.contains("@strcmp"));
    }

    // ---- algebraic effects: handler-owned state (effects.rs) ----

    #[test]
    fn handler_owned_mutable_state_threads_through_a_heap_cell() {
        // A `mut` an effect handler arm captures is promoted to a shared heap
        // cell: the env-carrying handler ABI passes the cell pointer, `get`
        // loads it and `set` stores it, so `perform` threads real state.
        let ir = module(
            "effect State { get: fn() -> int  set: fn(int) -> Unit }\n\
             fn bump() -> int !State = { let a = perform State.get()  perform State.set(a + 1)  perform State.get() }\n\
             fn main() -> int { mut c = 0\n  let r = handle State get => c set v => { c = v } in bump()\n  print(\"r=${toString(r)} c=${toString(c)}\")\n  0 }\n",
        );
        // env-carrying handler ABI (push takes a 4th i8* env; perform resolves it)
        assert!(ir.contains("declare i32 @__osprey_handler_push(i8*, i8*, i8*, i8*)"));
        assert!(ir.contains("@__osprey_handler_lookup_env"));
        // the captured `mut` became a heap cell (malloc'd, stored, loaded)
        assert!(ir.contains("@osp_alloc"));
        // each arm is emitted with the hidden leading env parameter
        assert!(ir.contains("i8* %__env"));
    }

    #[test]
    fn fiber_await_unboxes_a_string_result_to_a_pointer() {
        // `await(spawn e)` recovers the fiber's element type: a string result is
        // a pointer, recovered with `inttoptr`, not kept as a raw integer.
        let ir = module(
            "fn greet(n: string) -> string = \"hi ${n}\"\n\
             fn main() -> Unit = print(await(spawn greet(\"x\")))\n",
        );
        assert!(ir.contains("fiber_await"));
        assert!(ir.contains("inttoptr i64"));
    }

    // ---- conversions / arithmetic (conv.rs) ----

    #[test]
    fn float_and_mixed_arithmetic_exercises_conversions() {
        // Float arithmetic, int→double promotion, division (always float),
        // negation, comparisons — conv.rs as_double/as_i64/box_to_i64 and
        // expr.rs arith/division/comparison/unary branches.
        let ir = module(
            "fn main() -> Unit = {\n\
               let f = 3.5\n\
               let i = 2\n\
               let mixed = f + i\n\
               let q = 10.0 / f\n\
               let neg = -f\n\
               let negi = -i\n\
               let lt = f < 5.0\n\
               let m = f % 2.0\n\
               print(\"${mixed} ${q} ${neg} ${negi} ${lt} ${m}\")\n\
             }\n",
        );
        assert!(ir.contains("sitofp i64"));
        assert!(ir.contains("fdiv double"));
        assert!(ir.contains("fneg double"));
        assert!(ir.contains("fcmp"));
    }

    #[test]
    fn boolean_logic_and_unary_not() {
        // && / || lower to i1 and/or; `not` / `!` to xor; bool box/zext paths.
        let ir = module(
            "fn main() -> Unit = {\n\
               let a = true\n\
               let b = false\n\
               let c = a && b\n\
               let d = a || b\n\
               let e = !a\n\
               print(\"${c} ${d} ${e}\")\n\
             }\n",
        );
        assert!(ir.contains("and i1"));
        assert!(ir.contains("or i1"));
        assert!(ir.contains("xor i1"));
    }

    // ---- records / aggregate (aggregate.rs) ----

    #[test]
    fn record_construct_field_access_and_update() {
        // Construct a record, read fields, update one — aggregate.rs
        // gen_constructor, gen_field_access, gen_update.
        let ir = module(
            "type Point = { x: int, y: int }\n\
             fn main() -> Unit = {\n\
               let p = Point { x: 1, y: 2 }\n\
               let p2 = p { x: 9 }\n\
               print(\"${p.x} ${p.y} ${p2.x}\")\n\
             }\n",
        );
        assert!(ir.contains("getelementptr"));
        assert!(ir.contains("store i64"));
    }

    #[test]
    fn ml_curried_string_result_compares_with_string_parameter() {
        let ir = ml_module(
            r#"value : int -> string -> string -> string
value doc path fallback = fallback

card : int -> int -> string -> string
card doc index selected =
    id = value doc "[${index}].id" "0"
    match id == selected
        true => " selected"
        false => ""
"#,
        );
        assert!(ir.contains("call i32 @strcmp(i8*"));
    }

    // ---- fibers (fiber.rs) ----

    #[test]
    fn fibers_channels_yield_select_and_done() {
        // spawn/await (covered elsewhere) plus Channel/send/recv, yield with and
        // without a value, select first-arm, fiber_yield and fiberDone.
        let ir = module(
            "fn work(n: int) -> int = n + 1\n\
             fn main() -> Unit = {\n\
               let ch = Channel(1)\n\
               send(ch, 42)\n\
               let got = recv(ch)\n\
               let y = yield 5\n\
               let z = fiber_yield(9)\n\
               let pick = select { 1 => 100  2 => 200 }\n\
               let f = spawn work(3)\n\
               print(\"${got} ${y} ${z} ${pick} ${await(f)} ${fiberDone(f)}\")\n\
             }\n",
        );
        assert!(ir.contains("channel_create"));
        assert!(ir.contains("channel_send"));
        assert!(ir.contains("channel_recv"));
        assert!(ir.contains("fiber_done"));
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
             fn add3(a: int) -> (int) -> (int) -> int =\n\
               fn(b: int) => fn(c: int) => a + b + c\n\
             fn makeAdder(n: int) -> (int) -> int = fn(x: int) => x + n\n\
             fn main() -> Unit = {\n\
               let cfg = Cfg { keep: fn(n: int) => n > 1 }\n\
               let chain = add3(1)(2)(3)\n\
               let computed = fold(map(range(1, 4), makeAdder(10)), 0, fn(a: int, b: int) => a + b)\n\
               let fieldcb = fold(filter(range(1, 5), cfg.keep), 0, fn(a: int, b: int) => a + b)\n\
               print(\"${chain} ${computed} ${fieldcb}\")\n\
             }\n",
        );
        // Each higher-order callee loads a function pointer from a closure cell.
        assert!(ir.contains("to { i8* }*"), "expected a closure cell-call");
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
    fn generic_function_value_without_a_slot_is_rejected() {
        // With NO consuming slot to fix the ABI (a still-generic lambda
        // returned from a generic function), codegen still rejects loudly —
        // a variables-as-i64 ABI would silently corrupt string/float
        // instantiations (closure.rs lambda_value).
        let err = compile_err(
            "fn mk<T>(x: T) = |y| => x\n\
             fn main() -> Unit = {\n\
               let f = mk(1)\n\
               print(\"${f(0)}\")\n\
             }\n",
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
        assert!(ir.contains("bitcast double"));
        assert!(ir.contains("zext i1"));
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
        assert!(ir.contains("zext i1"));
        assert!(ir.contains("icmp eq i64"));
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
        assert!(ir.contains("sitofp i64"));
        assert!(ir.contains("fcmp"));
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
        let _a = crate::builder::Codegen::new();
        let _b = crate::builder::Codegen::default();
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
}
