use super::*;

/// Rendering an erased value goes through its descriptor's render
/// function — never the raw word, which printed heap addresses as
/// integers on every backend while `--check` called the file clean
/// (finding D, [TYPE-ANY]).
#[test]
fn erased_values_render_through_their_descriptor() {
    let ir = module("fn dynamic() -> any = \"a\" + \"b\"\nprint(\"${dynamic()}\")\n");
    assert!(ir.contains("call i8* @osp.any.to_string(i8* "));
    let renderer = function_body(&ir, "define i8* @osp.any.to_string(i8* %box)");
    assert!(
        renderer.contains("call i8* %r"),
        "the shared entry must dispatch through the descriptor's render slot:\n{renderer}"
    );
}

/// A structural arm over an erased scrutinee narrows by descriptor
/// IDENTITY: the candidate set is computed at compile time from the
/// declared records, each test is one pointer compare, and the bound
/// fields are the deep-boxed children — no field name is compared at run
/// time ([PATTERN-STRUCTURAL], [TYPE-ANY]).
#[test]
fn structural_narrowing_compares_descriptors() {
    let ir = module(
        "type Point = { x: int, y: int }\n\
             fn erased() -> any = Point { x: 1, y: 2 }\n\
             fn describe(v: any) -> int = match v {\n\
               { x, y } => 1\n\
               _ => 0\n\
             }\n\
             print(\"${describe(erased())}\")\n",
    );
    let body = function_body(&ir, "define i64 @describe(i8* %$p0)");
    assert!(
        body.contains("icmp eq i8* ") && body.contains("@osp.any.desc.row.x.y"),
        "narrowing must be a descriptor pointer compare:\n{body}"
    );
    assert!(
        !body.contains("@strcmp") && !body.contains("@osp_string_equals"),
        "no name may be compared at run time:\n{body}"
    );
    // The erasure deep-boxes through the per-layout boxer, whose slots
    // hold one child box per field.
    assert!(
        ir.contains("call i8* @osp.any.boxrow.Point"),
        "a record erasure must deep-box:\n{ir}"
    );
}

/// Over a CONCRETE record scrutinee the same structural arm selects
/// statically — field loads, no descriptor anywhere. The tuple pattern is
/// the positional spelling of the same mechanism ([PATTERN-TUPLE]).
#[test]
fn structural_match_on_a_concrete_record_is_static() {
    let ir = module(
        "type Pair = Pair(int, string)\n\
             fn fst(p: Pair) -> int = match p {\n\
               (n, _) => n\n\
             }\n\
             print(\"${fst(Pair(7, \\\"a\\\"))}\")\n",
    );
    let body = function_body(&ir, "define i64 @fst(i8* %$p0)");
    assert!(
        body.contains("load i64, i64*") && !body.contains("@osp.any.desc"),
        "a concrete-record structural match binds statically:\n{body}"
    );
}

#[test]
fn match_lowers_to_phi() {
    let ir = module("fn pick(a: int, b: int) -> int = match a < b { true => a false => b }\n");
    shows(&ir, &["icmp", "br i1", "phi i64"]);
}

#[test]
fn named_arguments_are_ordered_by_declaration() {
    // Call sites pass b before a; the emitted call must follow declared order.
    // The total helper keeps this fixture focused on argument order.
    let ir = module("fn sub(a, b) = wrapSub(a, b)\nlet r = sub(b: 1, a: 9)\n");
    assert!(ir.contains("@sub(i64 9, i64 1)"));
    let external = module(
        "extern fn takeFirst(first: int, second: int) -> int\n\
             fn namedWitness() -> int = takeFirst(second: 22, first: 11)\n",
    );
    assert!(
        function_body(&external, "define i64 @namedWitness()")
            .contains("@takeFirst(i64 11, i64 22)"),
        "extern calls must preserve declaration order and the C ABI:\n{external}"
    );
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
            doc: None,
            position: None,
        }],
        doc: None,
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
        "fn dbl(x: int) -> int = satMul(x, 2)\n\
             fn big(x: int) -> bool = x > 4\n\
             fn add(a: int, b: int) -> int = satAdd(a, b)\n\
             fn main() -> Unit = {\n\
               range(1, 6) |> map(dbl) |> filter(big) |> forEach(print)\n\
               let s = range(1, 6) |> fold(0, add)\n\
               print(\"sum=${s}\")\n\
             }\n",
    );
    shows(&ir, &["@osp_alloc", "icmp ne i64", "alloca i64"]);
}

#[test]
fn list_builders_map_filter_fold_and_foreach() {
    // mapList / filterList / foldList / forEachList over a runtime list.
    // Exercises iter.rs list_builder (both branches), fold_list,
    // for_each_list and collections list-builder protocol.
    let ir = module(
        "fn dbl(x: int) -> int = satMul(x, 2)\n\
             fn keep(x: int) -> bool = x > 1\n\
             fn add(a: int, b: int) -> int = satAdd(a, b)\n\
             fn main() -> Unit = {\n\
               let xs = listAppend(listAppend(List(), 1), 2)\n\
               let m = mapList(xs, dbl)\n\
               let f = filterList(xs, keep)\n\
               let t = foldList(xs, 0, add)\n\
               forEachList(xs, print)\n\
               print(\"len=${listLength(m)} f=${listLength(f)} t=${t}\")\n\
             }\n",
    );
    shows(
        &ir,
        &[
            "osprey_list_builder_new",
            "osprey_list_builder_push",
            "osprey_list_builder_seal",
        ],
    );
}

#[test]
fn iterator_lambda_callbacks_inline_and_let_bound() {
    // An inline lambda and a let-bound lambda both serve as iterator
    // callbacks — iter.rs callback_of's Lambda arms + the lambdas cache.
    let ir = module(
        "fn main() -> Unit = {\n\
               let f = fn(x: int) => satAdd(x, 1)\n\
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
        "fn make(a: int, b: int) -> () -> int = fn() -> int => {\n\
               let m = { \"k\": a }\n\
               let o = { x: b, y: a }\n\
               let xs = [a, b]\n\
               let shown = \"${a} ${b} ${listLength(xs)}\"\n\
               match a + b { Success { value } => value Error { message } => a }\n\
             }\n\
             fn main() -> Unit = {\n\
               let g = make(1, 2)\n\
               print(\"r=${g()}\")\n\
             }\n",
    );
    shows(&ir, &["call i8* @osp_alloc", "bitcast i8* %__env"]);
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
        "fn dbl(x: int) -> int = satMul(x, 2)\n\
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
    // Each `*` is `int` and may perform `Arith.overflow`, so the program
    // installs a policy ([ARITH-EFFECT-DISCHARGE]).
    let ir = module(
        "type Shape =\n\
               Circle { r: int }\n\
               | Square { s: int }\n\
               | Blank\n\
             fn area(sh: Shape) = match sh {\n\
               Circle { r } => r * r\n\
               Square { s } => s * s\n\
               _ => 0\n\
             }\n\
             fn main() -> Unit = {\n\
               handle Arith { overflow _ _ _ wrapped => wrapped }\n\
               print(\"a=${area(Circle { r: 3 })}\")\n\
             }\n",
    );
    shows(&ir, &["load i64, i64*", "icmp eq i64"]);
    assert_eq!(
        ir.matches("call { i64, i1 } @llvm.smul.with.overflow.i64")
            .count(),
        2,
        "both integer multiplication arms must be overflow-checked:\n{ir}"
    );
}

#[test]
fn positional_sub_patterns_bind_by_slot_over_a_named_payload() {
    // `Ctor(a, b)` is the POSITIONAL destructure: column i takes payload
    // slot i, whatever the binder is spelled ([TYPE-UNION-POSITIONAL]).
    // `bind_variant_fields` used to pick its mode from the *declaration* —
    // by slot only when the variant was declared positionally, by name
    // otherwise — while `osprey-types` bound `sub_patterns` by slot for
    // every variant. Over a named payload the two disagreed: a binder that
    // named no field silently bound nothing and the arm body died at
    // `codegen: unknown name a`.
    let ir = module(
        "type Pair = Pair { first: string, second: int }\n\
             fn render(p: Pair) -> string = match p {\n\
               Pair(a, b) => \"${a}|${b}\"\n\
             }\n\
             fn main() -> Unit = print(render(Pair { first: \"ada\", second: 36 }))\n",
    );
    // Slot 0 (`first`, a string) and slot 1 (`second`, an int) are payload
    // fields 1 and 2 — field 0 is the tag. Both must be loaded, at their
    // declared LLVM types, for the two binders to have resolved by column.
    let body = function_body(&ir, "define i8* @render");
    assert!(
        body.contains("load i8*, i8**") && body.contains("load i64, i64*"),
        "both payload slots must be loaded at their declared types:\n{body}"
    );
}

#[test]
fn positional_binders_ignore_field_names_even_when_they_collide() {
    // The dangerous half of the same disagreement. Here every binder IS a
    // declared field name, but in the opposite order, so by-name and
    // by-slot binding disagree *silently* rather than failing: the checker
    // typed `second: string` / `first: int` from the columns while codegen
    // resolved each binder to its like-named slot. The program compiled and
    // printed values of the wrong static types.
    let ir = module(
        "type Pair = Pair { first: string, second: int }\n\
             fn secondSlot(p: Pair) -> int = match p {\n\
               Pair(second, first) => first\n\
             }\n\
             fn main() -> Unit = print(\"n=${secondSlot(Pair { first: \"ada\", second: 36 })}\")\n",
    );
    // `first` sits in column 1, so it must yield slot 1's i64 directly.
    // Binding by name instead reached slot 0 and returned that string
    // pointer `ptrtoint`-cast to i64 out of an `-> int` function, so the
    // program printed a raw heap address that changed between runs.
    let body = function_body(&ir, "define i64 @secondSlot");
    assert!(
        !body.contains("ptrtoint"),
        "an `-> int` arm must not return a payload pointer as an int:\n{body}"
    );
    assert!(
        body.contains("phi i64"),
        "the column-1 binder must carry slot 1's int type:\n{body}"
    );
}

#[test]
fn a_positional_declaration_binds_by_column_through_the_named_form() {
    // The third shape, and the reason the pattern form alone cannot settle
    // every case. A POSITIONALLY declared variant (`Fail(string)`) has only
    // synthetic slot names, which no binder can spell, so the *named* form
    // over it is positional after all and must fall back to the column —
    // `slot_of`'s `declared_positionally` arm. Resolving strictly by name
    // here bound nothing and the arm died at `codegen: unknown name reason`
    // ([TYPE-UNION-POSITIONAL]).
    let ir = module(
        "type Verdict = Pass | Fail(string)\n\
             fn why(v: Verdict) -> string = match v {\n\
               Pass => \"ok\"\n\
               Fail { reason } => reason\n\
             }\n\
             fn main() -> Unit = print(why(Fail(\"bad\")))\n",
    );
    // Slot 0 is payload field 1; loading it as an i8* is the only way the
    // binder could have resolved to a column rather than to a name.
    let body = function_body(&ir, "define i8* @why");
    assert!(
        body.contains("load i8*, i8**"),
        "the column-0 binder must load the positional payload slot:\n{body}"
    );
}

#[test]
fn single_variant_record_destructures_in_match() {
    // A single-variant type whose sole variant shares the type's name is
    // classified as a record, so it never lands in `union_variants`.
    // Matching it with a constructor pattern must still destructure its
    // fields: the `{ i64 tag, fields… }` block is identical to a union
    // variant's, carrying tag 0. Regression for #175 — this routed to
    // `gen_literal_match`, which rejected the constructor arm
    // ("unsupported construct: destructuring match arm").
    let ir = module(
        "type V = V { a: int, b: int }\n\
             fn first(v) = match v { V { a, b } => a }\n\
             print(\"r=${first(V { a: 10, b: 20 })}\")\n",
    );
    // The field bind loads slot 1 of the block, reached via the tag compare.
    shows(&ir, &["load i64, i64*", "icmp eq i64"]);
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
    shows(
        &ir,
        &[
            "osprey_list_length",
            "osprey_list_get",
            "osprey_list_drop",
            "icmp sge i64",
        ],
    );
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
