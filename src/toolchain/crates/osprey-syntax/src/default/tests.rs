//! Default-flavor parser tests.
#![expect(
    clippy::indexing_slicing,
    reason = "test assertions: an out-of-bounds index is a test failure, not a production panic"
)]

use crate::parse_program;
use crate::test_support::one_stmt;
use osprey_ast::{Expr, Pattern, Stmt};

#[test]
fn lowers_doc_comments_on_let_and_function() {
    // A `///` block above a binding is captured as its `doc`, stripped of the
    // markers, and the recorded position stays on the declaration keyword/name
    // (line 3 here), not the comment lines. Implements [LSP-HOVER-DOCS]
    match one_stmt(
        "/// The retry budget.\n/// Bounded above by `maxRetries`.\nlet retries: int = 3\n",
    ) {
        Stmt::Let {
            name,
            doc,
            position,
            ..
        } => {
            assert_eq!(name, "retries");
            // The multi-line doc is one paragraph → the whole summary.
            let d = doc.as_ref().expect("doc present");
            assert_eq!(
                d.summary,
                "The retry budget. Bounded above by `maxRetries`."
            );
            assert_eq!(position.map(|p| p.line), Some(3));
        }
        s => panic!("expected let, got {s:?}"),
    }
    match one_stmt("/// Adds two ints.\nfn add(a: int, b: int) -> int = a + b\n") {
        Stmt::Function { doc, position, .. } => {
            assert_eq!(
                doc.as_ref().map(|d| d.summary.clone()).as_deref(),
                Some("Adds two ints.")
            );
            assert_eq!(position.map(|p| p.line), Some(2));
        }
        s => panic!("expected function, got {s:?}"),
    }
    // An undocumented binding carries no doc.
    match one_stmt("let x = 1\n") {
        Stmt::Let { doc, .. } => assert_eq!(doc, None),
        s => panic!("expected let, got {s:?}"),
    }
}

#[test]
fn lowers_let() {
    match one_stmt("let x = 42\n") {
        Stmt::Let {
            name,
            value,
            mutable,
            ..
        } => {
            assert_eq!(name, "x");
            assert!(!mutable);
            assert_eq!(value, Expr::Integer(42));
        }
        s => panic!("expected let, got {s:?}"),
    }
}

#[test]
fn lowers_function_with_binary_body() {
    match one_stmt("fn add(a: int, b: int) -> int = a + b\n") {
        Stmt::Function {
            name,
            parameters,
            return_type,
            body,
            ..
        } => {
            assert_eq!(name, "add");
            assert_eq!(parameters.len(), 2);
            assert_eq!(parameters[0].name, "a");
            assert_eq!(return_type.unwrap().name, "int");
            match body {
                Expr::Binary { op, .. } => assert_eq!(op, "+"),
                b => panic!("expected binary, got {b:?}"),
            }
        }
        s => panic!("expected function, got {s:?}"),
    }
}

#[test]
fn lowers_union_type() {
    match one_stmt("type Color = Red | Green | Blue\n") {
        Stmt::Type { name, variants, .. } => {
            assert_eq!(name, "Color");
            assert_eq!(variants.len(), 3);
            assert_eq!(variants[2].name, "Blue");
        }
        s => panic!("expected type, got {s:?}"),
    }
}

#[test]
fn lowers_extern_with_ptr() {
    match one_stmt("extern fn sqlite3_open(filename: string, ppDb: Ptr) -> int\n") {
        Stmt::Extern {
            name,
            parameters,
            return_type,
            ..
        } => {
            assert_eq!(name, "sqlite3_open");
            assert_eq!(parameters.len(), 2);
            assert_eq!(parameters[1].ty.name, "Ptr");
            assert_eq!(return_type.unwrap().name, "int");
        }
        s => panic!("expected extern, got {s:?}"),
    }
}

#[test]
fn lowers_match() {
    match one_stmt("let r = match x {\n  Ok { value } => value\n  _ => 0\n}\n") {
        Stmt::Let {
            value: Expr::Match { arms, .. },
            ..
        } => {
            assert_eq!(arms.len(), 2);
            assert!(matches!(arms[1].pattern, Pattern::Wildcard));
        }
        s => panic!("expected let-match, got {s:?}"),
    }
}

#[test]
fn lowers_effect_and_perform() {
    let parsed = parse_program(
        "effect Log { info: fn(string) -> Unit }\nfn go() = perform Log.info(msg: \"hi\")\n",
    );
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    assert!(matches!(parsed.program.statements[0], Stmt::Effect { .. }));
}

#[test]
fn reports_syntax_error() {
    let parsed = parse_program("fn (= \n");
    assert_ne!(parsed.errors, Vec::<crate::SyntaxError>::new());
}

#[test]
fn reports_missing_node_error() {
    // An arm with no body forces tree-sitter to insert a MISSING
    // identifier; collect_errors reports it via the is_missing format branch.
    let parsed = parse_program("fn f() = match v { 1 => }\n");
    assert!(
        parsed
            .errors
            .iter()
            .any(|e| e.message.starts_with("missing")),
        "expected a missing-node error, got {:?}",
        parsed.errors
    );
    // The error carries a 1-based line.
    assert!(parsed.errors[0].position.line >= 1);
}

#[test]
fn rejects_out_of_range_integer_literals_without_substituting_zero() {
    let too_large = parse_program("let n = 9223372036854775808\n");
    assert!(too_large
        .errors
        .iter()
        .any(|e| e.message.contains("outside the signed 64-bit range")));

    let minimum = parse_program("let n = -9223372036854775808\n");
    assert!(minimum.errors.is_empty(), "errors: {:?}", minimum.errors);
    assert!(matches!(
        minimum.program.statements.first(),
        Some(Stmt::Let {
            value: Expr::Integer(i64::MIN),
            ..
        })
    ));
}

/// SECOND ROOT. A block is `'{' repeat(statement) optional(expression) '}'`
/// (`src/toolchain/tree-sitter-osprey/grammar.js`), and that trailing `optional`
/// expression will ABSORB an orphan left behind when a juxtaposed
/// application is split.
///
/// `let r = double 5` inside a block does not parse as a call — Default has
/// no application production — so it becomes `let r = double` with the
/// block's TAIL set to `5`. The block's value is therefore the ARGUMENT the
/// author wrote, and `go()` returns 5 where the source says 10.
///
/// This is a distinct root from the discard guard in
/// `osprey-types::check::infer_block_stmt`, and it is why fixing that guard
/// alone is not enough: here NOTHING is discarded. Exactly one statement
/// survives (`let r = double`), the tail is well-typed, the return type
/// matches, and the program type-checks with zero errors. No discard rule,
/// however general, can reach a value that is being RETURNED.
///
/// Each source below is a whole program that must be REJECTED.
const BLOCK_TAIL_ABSORBS_JUXTAPOSED_ARGUMENT: &[(&str, &str)] = &[
    (
        "integer argument",
        "fn double(x) = x * 2\n\
         fn go() -> int = {\n\
           let r = double 5\n\
         }\n",
    ),
    (
        "string argument",
        "fn greet(s) = s\n\
         fn go() -> string = {\n\
           let r = greet \"hi\"\n\
         }\n",
    ),
    (
        "identifier argument",
        "fn double(x) = x * 2\n\
         fn go(n) -> int = {\n\
           let r = double n\n\
         }\n",
    ),
    (
        "two arguments, the last absorbed",
        "fn add(a, b) = a + b\n\
         fn go() -> int = {\n\
           let r = add 2 3\n\
         }\n",
    ),
    (
        "nested block tail",
        "fn double(x) = x * 2\n\
         fn go() -> int = {\n\
           let inner = {\n\
             let r = double 5\n\
           }\n\
           inner\n\
         }\n",
    ),
    (
        "lambda body block tail",
        "fn double(x) = x * 2\n\
         fn go() -> int = {\n\
           let f = |n| => {\n\
             let r = double n\n\
           }\n\
           f(1)\n\
         }\n",
    ),
];

#[test]
fn a_block_tail_does_not_absorb_a_juxtaposed_argument() {
    for (label, src) in BLOCK_TAIL_ABSORBS_JUXTAPOSED_ARGUMENT {
        let parsed = parse_program(src);
        assert!(
            !parsed.errors.is_empty(),
            "a block tail absorbing a juxtaposed {label} must be rejected; \
             parsed clean into {:?} for:\n{src}",
            parsed.program.statements
        );
    }
}

#[test]
fn a_juxtaposed_argument_never_becomes_the_blocks_value() {
    // The shape assertion, stated positively so it cannot be satisfied by
    // an unrelated parse error: whatever `let r = double 5` means, the
    // block's VALUE must never be the literal `5` the author wrote as the
    // argument. Today it is exactly that, which is how `go()` returns 5.
    let parsed = parse_program(
        "fn double(x) = x * 2\n\
         fn go() -> int = {\n\
           let r = double 5\n\
         }\n",
    );
    let Some(Stmt::Function { body, .. }) = parsed.program.statements.get(1) else {
        panic!(
            "expected a second function statement, got {:?}",
            parsed.program.statements
        );
    };
    if let Expr::Block { value, .. } = body {
        assert_ne!(
            value.as_deref(),
            Some(&Expr::Integer(5)),
            "the block's value is the argument `5`, so `go()` returns the \
             ARGUMENT instead of applying `double` to it"
        );
    }
}
