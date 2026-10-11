use super::*;

#[test]
fn a_resumable_operation_sends_every_argument_with_its_ownership_kind() {
    // The operation mailbox is length-carrying and kind-tagged
    // ([EFFECTS-OPERATION-MAILBOX]). Nothing else in Rust pins this ABI —
    // a drift used to surface only as a C link error or, worse, as
    // `osp_release` called on an integer. Seventeen slots is the arity a
    // fixed sixteen-word mailbox silently zeroed (#182); the `string` slot
    // is the one whose reference the mailbox owns (#185).
    let ir = module(
            "effect Wide { control op: fn(int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, string) -> int }\n\
             fn body() -> int !Wide = perform Wide.op(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, \"x\")\n\
             fn main() -> int { let r = {\n\
                 handle Wide {\n\
                     op a b c d e f g h i j k l m n o p q => resume(p)\n\
                 }\n\
                 body()\n\
             }\n\
               print(\"r=${toString(r)}\")\n\
               0 }\n",
        );
    assert!(
        ir.contains("declare i64 @__osprey_coro_suspend(i8*, i64, i64*, i8*, i64)"),
        "suspend must take a kinds array beside the words:\n{ir}"
    );
    // Both arrays are sized by the REAL arity — never a fixed capacity.
    assert!(
        ir.contains("alloca [17 x i64]") && ir.contains("alloca [17 x i8]"),
        "the mailbox must be sized by the operation's declared arity:\n{ir}"
    );
    // Scalar slots tag 0; the trailing `string` tags 1, and only that one is
    // a reference the mailbox releases when it retires.
    assert!(
        ir.contains("store i8 1, i8*") && ir.contains("store i8 0, i8*"),
        "each slot must carry its operand kind:\n{ir}"
    );
    // The dispatcher takes the mailbox, reads through it, and retires it.
    for symbol in [
        "@__osprey_coro_take_args",
        "@__osprey_coro_mail_op",
        "@__osprey_coro_mail_arg",
        "@__osprey_coro_mail_free",
    ] {
        assert!(ir.contains(symbol), "missing {symbol} in:\n{ir}");
    }
    // The superseded fixed-width accessors must not come back.
    assert!(
        !ir.contains("@__osprey_coro_arg(") && !ir.contains("@__osprey_coro_op("),
        "the fixed-width mailbox accessors are gone:\n{ir}"
    );
}

#[test]
fn handler_owned_mutable_state_threads_through_a_heap_cell() {
    // A `mut` an effect handler arm captures is promoted to a shared heap
    // cell: the env-carrying handler ABI passes the cell pointer, `get`
    // loads it and `set` stores it, so `perform` threads real state.
    let ir = module(
            "effect State { get: fn() -> int  set: fn(int) -> Unit }\n\
             fn bump() -> int !State = { let a = perform State.get()  perform State.set(satAdd(a, 1))  perform State.get() }\n\
             fn main() -> int { mut c = 0\n\
               let r = {\n\
                   handle State {\n\
                       get => c set v => { c = v }\n\
                   }\n\
                   bump()\n\
               }\n\
               print(\"r=${toString(r)} c=${toString(c)}\")\n\
               0 }\n",
        );
    // env-carrying handler ABI (push takes an i8* env; perform resolves it)
    shows(
        &ir,
        &[
            "declare i32 @__osprey_handler_push_scoped(i32, i8*, i8*, i32)",
            "@__osprey_handler_lookup_env",
        ],
    );
    // the captured `mut` became a heap cell (malloc'd, stored, loaded)
    assert!(ir.contains("@osp_alloc"));
    // each arm is emitted with the hidden leading env parameter
    assert!(ir.contains("i8* %__env"));
}

#[test]
fn handler_rebound_function_cell_calls_latest_closure_indirectly() {
    let ir = module(
        "effect ClosureSlot { rebind: fn(int) -> Unit }\n\
             fn makeAdder(n: int) -> (int) -> int = fn(x) => satAdd(x, n)\n\
             fn main() -> int {\n\
             mut rb = fn(x) => satAdd(x, 1)\n\
             handle ClosureSlot {\n\
                 rebind offset => { rb = makeAdder(offset) }\n\
             }\n\
             perform ClosureSlot.rebind(40)\n\
             print(toString(rb(2)))\n\
             0\n\
             }\n",
    );

    let push = ir
        .lines()
        .find(|line| line.contains("call i32 @__osprey_handler_push"))
        .expect("handler push");
    assert!(
        !push.contains("i8* null"),
        "the assignment target must be captured as a shared function cell:\n{ir}"
    );
    assert!(
            !ir.contains("@rb"),
            "a handler-rebound function cell must load and call its latest closure, not emit a direct call to an undefined/stale `rb` symbol:\n{ir}"
        );
}

#[test]
fn fiber_await_unboxes_a_string_result_to_a_pointer() {
    // `await(spawn e)` recovers the fiber's element type: a string result is
    // a pointer, recovered with `inttoptr`, not kept as a raw integer.
    let ir = module(
        "fn greet(n: string) -> string = \"hi ${n}\"\n\
             fn main() -> Unit = print(await(spawn greet(\"x\")))\n",
    );
    shows(&ir, &["fiber_await", "inttoptr i64"]);
    assert!(
        ir.lines()
            .any(|line| line.contains("@fiber_spawn_env_owned") && line.ends_with(", i64 1)")),
        "a managed string fiber result must carry its ownership bit:\n{ir}"
    );
}

#[test]
fn fiber_result_shape_survives_named_and_closure_function_boundaries() {
    fn result_pointer_unboxes(ir: &str) -> usize {
        ir.lines()
            .collect::<Vec<_>>()
            .windows(2)
            .filter(|pair| {
                matches!(pair, [first, second]
                    if first.contains("inttoptr i64")
                        && first.contains("to i8*")
                        && second.contains("bitcast i8*")
                        && second.contains("to { i64, i8, i8* }*"))
            })
            .count()
    }

    // `checkedAdd` is the Result-producing arithmetic ([ARITH-EFFECT]).
    let named = module(
        "fn start(n: int) -> Fiber<Result<int, Error>> = spawn(checkedAdd(n, 1))\n\
             fn finish(f: Fiber<Result<int, Error>>) -> Result<int, Error> = await(f)\n\
             let through_return = await(start(9223372036854775807))\n\
             let through_parameter = finish(spawn(checkedAdd(9223372036854775807, 1)))\n",
    );
    assert_eq!(
        result_pointer_unboxes(&named),
        2,
        "named Fiber<Result> boundaries must recover the complete Result block:\n{named}"
    );

    let closure = module(
        "let start = fn(n: int) -> Fiber<Result<int, Error>> => spawn(checkedAdd(n, 1))\n\
             let finish = fn(f: Fiber<Result<int, Error>>) -> Result<int, Error> => await(f)\n\
             let through_return = await(start(9223372036854775807))\n\
             let through_parameter = finish(spawn(checkedAdd(9223372036854775807, 1)))\n",
    );
    assert_eq!(
        result_pointer_unboxes(&closure),
        2,
        "closure Fiber<Result> boundaries must recover the complete Result block:\n{closure}"
    );
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
    shows(&ir, &["sitofp i64", "fdiv double", "fneg double", "fcmp"]);
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
    shows(&ir, &["and i1", "or i1", "xor i1"]);
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
    shows(&ir, &["getelementptr", "store i64"]);
}

#[test]
fn generic_record_update_rebuilds_the_instantiation_layout() {
    // [TYPE-RECORD-UPDATE] A generic record's handle names its
    // INSTANTIATION (`Box#i64`), which `ctor_layout` has never heard of:
    // every update of one — direct or through a generic function — died
    // with `codegen: unknown name `Box#i64``. `gen_update` now resolves the
    // registered layout, as field access always has.
    let ir = module(
        "type Box<T> = { held: T }\n\
             fn rebox(b) = b { held: 5 }\n\
             fn swap(b, v) = b { held: v }\n\
             fn main() -> Unit = {\n\
               let boxed = Box { held: 1 }\n\
               let changed = boxed { held: 2 }\n\
               let swapped = swap(Box { held: \"a\" }, \"b\")\n\
               print(\"${changed.held} ${rebox(boxed).held} ${swapped.held}\")\n\
             }\n",
    );
    shows(&ir, &["getelementptr", "store i64", "store i8*"]);
}

#[test]
fn record_update_of_a_handler_promoted_cell_reads_the_cell() {
    // [TYPE-RECORD-UPDATE] [EFFECTS-HANDLER-STATE] A `mut` record a
    // handler arm rebinds is promoted to a shared cell, which a scope
    // lookup cannot see: `acc = acc { … }` inside the arm and `acc { … }`
    // after the region both died with `codegen: unknown name `acc``, while
    // `acc = Point { … }` beside them compiled. The update now reads its
    // base exactly as the identifier `acc` is read.
    let ir = module(
        "type Point = { x: int, y: int }\n\
             effect Steer { nudge : fn(int) -> int }\n\
             fn steered() = {\n\
               mut acc = Point { x: 0, y: 0 }\n\
               handle Steer {\n\
                 nudge by => {\n\
                   acc = acc { x: by, y: satAdd(acc.y, by) }\n\
                   acc.y\n\
                 }\n\
               }\n\
               let first = perform Steer.nudge(5)\n\
               acc { x: first }\n\
             }\n\
             fn main() -> Unit = {\n\
               let s = steered()\n\
               print(\"${s.x} ${s.y}\")\n\
             }\n",
    );
    shows(&ir, &["getelementptr", "store i64"]);
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
fn fibers_channels_yield_and_done() {
    // spawn/await (covered elsewhere) plus Channel/send/recv, yield with and
    // without a value, fiber_yield and fiberDone.
    let ir = module(
        "fn work(n: int) -> int = satAdd(n, 1)\n\
             fn main() -> Unit = {\n\
               let ch = Channel(1)\n\
               send(ch, 42)\n\
               let got = recv(ch)\n\
               let y = yield 5\n\
               let z = fiber_yield(9)\n\
               let f = spawn work(3)\n\
               print(\"${got} ${y} ${z} ${await(f)} ${fiberDone(f)}\")\n\
             }\n",
    );
    shows(
        &ir,
        &[
            "channel_create",
            "channel_send",
            "channel_recv",
            "fiber_done",
        ],
    );
}

#[test]
fn direct_ast_select_is_rejected_instead_of_choosing_the_first_arm() {
    // [CONCURRENCY-SELECT-REJECT]: the type checker rejects `select` first, so
    // codegen sees one only when an AST is compiled directly. It used to lower
    // the FIRST arm's body and return it as the select's value — a wrong answer
    // that looked right. Plan 0007's acceptance bar is that this path cannot
    // silently pick an arm.
    let err = compile_err(
        "fn main() -> Unit = {\n\
               let pick = select { 1 => 100  2 => 200 }\n\
               print(\"${pick}\")\n\
             }\n",
    );
    assert!(
        err.to_string().contains("`select` is not supported"),
        "expected the select rejection, got {err}"
    );
}

#[test]
fn direct_ast_assignment_to_an_unbound_name_is_rejected_instead_of_binding_a_local() {
    // The type checker rejects rebinding an unknown name first, so codegen
    // sees one only when an AST is compiled directly. It used to bind a
    // fresh local nobody reads, so the write vanished without a diagnostic.
    let err = compile_err(
        "fn main() -> Unit = {\n\
               ghost = 1\n\
               print(\"done\")\n\
             }\n",
    );
    assert!(
        matches!(&err, CodegenError::UnknownName(name) if name == "ghost"),
        "expected the unknown-name rejection, got {err}"
    );
}

#[test]
fn an_unexpanded_opaque_alias_is_refused_instead_of_lowered_as_a_handle() {
    // [MODULES-OPAQUE-TYPES]: the checker reads a project in which an
    // opaque alias keeps its name; the backend must be handed the copy
    // with every alias expanded. Lowering the checked program would treat
    // the `int` behind `Token` as a heap handle, so it is refused.
    let mut program = parse_program("type Token = int\nprint(\"x\")\n").program;
    for statement in &mut program.statements {
        if let osprey_ast::Stmt::Type { opaque, .. } = statement {
            *opaque = true;
        }
    }
    let err = compile_program(&program).unwrap_err();
    assert!(
        err.to_string()
            .contains("opaque alias `Token` reached code generation unexpanded"),
        "expected the unexpanded-alias refusal, got {err}"
    );
}

#[test]
fn match_arms_of_different_physical_types_are_rejected_not_unitised() {
    // The other loud-failure branch with no surface syntax of its own: the
    // checker rejects mismatched arms (`cannot unify int with string`), so
    // `finish_phi` only meets them when an AST reaches codegen directly.
    // It used to return `Value::unit()` there, turning a whole class of
    // type errors into silently-unit expressions; it must error instead,
    // except where the value is genuinely discarded.
    let err = compile_err(
        "fn pick(n: int) -> int = match n {\n\
               1 => 1\n\
               _ => \"two\"\n\
             }\n\
             fn main() -> Unit = print(\"${pick(1)}\")\n",
    );
    assert!(
        err.to_string().contains("match arms disagree on type"),
        "expected the arm-type mismatch, got {err}"
    );
}
