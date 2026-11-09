//! String literal and interpolation tests.
#![expect(
    clippy::indexing_slicing,
    reason = "test assertions: an out-of-bounds index is a test failure, not a production panic"
)]

use super::*;

/// A trivial fragment parser standing in for a flavor's real one, so the
/// shared splitter is exercised without pulling in either frontend.
fn frag(text: &str) -> Expr {
    Expr::Identifier(text.trim().to_string())
}

#[test]
fn unquote_resolves_every_escape_and_keeps_unknown() {
    // \n \r \t \e \0 \" \\ recognised; \q kept verbatim as `\q`.
    assert_eq!(
        unquote("\"\\n\\r\\t\\e\\0\\\"\\\\\\q\""),
        "\n\r\t\u{1b}\0\"\\\\q"
    );
    // A trailing lone backslash in a quote-less ML fragment hits the escape
    // match's `None` arm and is preserved rather than dropped. (ML passes
    // delimiter-free content; only a matched `"…"` pair is stripped, so a
    // single dangling quote is never fabricated here.)
    assert_eq!(unquote("x\\"), "x\\");
}

#[test]
fn unquote_keeps_a_trailing_escaped_quote_for_ml_and_default_raw() {
    // Regression (Osprey2's validation_pipeline twin): content ending in an
    // escaped quote. ML hands raw WITHOUT surrounding quotes; the Default
    // token carries them. Both must end in a literal `"`. Independent
    // prefix/suffix stripping ate the ML raw's closing `"`, diverging its IR
    // from the Default twin — only a matched `"…"` pair may be stripped.
    assert_eq!(unquote("he said \\\"hi\\\""), "he said \"hi\""); // ML raw, no quotes
    assert_eq!(unquote("\"he said \\\"hi\\\"\""), "he said \"hi\""); // Default token
}

#[test]
fn a_literal_line_break_is_a_newline_on_every_checkout() {
    assert_eq!(unquote("a\r\nb\nc"), "a\nb\nc");
    assert_eq!(unquote("lone\rreturn"), "lone\rreturn");
    assert_eq!(unquote("escaped\\r\\n"), "escaped\r\n");
}

#[test]
fn a_lambda_stays_code_while_text_and_nested_text_keep_their_escapes() {
    for (raw, decoded) in [
        ("${f(\\row => row)}", "${f(\\row => row)}"),
        ("${f(\\n => n)}\\n", "${f(\\n => n)}\n"),
        ("${f(\\row, x => row)}", "${f(\\row, x => row)}"),
        // A nested string's escapes wait for the parser that reads that string.
        ("${f (\"a\\tb\")}", "${f (\"a\\tb\")}"),
        ("${f(\"\\\"\")}", "${f(\"\\\"\")}"),
        ("${f(\"\\\\\")}", "${f(\"\\\\\")}"),
        (
            "${\"in\\t${f(\\row => row)}\"}",
            "${\"in\\t${f(\\row => row)}\"}",
        ),
        // The old spelling of a nested string is decoded here, once.
        ("${f(\\\"a\\\")}", "${f(\"a\")}"),
        ("${f(\\\"a\\\")}\\t", "${f(\"a\")}\t"),
        ("${1\\n x => y}", "${1\n x => y}"),
        ("${a\\tb}", "${a\tb}"),
        ("no fragment \\row => x", "no fragment \row => x"),
    ] {
        assert_eq!(unquote(raw), decoded, "{raw}");
    }
}

#[test]
fn interpolation_splits_text_expr_text_and_handles_nested_braces() {
    let parts = lower_interpolation("\"v ${1 + 2} end\"", None, Flavor::Default, frag);
    assert_eq!(parts.len(), 3);
    assert!(matches!(parts[0], InterpolatedPart::Text(ref t) if t == "v "));
    assert!(matches!(parts[1], InterpolatedPart::Expr(Expr::Identifier(ref e)) if e == "1 + 2"));
    assert!(matches!(parts[2], InterpolatedPart::Text(ref t) if t == " end"));
    // Nested braces inside `${…}` are captured whole, and an interpolation
    // ending exactly at `}` leaves no trailing text part.
    let nested = lower_interpolation("\"${match x { a => 1 }}\"", None, Flavor::Default, frag);
    assert_eq!(nested.len(), 1);
    assert!(matches!(nested[0], InterpolatedPart::Expr(_)));
}

#[test]
fn fragment_coordinates_follow_raw_escapes_and_each_flavors_column_unit() {
    let raw = "\"🦅\\n${alpha\\t+ beta}\"";
    for flavor in [Flavor::Default, Flavor::Ml] {
        let prefix = fragment_binding(flavor).len();
        let maps = fragments(raw, Position { line: 7, column: 4 });
        assert_eq!(maps.len(), 1);
        let map = &maps[0];
        assert_eq!(map.text, "alpha\t+ beta");
        let beta = map.text.find("beta").unwrap_or_default();
        let range = map.map_range(prefix + beta..prefix + beta + 4, prefix);
        assert_eq!(range, raw.find("beta").map(|start| start..start + 4));
        assert_eq!(range.and_then(|range| raw.get(range)), Some("beta"));
        let before = raw.split("beta").next().unwrap_or_default();
        let width = match flavor {
            Flavor::Default | Flavor::Python => before.len(),
            Flavor::Ml => before.chars().count(),
        };
        assert_eq!(
            map.map_position(
                Position {
                    line: 1,
                    column: u32::try_from(prefix + beta).unwrap_or_else(|error| panic!("{error}"))
                },
                prefix,
                flavor
            ),
            Some(Position {
                line: 7,
                column: 4 + u32::try_from(width).unwrap_or_else(|error| panic!("{error}"))
            })
        );
        assert_eq!(
            map.map_position(Position { line: 1, column: 0 }, prefix, flavor),
            None
        );
        assert_eq!(map.map_range(0..1, prefix), None);
    }
}

#[test]
fn a_decoded_newline_inside_a_fragment_does_not_invent_a_source_line() {
    let raw = "\"${first\\nsecond}\"";
    let maps = fragments(raw, Position { line: 2, column: 3 });
    let map = &maps[0];
    assert_eq!(map.text, "first\nsecond");
    assert_eq!(
        map.map_position(Position { line: 2, column: 0 }, 14, Flavor::Default),
        Some(Position {
            line: 2,
            column: 13
        })
    );
    for flavor in [Flavor::Default, Flavor::Ml] {
        let mut block = Expr::Block {
            statements: vec![],
            value: Some(Box::new(Expr::Identifier("second".into()))),
            position: Some(Position { line: 2, column: 0 }),
        };
        positions::rebase_expr(&mut block, map, flavor);
        assert!(matches!(
            block,
            Expr::Block {
                position: Some(Position {
                    line: 2,
                    column: 13
                }),
                ..
            }
        ));
    }
    let multiline = fragments("\"${first\nsecond}\"", Position { line: 2, column: 3 });
    assert_eq!(
        multiline[0].map_position(Position { line: 2, column: 0 }, 14, Flavor::Default),
        Some(Position { line: 3, column: 0 })
    );
}
