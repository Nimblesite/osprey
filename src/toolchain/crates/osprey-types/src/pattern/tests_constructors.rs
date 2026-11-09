//! Pattern tests: constructors, variants, results and exhaustiveness.

use crate::testutil::{bad_with, check, ok};

#[test]
fn positional_constructor_destructures_fields() {
    ok("type Wrap = Wrap { value: int }\n\
        fn unwrap(w: Wrap) -> int = match w {\n\
          Wrap(v) => v\n\
        }\n");
    // A payload slot binds one level deep. A constructor nested inside it
    // has no fall-through spelling, so the Default flavor refuses it here
    // exactly as the ML parser refuses it ([FLAVOR-ML-PATTERN-GROUP]).
    let errs = check(
        "type Wrap = Wrap { value: int }\n\
        type Box = Box { item: Wrap }\n\
        fn deep(b: Box) -> int = match b {\n\
          Box(Wrap(v)) => v\n\
        }\n",
    );
    assert!(
        errs.iter()
            .any(|e| e.message.contains("nested constructor patterns")),
        "{errs:?}"
    );
}

#[test]
fn record_constructor_pattern_unifies_owner() {
    // A `Ctor { fields }` pattern over a record type ties the discriminant to
    // the record owner type.
    ok("type Point = { x: int, y: int }\n\
        fn getx(p: Point) -> int = match p {\n\
          Point { x, y } => x\n\
        }\n");
}

#[test]
fn bare_result_variant_bindings_match_the_builtin() {
    // `Success`/`Error` as bare bindings over a real Result match the builtin
    // variants and bind nothing.
    ok("fn truthy(r: Result<int, Error>) -> int = match r {\n\
          Success => 1\n\
          Error => 0\n\
        }\n");
}

#[test]
fn nullary_union_variant_bindings_unify_and_are_exhaustive() {
    // No catch-all: each nullary binding is a variant, exercising both the
    // binding-unify path and exhaustiveness via `pattern_ctor_name`.
    ok("type Color = Red | Green | Blue\n\
        fn name(c: Color) -> string = match c {\n\
          Red => \"r\"\n\
          Green => \"g\"\n\
          Blue => \"b\"\n\
        }\n");
}

#[test]
fn non_exhaustive_union_reports_missing_variants() {
    let errs = check(
        "type Color = Red | Green | Blue\n\
         fn name(c: Color) -> string = match c {\n\
           Red => \"r\"\n\
           Green => \"g\"\n\
         }\n",
    );
    assert!(errs
        .iter()
        .any(|e| e.message.contains("non-exhaustive") && e.message.contains("Blue")));
}

/// `describe` matching `arms` over a union with a positional, a named and
/// a nullary variant.
fn describe_shape(arms: &str) -> String {
    format!(
        "type Shape = Circle(int) | Square {{ side: int }} | Dot\n\
         fn describe(s) = match s {{\n{arms}}}\n"
    )
}

// A bare variant name selects that variant whether or not it carries a
// payload — what ML's `Circle =>` and codegen's `pattern_ctor` already do.
// The checker read a bare *payload* variant as a variable binding, so the
// arm covered no variant, repeated none, and left the scrutinee untyped
// ([TYPE-MATCH-EXHAUSTIVE]).
#[test]
fn bare_payload_variant_arms_cover_their_variants() {
    ok(&describe_shape(
        "Circle => \"circle\"\nSquare => \"square\"\nDot => \"dot\"\n",
    ));
}

#[test]
fn bare_payload_variant_arm_leaves_the_others_missing() {
    // Accepted without a diagnostic, `describe(Dot)` runs off the last arm
    // into codegen's `unreachable`.
    bad_with(
        &describe_shape("Circle => \"circle\"\n"),
        "non-exhaustive match on `Shape`: missing Square, Dot",
    );
}

#[test]
fn bare_payload_variant_arm_repeated_is_unreachable() {
    bad_with(
        &describe_shape("Circle => \"circle\"\nCircle => \"again\"\n_ => \"other\"\n"),
        "unreachable match arm: variant `Circle` is already matched by an earlier arm",
    );
}

#[test]
fn bare_payload_variant_arm_ties_the_scrutinee_to_its_union() {
    // Accepted without a diagnostic, `describe(7)` reaches clang as
    // `bitcast i8* 7 to i64*`.
    let src =
        describe_shape("Circle => \"circle\"\n_ => \"other\"\n") + "let wrong = describe(7)\n";
    bad_with(&src, "type mismatch: cannot unify Shape with int");
}

#[test]
fn uppercase_binding_over_non_union_is_a_plain_binding() {
    // An uppercase name over an unconstrained (non-Con) discriminant is just a
    // catch-all binding — `unknown_variant_owner` returns None for non-Con.
    ok("fn f(v) = match v {\n\
          X => X\n\
        }\n");
    // An uppercase name that IS a real variant of the union is fine.
    ok("type Color = Red | Green\n\
        fn g(c: Color) -> int = match c {\n\
          Red => 1\n\
          Green => 2\n\
        }\n");
}

#[test]
fn result_pattern_with_an_unknown_field_binds_a_fresh_var() {
    // `Success { value, surplus }` binds the canonical `value` to the
    // success payload and a fresh var for a LATER, non-role field — the
    // first binder is the payload by role, whatever it is spelled
    // [FLAVOR-ML-MATCH].
    ok("fn f(r: Result<int, Error>) -> int = match r {\n\
          Success { value, surplus } => value\n\
          Error { message } => 0\n\
        }\n");
}

#[test]
fn renamed_result_binders_bind_the_payload_by_role() {
    // `Success { renamed }` binds the success payload under `renamed`,
    // exactly as `Success { value }` binds it under `value`, so a generic
    // callee reached through the renamed binder keeps its element type
    // (#283). A fresh-variable binding instead left the callee's `T`
    // unconstrained and codegen refused the closure as still generic.
    let parsed = osprey_syntax::parse_program_with_flavor(
        "type Evidence = { evidenceId: string }\n\
         type Section = { records: List<Evidence> }\n\
         type Focus = { picked: string }\n\
         fn find<T>(xs: List<T>, p: fn(T) -> bool) -> Result<T, Error> = match xs {\n\
             [] => Error { message: \"missing\" }\n\
             [head, ...tail] => match p(head) {\n\
                 true => Success { value: head }\n\
                 false => find(tail, p)\n\
             }\n\
         }\n\
         fn detail(focus: Focus) -> Result<Evidence, Error> =\n\
             match Success { value: Section { records: [Evidence { evidenceId: \"a\" }] } } {\n\
                 Error { message } => Error { message: \"no section\" }\n\
                 Success { section } => match find(section.records, fn(record) => record.evidenceId == focus.picked) {\n\
                     Success { record } => Success { value: record }\n\
                     Error { message } => Error { message: \"not found\" }\n\
                 }\n\
             }\n",
        osprey_syntax::Flavor::Default,
    );
    let types = crate::infer_program(&parsed.program);
    let lambdas: Vec<String> = types.lambdas.values().map(ToString::to_string).collect();
    assert!(!lambdas.is_empty(), "the closure survives inference");
    assert!(
        types.lambdas.values().all(|ty| !crate::has_type_var(ty)),
        "every residual lambda is concrete: {lambdas:?}"
    );
}

#[test]
fn unknown_constructor_pattern_is_an_error_but_binds_fields() {
    bad_with(
        "fn f(v) = match v {\n\
           Bogus { a, b } => a\n\
           _ => 0\n\
         }\n",
        "unknown constructor `Bogus`",
    );
}

#[test]
fn list_patterns_bind_head_tail_and_prefix() {
    ok("fn classify(xs) = match xs {\n\
          [] => \"empty\"\n\
          [single] => \"one\"\n\
          [head, ...tail] => \"many\"\n\
        }\n");
}

#[test]
fn structural_pattern_over_a_record_value_binds_field_types() {
    // Matching a record *value* (not a nominal annotation) makes the
    // discriminant a real `Type::Record`, so structural binding reads the
    // field's declared type.
    ok("type Point = { x: int, y: int }\n\
        fn sum() -> int = match Point { x: 1, y: 2 } {\n\
          { x, y } => x + y\n\
        }\n");
}

#[test]
fn type_annotated_pattern_binds_the_named_type() {
    ok("fn f(v) = match v {\n\
          n: Int => n\n\
          _ => 0\n\
        }\n");
}

#[test]
fn nullary_record_owner_pattern_unifies() {
    // `type Foo = Foo` is a nullary record constructor; matching it ties the
    // discriminant to the empty-record owner type (`ctor_owner_ty`'s
    // record arm).
    ok("type Foo = Foo\n\
        fn f(x: Foo) -> int = match x {\n\
          Foo => 1\n\
        }\n");
}

#[test]
fn result_owner_constructor_pattern_autowraps_payload() {
    // A `Success { value }` pattern over a non-Result discriminant auto-wraps
    // it as the success payload, and `Error { message }` binds a string.
    ok("fn f(n: int) -> int = match n {\n\
          Success { value } => value\n\
          Error { message } => 0\n\
        }\n");
}

#[test]
fn result_is_not_a_boolean_condition_and_bool_matches_stay_exhaustive() {
    let condition_errors = check("fn f(r: Result<int, Error>) -> int = r ? 1 : 0\n");
    assert!(condition_errors
        .iter()
        .any(|e| e.message.contains("Result") && e.message.contains("bool")));

    bad_with("let x = match true { true => 1 }\n", "non-exhaustive");
}

#[test]
fn bare_result_annotation_uses_the_no_args_unwrap_fallback() {
    // A bare `Result` annotation (no type args) is still a Result; matching a
    // A literal cannot directly match a Result wrapper.
    bad_with(
        "fn f(r: Result) -> int = match r {\n\
           0 => 0\n\
           _ => 1\n\
         }\n",
        "Result",
    );
}

#[test]
fn lowercase_binding_is_a_legal_catch_all() {
    // A genuine lower-case variable binding still absorbs the remaining
    // variants, keeping a partial union match legal.
    ok("type Color = Red | Green | Blue\n\
        fn name(c: Color) -> string = match c {\n\
          Red => \"r\"\n\
          other => \"?\"\n\
        }\n");
}

#[test]
fn misspelled_uppercase_variant_is_not_a_catch_all() {
    // `Bleu` is not a `Color` variant: it must be reported rather than
    // silently absorbing the missing variants as a catch-all.
    bad_with(
        "type Color = Red | Green | Blue\n\
         fn name(c: Color) -> string = match c {\n\
           Red => \"r\"\n\
           Bleu => \"?\"\n\
         }\n",
        "Bleu",
    );
}

#[test]
fn arm_after_a_catch_all_is_unreachable() {
    bad_with(
        "type Color = Red | Green | Blue\n\
         fn name(c: Color) -> string = match c {\n\
           Red => \"r\"\n\
           _ => \"?\"\n\
           Green => \"g\"\n\
         }\n",
        "unreachable",
    );
}

#[test]
fn duplicate_variant_arm_is_unreachable() {
    let errs = check(
        "type Color = Red | Green | Blue\n\
         fn name(c: Color) -> string = match c {\n\
           Red => \"r\"\n\
           Green => \"g\"\n\
           Red => \"r2\"\n\
           Blue => \"b\"\n\
         }\n",
    );
    assert!(
        errs.iter()
            .any(|e| e.message.contains("unreachable") && e.message.contains("Red")),
        "expected a duplicate-variant unreachable error: {errs:?}"
    );
}
