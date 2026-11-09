//! Expression inference tests: records, calls, indexing and arithmetic.

use crate::check::check_program;
use crate::testutil::{bad, bad_with, bind, ok};
use crate::{infer_program, Type};
use osprey_syntax::parse_program;

#[test]
fn record_update_on_record_and_field_assign() {
    ok("type Point = { x: int, y: int }\n\
        let p = Point { x: 1, y: 2 }\n\
        let q = p { x: 10 }\n");
}

#[test]
fn pipe_and_update_ast_nodes() {
    // The parser desugars `|>` into a `Call` and record-update `r { f }` into a
    // `TypeConstructor`, so `Expr::Pipe`/`Expr::Update` are built directly.
    use osprey_ast::{Expr, FieldAssignment, Parameter, Program, Stmt, TypeExpr};
    let inc = Stmt::Function {
        name: "inc".into(),
        type_params: Vec::new(),
        parameters: vec![Parameter {
            name: "n".into(),
            ty: Some(TypeExpr::named("int")),
            inline_constraint: false,
        }],
        return_type: Some(TypeExpr::named("int")),
        body: Expr::Identifier("n".into()),
        effects: Vec::new(),
        effect_tail: None,
        effect_row_present: false,
        doc: None,
        position: None,
    };
    // Pipe, non-call form: `10 |> inc` applies `inc(10)`.
    let bare_pipe = bind(
        "piped",
        Expr::Pipe {
            left: Box::new(Expr::Integer(10)),
            right: Box::new(Expr::Identifier("inc".into())),
        },
    );
    // Pipe, call form: `10 |> inc(0)` prepends `10`, becoming `inc(10, 0)`
    // (an arity mismatch — but the call-form branch is what we exercise).
    let call_pipe = bind(
        "call_piped",
        Expr::Pipe {
            left: Box::new(Expr::Integer(10)),
            right: Box::new(Expr::Call {
                function: Box::new(Expr::Identifier("inc".into())),
                arguments: vec![Expr::Integer(0)],
                named_arguments: Vec::new(),
            }),
        },
    );
    // `Expr::Update` over a non-record binding is a field write on an
    // `int`, rejected like the read would be ([TYPE-RECORD-UPDATE]); it
    // used to pass silently and reach codegen.
    let update = bind(
        "updated",
        Expr::Update {
            record: "n".into(),
            fields: vec![FieldAssignment {
                name: "x".into(),
                value: Expr::Integer(1),
            }],
        },
    );
    let prog = Program {
        statements: vec![
            inc,
            bind("n", Expr::Integer(2)),
            bare_pipe,
            call_pipe,
            update,
        ],
        doc: None,
    };
    // Only the deliberate pipe arity mismatch and the update are expected.
    let errs = check_program(&prog);
    assert!(
        errs.iter().all(|e| e.message.contains("arity")
            || e.message.contains("field 'x' on non-struct type int")),
        "unexpected errors: {errs:?}"
    );
    assert!(
        errs.iter()
            .any(|e| e.message.contains("non-struct type int")),
        "an update on an int must be rejected: {errs:?}"
    );
}

#[test]
fn record_update_is_checked_in_every_direction() {
    // [TYPE-RECORD-UPDATE] The field must exist, the value must fit, the
    // base must be a record, and a generic base defers all three to the
    // call that fixes it — each used to pass silently.
    const DECLS: &str = "type Point = { x: int, y: int }\n\
                         type Label = { v: string }\n\
                         type Box<T> = { held: T }\n\
                         type Shape = Circle { r: int } | Square { s: int }\n\
                         let p = Point { x: 1, y: 2 }\n";
    bad_with(
        &format!("{DECLS}let q = p {{ z: 5 }}\n"),
        "record type `Point` has no field `z`",
    );
    bad_with(
        &format!("{DECLS}let q = p {{ x: \"s\" }}\n"),
        "cannot unify int with string",
    );
    bad_with(
        &format!("{DECLS}fn bump(r) = r {{ v: 9 }}\nlet q = bump(Label {{ v: \"a\" }})\n"),
        "cannot unify string with int",
    );
    bad_with(
        &format!("{DECLS}fn setZ(r) = r {{ z: 1 }}\nlet q = setZ(p)\n"),
        "record type `Point` has no field `z`",
    );
    bad_with(
        &format!("{DECLS}let b = Box {{ held: 1 }}\nlet q = b {{ held: \"s\" }}\n"),
        "cannot unify int with string",
    );
    bad_with(
        &format!("{DECLS}let n = 2\nlet q = n {{ x: 1 }}\n"),
        "cannot access field 'x' on non-struct type int",
    );
    bad_with(
        &format!("{DECLS}let s = Circle {{ r: 1 }}\nlet q = s {{ r: 2 }}\n"),
        "cannot access field 'r' on non-struct type Shape",
    );
    bad_with(
        &format!("{DECLS}fn f(x: any) = x {{ v: 1 }}\n"),
        "cannot update field `v` on an erased `any`",
    );
}

#[test]
fn record_update_through_a_generic_function_keeps_assignment_rules() {
    // [TYPE-RECORD-UPDATE] A deferred write is resolved as an ASSIGNMENT
    // into the field, so the one-way rules survive the deferral: a bare
    // value fills a `Result` slot (implicit `Success`), anything erases
    // into `any`, and two unrelated records share one update function.
    ok("type Outcome = { r: Result<int, string> }\n\
        fn settle(o) = o { r: 1 }\n\
        let settled = settle(Outcome { r: 2 })\n");
    ok("type Cell = { v: any }\n\
        fn erase(c) = c { v: 1 }\n\
        let erased = erase(Cell { v: \"s\" })\n");
    ok("type Point = { x: int, y: int }\n\
        type Tag = { x: string }\n\
        fn setX(r, v) = r { x: v }\n\
        let moved = setX(Point { x: 1, y: 2 }, 7)\n\
        let retagged = setX(Tag { x: \"a\" }, \"b\")\n\
        let shown = \"${moved.x} ${retagged.x}\"\n");
    bad_with(
        "type Point = { x: int, y: int }\n\
         fn setX(r, v) = r { x: v }\n\
         let fine = setX(Point { x: 1, y: 2 }, 3)\n\
         let wrong = setX(Point { x: 1, y: 2 }, \"s\")\n",
        "cannot unify int with string",
    );
}

#[test]
fn a_field_assigned_twice_is_rejected_in_updates_and_constructions() {
    // [TYPE-RECORD-UPDATE] Of two assignments to one slot only one can
    // land, and which one was an accident of lowering (the update kept the
    // FIRST, so `p { x: 5, x: 6 }` printed 5).
    bad_with(
        "type Point = { x: int, y: int }\n\
         let p = Point { x: 1, y: 2 }\n\
         let q = p { x: 5, x: 6 }\n",
        "record update assigns field `x` more than once",
    );
    bad_with(
        "type Point = { x: int, y: int }\n\
         let p = Point { x: 1, x: 2, y: 3 }\n",
        "constructor `Point` assigns field `x` more than once",
    );
}

#[test]
fn nullary_record_constructor_used_as_value() {
    // `type Foo = Foo` is a single-variant record whose constructor has no
    // fields: the bare name `Foo` is a record value (the empty-record arm).
    ok("type Foo = Foo\n\
        let x = Foo\n");
}

#[test]
fn method_call_with_positional_and_named_args() {
    // The parser lowers `t.m(..)` to a plain `Call`, so an `Expr::MethodCall`
    // is built directly to exercise `infer_method_call`'s arg loops.
    use osprey_ast::{Expr, NamedArgument, Parameter, Program, Stmt, TypeExpr};
    let int_param = |name: &str| Parameter {
        name: name.into(),
        ty: Some(TypeExpr::named("int")),
        inline_constraint: false,
    };
    let body = Expr::MethodCall {
        target: Box::new(Expr::Integer(1)),
        method: "combine".into(),
        arguments: vec![Expr::Integer(2)],
        named_arguments: vec![NamedArgument {
            name: "third".into(),
            value: Expr::Integer(3),
        }],
    };
    let prog = Program {
        statements: vec![Stmt::Function {
            name: "combine".into(),
            type_params: Vec::new(),
            parameters: vec![int_param("self"), int_param("other"), int_param("third")],
            return_type: Some(TypeExpr::named("int")),
            body: Expr::Identifier("self".into()),
            effects: Vec::new(),
            effect_tail: None,
            effect_row_present: false,
            doc: None,
            position: None,
        }],
        doc: None,
    };
    // The function's signature pass registers `combine`; the MethodCall is a
    // top-level binding whose initializer drives `infer_method_call`.
    let mut stmts = prog.statements;
    stmts.push(bind("called", body));
    let errs = check_program(&Program {
        statements: stmts,
        doc: None,
    });
    assert!(errs.is_empty(), "unexpected type errors: {errs:?}");
}

#[test]
fn named_args_reorder_and_fall_back() {
    // Reorder succeeds when every name matches a parameter.
    ok("fn mk(a: int, b: string) -> int = a\n\
        let r = mk(b: \"x\", a: 1)\n");
    // A named call to an unknown function still type-checks its args (the
    // fallback that maps the named args positionally).
    ok("let f = fn(a) => a\n\
        let r = f(a: 7)\n");
}

#[test]
fn index_string_and_unknown_target() {
    // String index yields Result<string, _>; an opaque target falls back to
    // a fresh Result.
    ok("fn ch(s: string) -> Result<string, Error> = s[0]\n\
        fn anyIdx(x) = x[0]\n");
}

#[test]
fn arith_list_map_concat_and_float_subtraction() {
    // `+` over lists and maps unifies operands.
    ok("let xs = [1, 2] + [3, 4]\n\
        fn fsub(a: float, b: float) -> float = a - b\n");
}

#[test]
fn map_literals_and_public_operations_reject_non_string_keys() {
    let literal_errors = bad("let m = { 1: \"one\" }\n");
    assert_ne!(literal_errors, Vec::<crate::error::TypeError>::new());

    let operation_errors = bad("let m = mapSet(Map(), 1, \"one\")\n");
    assert_ne!(operation_errors, Vec::<crate::error::TypeError>::new());
}

#[test]
fn natural_arithmetic_infers_plain_numbers() {
    let parsed = parse_program(
        "fn intCalc(a: int, b: int, c: int) = (a + b) * c - 1\n\
         fn floatCalc(a: float, b: float, c: float) = (a + b) * c - 1.0\n\
         fn mixedChain(a: int, b: int, c: float) = (a + b) + c\n\
         fn inferredIntAdd(a, b) = a + b\n\
         fn intNeg(n: int) = -n\n\
         fn floatNeg(n: float) = -n\n",
    );
    assert!(
        parsed.errors.is_empty(),
        "syntax errors: {:?}",
        parsed.errors
    );
    // [ARITH-EFFECT] No arithmetic type contains a `Result`.
    let types = infer_program(&parsed.program);
    assert_eq!(types.return_type("intCalc"), Some(&Type::int()));
    assert_eq!(types.return_type("floatCalc"), Some(&Type::float()));
    assert_eq!(types.return_type("mixedChain"), Some(&Type::float()));
    assert_eq!(types.return_type("inferredIntAdd"), Some(&Type::int()));
    assert_eq!(types.return_type("intNeg"), Some(&Type::int()));
    assert_eq!(types.return_type("floatNeg"), Some(&Type::float()));

    // Arithmetic never unwraps a `Result` operand.
    let errors = bad("fn wrong(r: Result<int, Error>) = r + 1\n");
    assert!(
        errors.iter().any(|e| e
            .message
            .contains("cannot unify int with Result<int, Error>")),
        "{errors:?}"
    );
}

#[test]
fn interpolation_preserves_the_complete_result() {
    ok("fn show(a: int, b: int) -> string = \"sum=${a + b}\"\n");
}

#[test]
fn perform_named_arguments_are_rejected_before_codegen() {
    // Codegen has no named-operation argument mapping; reject this source
    // form instead of silently dropping its value.
    // [EFFECTS-OP-TYPING]
    bad_with(
        "effect Logger { log: fn(string) -> Unit }\n\
         fn run() -> Unit !Logger = perform Logger.log(msg: \"hi\")\n",
        "does not support named arguments",
    );
}

#[test]
fn nullary_union_variant_used_as_value() {
    // A bare nullary *union* variant (`Red`) is a value of its owner type —
    // the non-record `Type::con(owner, args)` arm of `lookup_ident`.
    ok("type Color = Red | Green | Blue\n\
        let c = Red\n");
}

#[test]
fn comparison_modulo_division_and_float_arith() {
    ok("fn lt(a: int, b: int) -> bool = a < b\n\
        fn md(a: int, b: int) = a % b\n\
        fn dv(a: int, b: int) = a / b\n\
        fn fadd(a: float, b: float) -> float = a + b\n\
        fn fmul(a: float, b: float) -> float = a * b\n");
}

#[test]
fn list_concat_when_only_right_is_a_list() {
    // `+` where the left operand starts unconstrained and the right is a
    // known list ties them and yields the list type (the r-side list arm).
    ok("fn cat(a, b: List<int>) = a + b\n");
}

#[test]
fn map_index_yields_value_result() {
    ok("fn lookup(m: Map<string, int>) -> Result<int, Error> = m[\"k\"]\n");
    // A key or index that is itself a `Result` is never silently unwrapped.
    assert_ne!(
        bad("fn bad(m: Map<string, int>) = m[checkedAdd(1, 1)]\n"),
        Vec::<crate::error::TypeError>::new()
    );
    assert_ne!(
        bad("fn bad(xs: List<int>) = xs[checkedAdd(1, 1)]\n"),
        Vec::<crate::error::TypeError>::new()
    );
}

#[test]
fn map_concatenation_unifies_operands() {
    ok("fn merge(a: Map<string, int>, b: Map<string, int>) -> Map<string, int> = a + b\n");
}

#[test]
fn calling_an_unannotated_param_constrains_it_to_a_function() {
    // `g` is an unannotated parameter (an unbound var); calling it drives the
    // `apply_fn` Var branch that synthesises a function shape.
    ok("fn apply(g, x) = g(x)\n");
}

#[test]
fn unknown_constructor_with_fields_is_an_error() {
    bad_with(
        "let r = Nonexistent { field: 1 }\n",
        "unknown constructor `Nonexistent`",
    );
}

#[test]
fn comparison_over_results_requires_explicit_handling() {
    let errs = bad("fn cmp(a: int, b: int) -> bool = checkedAdd(a, b) == checkedAdd(b, a)\n");
    assert!(errs.iter().any(|e| {
        e.message.contains("cannot compare a `Result` directly")
            && e.message.contains("match")
            && e.message.contains("?:")
    }));
    ok("fn cmp(a: int, b: int) -> bool = (checkedAdd(a, b) ?: 0) == (checkedAdd(b, a) ?: 0)\n");
}

#[test]
fn calling_a_non_identifier_and_a_non_function() {
    // Calling the result of a lambda expression directly: the callee is not a
    // bare identifier, so `infer_call` takes the `other` branch.
    ok("fn main() = {\n\
          handle Arith { overflow _ _ _ wrapped => wrapped }\n\
          let r = (fn(x) => x + 1)(41)\n\
        }\n");
    // Calling a non-function value is an error (`apply_fn` non-function arm).
    bad_with("let x = 5\nlet r = x(1)\n", "cannot call");
}

#[test]
fn lambda_with_param_and_return_annotations() {
    ok("let f = fn(x: int) -> int => x + 1\n\
        fn main() = {\n\
          handle Arith { overflow _ _ _ wrapped => wrapped }\n\
          let r = f(10)\n\
        }\n");
}

#[test]
fn lowercase_record_update_via_constructor_syntax() {
    // Every brace head takes the grammar's `type_constructor` shape; the
    // lowerer turns a lower-cased one into the `Expr::Update` checked here.
    ok("type Point = { x: int, y: int }\n\
        fn shift(p: Point) -> Point = p { x: 99 }\n");
}
