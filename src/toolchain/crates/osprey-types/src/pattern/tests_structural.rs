//! Pattern tests: structural rows, erased scrutinees and tuples.

use crate::testutil::{bad_with, check, ok};

#[test]
fn structural_pattern_binds_record_fields() {
    ok("type Point = { x: int, y: int }\n\
        fn getx(p: Point) -> int = match p {\n\
          { x, y } => x + y\n\
        }\n");
    // An UNRESOLVED scrutinee refuses a structural arm: the old contract
    // bound its fields as fresh variables, which let the body read each
    // field at whatever type it pleased — recovery through a pattern,
    // with no annotation to reject ([TYPE-ANY]).
    let errs = check("fn f(v) = match v {\n  { a, b } => 0\n  _ => 1\n}\n");
    assert!(
        errs.iter()
            .any(|e| e.message.contains("needs a record or `any` scrutinee")),
        "an unresolved scrutinee must not bind structural fields: {errs:?}"
    );
}

/// The refusal names the type it FOUND when there is one to name. An
/// unresolved scrutinee has no user-facing spelling and gets the annotation
/// hint instead, but a concrete non-row scrutinee must say what it is —
/// otherwise the message reads the same for "annotate this" and "this can
/// never be a row", which are different mistakes.
#[test]
fn a_non_row_scrutinee_is_named_in_the_refusal() {
    for (src, want) in [
        (
            "fn f(v: int) = match v {\n  { a } => 0\n  _ => 1\n}\n",
            "found int",
        ),
        (
            "fn f(v: string) = match v {\n  { a } => 0\n  _ => 1\n}\n",
            "found string",
        ),
        // A function value is neither a record nor a named constructor, so
        // it leaves `structural_row` by its non-`Con` exit.
        (
            "fn g(x) = x\nlet r = match g {\n  { a } => 0\n  _ => 1\n}\n",
            "needs a record or `any` scrutinee",
        ),
    ] {
        bad_with(src, want);
    }
}

/// Binder collection over a structural pattern feeds reachability and free
/// variable analysis. A tuple slot spelled `_` carries an EMPTY binder and
/// must not be collected as a name — a `""` binding would shadow nothing
/// and be impossible to reference.
#[test]
fn structural_binder_names_skip_unbound_slots() {
    use osprey_ast::Pattern;
    let named = Pattern::Structural {
        fields: vec![
            (String::from("x"), String::from("x")),
            (String::from("y"), String::from("y")),
        ],
        open: false,
    };
    assert_eq!(super::pattern_binder_names(&named), vec!["x", "y"]);
    let tuple = osprey_ast::tuple_pattern(vec![String::from("n"), String::new()]);
    assert_eq!(super::pattern_binder_names(&tuple), vec!["n"]);
}

#[test]
fn structural_fields_of_an_erased_scrutinee_are_erased_themselves() {
    // Over `any` the row is unknown until run time, so a bound field is
    // itself `any` — reading it with an operator is the erasure read the
    // checker deletes everywhere else.
    let errs = check(
        "fn f(v: any) -> int = match v {\n\
           { n } => n + 1\n\
           _ => 0\n\
         }\n",
    );
    assert!(
        errs.iter()
            .any(|e| e.message.contains("cannot apply `+` to an erased `any`")),
        "a narrowed field must stay erased until matched further: {errs:?}"
    );
}

#[test]
fn a_match_over_any_requires_a_catch_all() {
    let errs = check(
        "fn f(v: any) -> int = match v {\n\
           { n } => 1\n\
         }\n",
    );
    assert!(
        errs.iter()
            .any(|e| e.message.contains("a match over `any` is never exhaustive")),
        "{errs:?}"
    );
}

#[test]
fn an_open_row_arm_shadows_every_extension_after_it() {
    let errs = check(
        "fn f(v: any) -> int = match v {\n\
           { x, .. } => 1\n\
           { x, y } => 2\n\
           _ => 0\n\
         }\n",
    );
    assert!(
        errs.iter().any(|e| e
            .message
            .contains("`{ x, .. }` arm already covers this row")),
        "{errs:?}"
    );
    // Closed rows shadow only their exact repetition; reordering the
    // spelling does not make it a different row.
    let errs = check(
        "fn f(v: any) -> int = match v {\n\
           { x, y } => 1\n\
           { y, x } => 2\n\
           _ => 0\n\
         }\n",
    );
    assert!(
        errs.iter()
            .any(|e| e.message.contains("already matched by an earlier arm")),
        "{errs:?}"
    );
}

#[test]
fn a_closed_structural_pattern_must_name_the_whole_row() {
    let errs = check(
        "type Point = { x: int, y: int }\n\
         fn f(p: Point) -> int = match p {\n\
           { x } => x\n\
         }\n",
    );
    assert!(
        errs.iter()
            .any(|e| e.message.contains("must name the whole row") && e.message.contains('y')),
        "{errs:?}"
    );
    // Opening the row with `..` selects the wider record.
    ok("type Point = { x: int, y: int }\n\
        fn f(p: Point) -> int = match p {\n\
          { x, .. } => x\n\
        }\n");
    // Naming a field the row lacks can never select.
    bad_with(
        "type Point = { x: int, y: int }\n\
         fn f(p: Point) -> int = match p {\n\
           { z, .. } => z\n\
         }\n",
        "has no such field",
    );
}

#[test]
fn tuple_patterns_bind_positional_record_slots() {
    // `(n, s)` is the positional spelling of `{ 0, 1 }`; over a
    // positionally-declared record each binder takes its slot's declared
    // type ([PATTERN-TUPLE]).
    ok("type Pair = Pair(int, string)\n\
        fn f(p: Pair) -> int = match p {\n\
          (n, s) => n + length(s)\n\
        }\n");
}

#[test]
fn shape_reading_patterns_are_rejected_over_an_erased_scrutinee() {
    // Each of these arms reads the erased word with no runtime test — the
    // annotation-pattern spelling compiled before this rule and read a
    // heap address exactly as finding B did ([TYPE-ANY]).
    for (arm, kind) in [
        ("0 => 1", "a literal pattern"),
        ("s: string => 2", "a type-annotated binding"),
        ("[a, b] => 3", "a list pattern"),
    ] {
        let errs = check(&format!(
            "fn f(v: any) -> int = match v {{\n  {arm}\n  _ => 0\n}}\n"
        ));
        assert!(
            errs.iter()
                .any(|e| e.message.contains(kind) && e.message.contains("cannot read")),
            "expected `{kind}` rejection for `{arm}`: {errs:?}"
        );
    }
    let errs = check(
        "type Color = Red | Green\n\
         fn f(v: any) -> int = match v {\n\
           Red => 1\n\
           _ => 0\n\
         }\n",
    );
    assert!(
        errs.iter().any(|e| e
            .message
            .contains("a variant pattern cannot read an erased `any`")),
        "{errs:?}"
    );
}
