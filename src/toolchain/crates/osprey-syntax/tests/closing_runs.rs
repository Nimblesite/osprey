//! Runs of closing brackets, the mark a callback pyramid leaves behind
//! ([LINT-NESTING-LAMBDA]): eight or more `)`, `}` or `]` on one line, with
//! only spaces between them, in either flavor.

use osprey_syntax::{closing_runs, Flavor, MIN_CLOSING_RUN};

/// `(line, column, length)` of one run.
type Run = (u32, u32, usize);

/// Every run in `source`.
fn runs(source: &str, flavor: Flavor) -> Vec<Run> {
    closing_runs(source, flavor)
        .into_iter()
        .map(|run| (run.position.line, run.position.column, run.length))
        .collect()
}

/// `depth` nested calls in the Default flavor, closed on line 1 at column
/// `10 + 2 * depth`.
fn nested(depth: usize) -> String {
    format!("fn f() = {}1{}\n", "g(".repeat(depth), ")".repeat(depth))
}

/// `depth` nested calls in the ML flavor, closed on line 1 at column `5 + 3 * depth`.
fn ml_nested(depth: usize) -> String {
    format!("f = {}1{}\n", "g (".repeat(depth), ")".repeat(depth))
}

#[test]
fn the_threshold_is_eight() {
    assert_eq!(MIN_CLOSING_RUN, 8);
}

#[test]
fn runs_start_at_eight_closers_and_are_counted_in_both_flavors() {
    for depth in 1..=12 {
        let at = u32::try_from(depth).unwrap_or(u32::MAX);
        let (default, ml) = if depth >= MIN_CLOSING_RUN {
            (vec![(1, 10 + 2 * at, depth)], vec![(1, 5 + 3 * at, depth)])
        } else {
            (Vec::new(), Vec::new())
        };
        assert_eq!(
            runs(&nested(depth), Flavor::Default),
            default,
            "depth {depth}"
        );
        assert_eq!(runs(&ml_nested(depth), Flavor::Ml), ml, "ML depth {depth}");
    }
}

#[test]
fn default_runs_are_found_where_they_start() {
    let cases: [(&str, &[Run]); 8] = [
        // Spaces between closers on one line do not break a run.
        ("fn f() = g(g(g(g(g(g(g(g(1) ) ) ) ) ) ) )\n", &[(1, 26, 8)]),
        // A run split over two lines is two short runs: layout, not nesting.
        ("fn f() = g(g(g(g(g(g(g(g(1))))\n))))\n", &[]),
        ("fn f() = g(g(g(g(g(g(g(g(g(1))\n)))))))\n", &[]),
        // Brackets of every kind count together.
        (
            "fn f() = g([h({ value: i([j({ k: [1] })]) })])\n",
            &[(1, 35, 9)],
        ),
        // A separator ends a run: two runs of four are not one of eight.
        ("fn f() = g(g(g(g(1)))), h(h(h(h(2))))\n", &[]),
        // Each run is reported once, where it starts.
        (
            "fn f() = g(g(g(g(g(g(g(g(1))))))))\nfn h() = g(g(g(g(g(g(g(g(g(2)))))))))\n",
            &[(1, 26, 8), (2, 28, 9)],
        ),
        // Closers inside strings and comments are text, not brackets.
        ("let s = \"))))))))))\"\n// }}}}}}}}}}\nfn f() = s\n", &[]),
        ("let s = \"${g(g(1))}))))))))\"\nfn f() = s\n", &[]),
    ];
    for (source, expected) in cases {
        assert_eq!(runs(source, Flavor::Default), expected, "{source}");
    }
}

#[test]
fn a_flat_use_block_leaves_no_run_where_its_pyramid_did() {
    let steps = 8;
    let pyramid = format!(
        "fn chain(n) = {}Success {{ value: n }}{}\n",
        (1..=steps)
            .map(|i| format!("andThen(step({i}, n), fn(v{i}) => "))
            .collect::<Vec<_>>()
            .concat(),
        ")".repeat(steps)
    );
    let flat = format!(
        "fn chain(n) = {{\n{}    Success {{ value: n }}\n}}\n",
        (1..=steps)
            .map(|i| format!("    use v{i} <- andThen(step({i}, n))\n"))
            .collect::<Vec<_>>()
            .concat()
    );
    // The record's `}` opens the run: the eight callbacks close after it.
    let close = u32::try_from(pyramid.find("})").unwrap_or(0)).unwrap_or(0);
    assert_eq!(runs(&pyramid, Flavor::Default), vec![(1, close, steps + 1)]);
    assert_eq!(runs(&flat, Flavor::Default), Vec::<Run>::new(), "{flat}");
}

#[test]
fn ml_runs_count_only_one_line() {
    let cases: [(&str, &[Run]); 4] = [
        // A block ending inside a call: the dedent splits the closers over lines.
        (
            "f = g (\\a => g (\\b => g (\\c => g (\\d =>\n    g (g (g (g (a + b + c + d))))))))\n",
            &[(2, 29, 8)],
        ),
        (
            "f = g (\\a => g (\\b => g (\\c => g (\\d =>\n    g (a + b + c + d)))))\n",
            &[],
        ),
        ("f = [g ([g ([g ([g (1)])])])]\n", &[(1, 21, 8)]),
        ("s = \"))))))))))\"\n(** }}}}}}}}}} *)\nf = s\n", &[]),
    ];
    for (source, expected) in cases {
        assert_eq!(runs(source, Flavor::Ml), expected, "{source}");
    }
}

#[test]
fn foreign_flavors_keep_their_own_bracket_style() {
    assert_eq!(
        runs("x = f(g(h(i(j(k(l(m(1))))))))\n", Flavor::Python),
        Vec::<Run>::new()
    );
    assert_eq!(
        runs("const x = f(g(h(i(j(k(l(m(1))))))));\n", Flavor::TypeScript),
        Vec::<Run>::new()
    );
}
