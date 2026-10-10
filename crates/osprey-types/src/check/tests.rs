use super::infer_program;
use crate::check::check_program;
use crate::error::TypeError;
use crate::testutil::{bad_with, check, ok};
use osprey_ast::{Expr, Stmt};
use osprey_syntax::parse_program;

/// One generic effect over a generic RECORD, differing only in the
/// instantiation the handler answers at.
fn stash(handler_payload: &str) -> String {
    format!(
        "type Box<T> = {{ value: T }}\n\
         effect Stash<T> {{\n\
         put: fn(T) -> Unit\n\
         take: fn() -> T\n\
         }}\n\
         fn stash() = perform Stash.put(Box {{ value: 42 }})\n\
         fn main() -> Unit = {{\n\
         let cached = {{\n\
             handle Stash {{\n\
                 put v => print(\"put\")\n\
                 take => Box {{ value: {handler_payload} }}\n\
             }}\n\
             stash()\n\
         }}\n\
         print(\"${{cached}}\")\n\
         }}\n"
    )
}

#[test]
fn a_handler_discharges_a_generic_record_instance_only_at_its_own_instantiation() {
    // The effect-row site keys that match a perform to a handler are built
    // by RENDERING the argument type, so a rendering that collapsed a
    // declared record to its bare name made `Box<int>` and `Box<string>`
    // ONE key — and the mismatch below type-checked with zero errors.
    //
    // Both directions are asserted here because the negative alone is
    // one-sided: a checker that rejected every generic record instance
    // would satisfy it while breaking the language. The end-to-end
    // rejection lives in
    // `examples/failscompilation/generic_effect_record_arg_mismatch.ospo`.
    let matching = check(&stash("0"));
    assert!(
        matching.is_empty(),
        "a handler at the SAME instance must discharge: {matching:?}"
    );

    let mismatched = check(&stash("\"cached text\""));
    assert!(
        !mismatched.is_empty(),
        "a handler at `Stash<Box<string>>` must not discharge a \
         `Stash<Box<int>>` operation"
    );
    // Exactly ONE error, naming the whole operation at its instantiation.
    // `!is_empty()` plus a field-name substring would also be satisfied by
    // an unrelated failure — a parse slip, a second spurious diagnostic —
    // so the mismatch would stop being what the test proves.
    let [only] = mismatched.as_slice() else {
        panic!("the instantiation mismatch is the ONLY thing wrong here: {mismatched:?}")
    };
    assert_eq!(
        only.message,
        "unhandled effect operations at program entry: \
         Stash<{ value: int }>.put; add a matching `handle`",
        "the diagnostic must name the operation at its instantiation \
         structurally, not collapse it to `Box`"
    );
}

#[test]
fn module_bodies_are_checked_in_a_child_scope() {
    let errs = check(
        "module Math {\n\
           fn square(x: int) -> int = x * x\n\
         }\n",
    );
    assert!(errs.is_empty(), "unexpected type errors: {errs:?}");
    // A type error inside a module body is still reported.
    let errs = check(
        "module Bad {\n\
           let y: int = \"not an int\"\n\
         }\n",
    );
    assert!(errs.iter().any(|e| e.message.contains("type mismatch")));
    // Module functions must run both declaration collection and body
    // checking; historically only module lets reached inference.
    bad_with(
        "module BadFn {\n\
           fn broken() -> int = \"not an int\"\n\
         }\n",
        "type mismatch",
    );
}

#[test]
fn a_discarded_result_statement_is_rejected() {
    // A bare statement whose value is a `Result` throws the failure away —
    // the one place the wrapper can vanish without anyone naming it.
    bad_with(
        "fn risky(n: int) = checkedAdd(n, 1)\n\
         fn go() -> int = {\n\
           risky(1)\n\
           0\n\
         }\n",
        "cannot be discarded",
    );
    ok("fn risky(n: int) = checkedAdd(n, 1)\n\
        fn go() -> int = {\n\
          let handled = risky(1) ?: 0\n\
          handled\n\
        }\n");
}

/// The fixed prelude every discard case is wrapped in. The orphan
/// statement always lands on LINE 5, so the position assertion below can
/// demand the error point AT the discarded value rather than at the block.
fn in_block(orphan: &str) -> String {
    format!(
        "fn add(a, b) = a + b\n\
         fn double(x) = x * 2\n\
         type Point = {{ x: int, y: int }}\n\
         fn go() -> int = {{\n\
           {orphan}\n\
           0\n\
         }}\n"
    )
}

/// The orphan line of `in_block`.
const ORPHAN_LINE: u32 = 5;

/// Shapes whose value is computed and then THROWN AWAY. Not one of them
/// involves `Result`, which is the entire point: the guard in
/// `infer_block_stmt` tests `is_named(names::RESULT)`, so every shape here
/// is accepted in silence today.
const DISCARDED_PURE_VALUES: &[(&str, &str)] = &[
    ("int literal", "42"),
    ("string literal", "\"orphan\""),
    ("bool literal", "true"),
    ("float literal", "3.5"),
    ("list literal", "[1, 2, 3]"),
    ("comparison", "1 < 2"),
    ("function value", "add"),
    ("lambda", "|x| => x"),
    ("interpolated string", "\"n=${42}\""),
    ("record construction", "Point { x: 1, y: 2 }"),
    ("field access", "Point { x: 1, y: 2 }.x"),
    ("block value", "{ 1 }"),
];

/// The same defect reached through ML-style juxtaposition. Default has no
/// application production, so `add 2 3` is not a call — before
/// [LEX-STATEMENT-BREAK] it silently split into `add` plus two orphan
/// literals. Each entry is a WHOLE program because the split changes the
/// statement count, including at TOP LEVEL.
const JUXTAPOSITION_SPLITS: &[(&str, &str)] = &[
    (
        "two arguments",
        "fn add(a, b) = a + b\n\
         fn go() -> int = {\n\
           let r = add 2 3\n\
           0\n\
         }\n",
    ),
    (
        "one argument",
        "fn double(x) = x * 2\n\
         fn go() -> int = {\n\
           let r = double 5\n\
           0\n\
         }\n",
    ),
    (
        "bare statement, not a let",
        "fn add(a, b) = a + b\n\
         fn go() -> int = {\n\
           add 2 3\n\
           0\n\
         }\n",
    ),
    (
        "identifier arguments",
        "fn add(a, b) = a + b\n\
         fn go(a, b) -> int = {\n\
           let r = add a b\n\
           0\n\
         }\n",
    ),
    (
        "inside a lambda body",
        "fn double(x) = x * 2\n\
         fn go() -> int = {\n\
           let f = |n| => {\n\
             let r = double n\n\
             0\n\
           }\n\
           f(1)\n\
         }\n",
    ),
    (
        "inside a nested block",
        "fn add(a, b) = a + b\n\
         fn go() -> int = {\n\
           let inner = {\n\
             let r = add 2 3\n\
             0\n\
           }\n\
           inner\n\
         }\n",
    ),
    (
        "at top level, outside any block",
        "fn add(a, b) = a + b\n\
         let r = add 2 3\n",
    ),
];

/// Statements that discard NOTHING and must stay accepted. Without these
/// the rule above is satisfied by a checker that rejects every expression
/// statement, which would delete side effects from the language.
const DISCARDS_NOTHING: &[(&str, &str)] = &[
    (
        "Unit-valued effectful call",
        "fn go() -> int = {\n\
           print(\"effect\")\n\
           0\n\
         }\n",
    ),
    (
        "two Unit-valued calls in sequence",
        "fn go() -> int = {\n\
           print(\"a\")\n\
           print(\"b\")\n\
           0\n\
         }\n",
    ),
    (
        "the block's own tail expression",
        "fn go() -> int = {\n\
           let x = 1\n\
           x\n\
         }\n",
    ),
    (
        "a nested block's tail expression",
        "fn go() -> int = {\n\
           let inner = {\n\
             let y = 2\n\
             y\n\
           }\n\
           inner\n\
         }\n",
    ),
    (
        "a Result consumed with ?:",
        "fn risky(n: int) = checkedAdd(n, 1)\n\
         fn go() -> int = {\n\
           let handled = risky(1) ?: 0\n\
           handled\n\
         }\n",
    ),
];

fn discard_errors(src: &str) -> Vec<TypeError> {
    check(src)
        .into_iter()
        .filter(|e| e.message.contains("cannot be discarded"))
        .collect()
}

fn messages(src: &str) -> Vec<String> {
    check(src).into_iter().map(|e| e.message).collect()
}

#[test]
fn a_discarded_pure_value_statement_is_rejected() {
    // ROOT CAUSE. `infer_block_stmt` guards the discard with
    // `is_named(names::RESULT)`, so ONLY a discarded `Result` is an error;
    // every other discarded value is accepted in silence. The rule is not
    // "reject a discarded Result" — it is "a statement whose value is
    // thrown away must have had nothing to throw away".
    for (label, orphan) in DISCARDED_PURE_VALUES {
        let src = in_block(orphan);
        let found = discard_errors(&src);
        assert!(
            !found.is_empty(),
            "a discarded {label} must be rejected; all errors were {:?} for:\n{src}",
            messages(&src)
        );
        // A rejection with no source location is not a truthful error: it
        // cannot point the author at the value being thrown away.
        assert!(
            found
                .iter()
                .any(|e| e.position.map(|p| p.line) == Some(ORPHAN_LINE)),
            "the {label} rejection must point at the orphan on line \
             {ORPHAN_LINE}, got {:?}",
            found.iter().map(|e| e.position).collect::<Vec<_>>()
        );
    }
}

#[test]
fn juxtaposition_does_not_split_into_silently_discarded_statements() {
    // Before the fixes, `add 2 3` parsed as `let r = add` plus two orphan
    // literals, so the program compiled, `r` bound a function value, and
    // the binary printed a raw (ASLR-varying) pointer to stdout and
    // exited 0. Rejection at EITHER stage satisfies this: a parse error
    // from [LEX-STATEMENT-BREAK] is the primary outcome, and a discard
    // error from [BLOCK-DISCARD] is the backstop.
    for (label, src) in JUXTAPOSITION_SPLITS {
        let parsed = parse_program(src);
        assert!(
            !parsed.errors.is_empty() || !check_program(&parsed.program).is_empty(),
            "juxtaposition with {label} must be rejected, not split into \
             discarded statements; got zero errors for:\n{src}"
        );
    }
}

/// The second root, seen from the checker. A block's trailing expression
/// used to absorb the orphan a juxtaposition split left behind, so
/// `let r = double 5` became `let r = double` with the block's TAIL set to
/// `5`, and `go()` returned 5 where the source says 10.
///
/// Why this could never be a checker fix: after the split the block holds
/// exactly one statement, a `Let`. There is no `Stmt::Expr` for
/// `infer_block_stmt` to look at, so generalising [BLOCK-DISCARD] — however
/// far — cannot reach it. The tail was not discarded; it was RETURNED. The
/// fix is [LEX-STATEMENT-BREAK], in the parser.
///
/// So this asserts the outcome, not the diagnosis: the program is rejected,
/// AND no tree survives in which the block yields the bare argument. The
/// second half is what keeps the test honest — a rejection for some
/// unrelated reason would satisfy the first half alone.
#[test]
fn a_juxtaposed_argument_never_becomes_a_blocks_returned_value() {
    const SRC: &str = "fn double(x) = x * 2\n\
                       fn go() -> int = {\n\
                         let r = double 5\n\
                       }\n";
    let parsed = parse_program(SRC);
    assert!(
        !parsed.errors.is_empty(),
        "`let r = double 5` applies nothing and must be rejected; it parsed \
         clean into {:?}",
        parsed.program.statements
    );
    let tail = match parsed.program.statements.get(1) {
        Some(Stmt::Function {
            body: Expr::Block { value, .. },
            ..
        }) => value.as_deref(),
        _ => None,
    };
    assert_ne!(
        tail,
        Some(&Expr::Integer(5)),
        "the block still yields the ARGUMENT 5, so `go()` would return it \
         instead of applying `double`"
    );
}

#[test]
fn a_statement_that_discards_nothing_stays_accepted() {
    // The counterweight: the rule must not be satisfied by rejecting every
    // expression statement, which would delete side effects wholesale.
    for (label, src) in DISCARDS_NOTHING {
        assert!(
            check(src).is_empty(),
            "{label} discards nothing and must be accepted, got {:?} for:\n{src}",
            messages(src)
        );
    }
}

mod more;
