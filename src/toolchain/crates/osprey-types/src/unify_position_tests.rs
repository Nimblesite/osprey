//! Unify diagnostics carry source positions.
//!
//! Every "cannot unify" error used to print with no `line:col` at all — the
//! CLI's `path: message` shape instead of `path:line:col: message` — so a
//! mismatch in a large program said nothing about where to look. A unify
//! failure now carries the position of the expression being inferred when it
//! failed, and a deferred-arithmetic site carries the position of the
//! operator that wrote it.

use crate::testutil::typecheck;
use osprey_ast::Position;
use osprey_syntax::Flavor;

fn error_position(flavor: Flavor, source: &str, fragment: &str) -> Option<Position> {
    let errors = typecheck(flavor, source);
    let position = errors
        .iter()
        .find(|error| error.message.contains(fragment))
        .and_then(|error| error.position);
    assert!(position.is_some(), "{errors:?}");
    position
}

/// Each mismatch names the line that wrote it: a mixed `+` operand names its
/// operator, a call argument names its statement, a violated `abs` obligation
/// names its expression — and the ML flavor's twins agree.
#[test]
fn a_mismatch_names_the_line_that_wrote_it() {
    for (flavor, source, fragment, line) in [
        (
            Flavor::Default,
            "let a = 1\nlet b = \"x\"\nlet c = a + b\nprint(toString(c))",
            "cannot unify",
            3,
        ),
        (
            Flavor::Default,
            "fn id(x: int) = x\nprint(toString(id(1.5)))",
            "cannot unify",
            2,
        ),
        (
            Flavor::Default,
            "let x = abs(\"text\")\nprint(toString(x))",
            "`abs` requires int or float",
            1,
        ),
        (
            Flavor::Ml,
            "a = 1\nb = \"x\"\nc = a + b\nprint (toString c)",
            "cannot unify",
            3,
        ),
    ] {
        let position = error_position(flavor, source, fragment);
        assert_eq!(position.map(|position| position.line), Some(line));
    }
}

/// A deferred site that still conflicts — an integer call against a float
/// comparison — names the operator whose overload conflicted.
#[test]
fn a_deferred_site_conflict_names_its_operator() {
    let position = error_position(
        Flavor::Default,
        "fn f(a, b) = (a - b) <= 1.0\nprint(toString(f(1, 2)))",
        "cannot unify",
    );
    assert_eq!(
        position,
        Some(Position {
            line: 1,
            column: 16
        })
    );
}

/// [FAILURE-EXPLICIT] Since the HTTP externs return `Result` (#337, #353), a
/// program written against their old `int` returns fails here. The mismatch
/// names the migration: decode the `Result` first. Every Result-returning
/// extern, in every way a value is used as its payload, says so; a mismatch
/// with no `Result` in it never does.
#[test]
fn a_result_used_as_its_payload_names_how_to_decode_it() {
    const HINT: &str = "decode the `Result` with `match` or `?:` before using it as";
    let calls = [
        "httpCreateServer(8080, \"127.0.0.1\")",
        "httpGet(1, \"/\", \"\")",
        "httpCreateClient(\"http://127.0.0.1\", 1000)",
        "httpCloseClient(1)",
    ];
    let uses = [
        "let next = value + 1\nprint(toString(next))",
        "fn twice(n: int) -> int = n * 2\nprint(toString(twice(value)))",
        "fn serve() -> int = value\nprint(toString(serve()))",
        "let total: int = value\nprint(toString(total))",
    ];
    for call in calls {
        for use_site in uses {
            let source = format!("let value = {call}\n{use_site}");
            let errors = typecheck(Flavor::Default, &source);
            let hinted: Vec<_> = errors.iter().filter(|e| e.message.contains(HINT)).collect();
            assert_eq!(hinted.len(), 1, "{source}\n{errors:?}");
            for error in hinted {
                let message = &error.message;
                assert!(message.contains("Result<int, Error>"), "{message}");
                assert!(message.ends_with(&format!("{HINT} `int`")), "{message}");
                assert!(error.position.is_some(), "{message}");
            }
        }
    }
    for clean in [
        "let a = 1\nlet b = \"x\"\nlet c = a + b\nprint(toString(c))",
        "fn twice(n: int) -> int = n * 2\nprint(toString(twice(\"x\")))",
        "let flag: bool = 3\nprint(toString(flag))",
        // The other direction: a payload where a `Result` was expected is not decoded.
        "type Feed<out T> = Feed { supply: T } | Dry\nfn feedInt() -> Feed<int> = Feed { supply: 1 }\nfn total(f: Feed<Result<int, string>>) = match f {\n    Feed { supply } => supply ?: 0\n    Dry => 0\n}\nprint(\"${total(feedInt())}\")",
    ] {
        let errors = typecheck(Flavor::Default, clean);
        assert!(
            errors.iter().any(|e| e.message.contains("cannot unify")),
            "{clean}\n{errors:?}"
        );
        assert!(
            errors.iter().all(|e| !e.message.contains(HINT)),
            "{clean}\n{errors:?}"
        );
    }
}
