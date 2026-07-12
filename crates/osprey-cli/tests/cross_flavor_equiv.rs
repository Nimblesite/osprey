//! Cross-flavor equivalence ([FLAVOR-TEST] / [FLAVOR-CURRY],
//! docs/specs/0023-LanguageFlavors.md). ML **curries by default** and offers a
//! second *uncurried* surface, so it mirrors BOTH Default function forms at the
//! canonical AST (modulo source positions). Three buckets pin the boundary:
//!
//! 1. ML curried `add x y = …` (sugar for `add = \x -> \y -> …`) ≡ Default
//!    *explicit-curry* `fn add(x) = fn(y) => …` — a one-parameter
//!    [`Stmt::Function`] whose body is an [`Expr::Lambda`].
//! 2. ML uncurried `add (x, y) = …` ≡ Default *multi-parameter* `fn add(x, y)` —
//!    one flat two-parameter [`Stmt::Function`], the `(int, int) -> int` shape.
//! 3. The two are NOT interchangeable: ML curried `add x y` differs from Default
//!    multi-parameter `fn add(x, y)`. Currying is a real semantic difference, not
//!    a spelling — each ML surface form mirrors its own Default twin and no other.

use osprey_syntax::{parse_program_with_flavor, Flavor};

/// The canonical AST as a debug string with every source `Position { … }`
/// payload scrubbed, so structural equality ignores spans.
fn canonical(src: &str, flavor: Flavor) -> String {
    let parsed = parse_program_with_flavor(src, flavor);
    assert!(
        parsed.errors.is_empty(),
        "unexpected {flavor} syntax errors: {:?}",
        parsed.errors
    );
    scrub_positions(&format!("{:?}", parsed.program))
}

/// Drop every `Position { line: N, column: M }` from a debug string. `Position`
/// has no nested braces, so the next `}` always closes it.
fn scrub_positions(debug: &str) -> String {
    let mut out = String::with_capacity(debug.len());
    let mut rest = debug;
    while let Some(idx) = rest.find("Position {") {
        out.push_str(&rest[..idx]);
        rest = &rest[idx..];
        match rest.find('}') {
            Some(close) => rest = &rest[close + 1..],
            None => break,
        }
    }
    out.push_str(rest);
    out
}

#[test]
fn doc_comments_lower_identically_across_flavors() {
    // A `///`-documented Default function and its `(** … *)` ML twin lower to
    // the SAME DocComment on the canonical AST — the doc body markup is
    // flavor-neutral, only the sigil differs ([DOC-MODEL], [FLAVOR-BOUNDARY]).
    // Both are curried single-param functions so the whole AST (doc included)
    // matches, not just the doc field.
    let default_doc = "/// Doubles `x`.\n\
                       ///\n\
                       /// # Returns\n\
                       /// twice x\n\
                       fn dbl(x) = fn(y) => x + y\n";
    let ml_doc = "(** Doubles `x`.\n\n    # Returns\n    twice x *)\n\
                  dbl x y = x + y\n";
    assert_eq!(
        canonical(default_doc, Flavor::Default),
        canonical(ml_doc, Flavor::Ml),
        "a documented function must lower to an identical DocComment in both flavors"
    );
}

#[test]
fn ml_multiparam_equals_default_explicit_curry() {
    // Both lower to a one-parameter Function { params: [x] } whose body is a
    // Lambda { params: [y], body: x + y } — ML curries by default, so the
    // multi-binding `add x y = …` IS the Default explicit-curry chain.
    let default_curry = "fn add(x) = fn(y) => x + y\n";
    let ml_multi = "add x y = x + y\n";
    assert_eq!(
        canonical(default_curry, Flavor::Default),
        canonical(ml_multi, Flavor::Ml),
        "curried ML multi-binding must equal Default explicit-curry at the canonical AST"
    );
}

#[test]
fn ml_uncurried_tuple_equals_default_multiparam() {
    // Bucket 2: ML's uncurried surface `add (x, y) = …` lowers to one flat
    // two-parameter Function { params: [x, y], body: x + y } — byte-identical to
    // Default `fn add(x, y)`. This is the mirror twin for every multi-parameter
    // Default function, so a `.osp` multi-param binding pairs with a `.ospml`
    // tuple binding and they emit identical IR.
    let default_multi = "fn add(x, y) = x + y\n";
    let ml_tuple = "add (x, y) = x + y\n";
    assert_eq!(
        canonical(default_multi, Flavor::Default),
        canonical(ml_tuple, Flavor::Ml),
        "ML uncurried tuple binding must equal Default multi-param at the canonical AST"
    );
}

#[test]
fn ml_multiparam_differs_from_default_multiparam() {
    // Default `fn add(x, y) = …` is one two-parameter Function — the uncurried
    // `(int, int) -> int`, a different node than the curried ML chain (a
    // one-parameter Function returning a Lambda). Currying is a real semantic
    // difference, not a spelling: ML never lowers to the multi-parameter form.
    let default_multi = "fn add(x, y) = x + y\n";
    let ml_multi = "add x y = x + y\n";
    assert_ne!(
        canonical(default_multi, Flavor::Default),
        canonical(ml_multi, Flavor::Ml),
        "Default multi-param must NOT equal the curried ML multi-binding"
    );
}
