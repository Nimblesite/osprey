//! Flavor-neutral string handling shared by every frontend: `${…}`
//! interpolation splitting and backslash-escape resolution. These belong to no
//! single flavor — both the Default (brace) and ML (layout) frontends call them
//! with their own fragment parser, so the scanning and escape rules live here in
//! exactly one place rather than being reached out of either flavor's folder
//! ([FLAVOR-FRONTEND], [STRING-INTERPOLATION]).

use osprey_ast::{Expr, InterpolatedPart};

/// Split a `"text ${expr} more"` literal into [`InterpolatedPart`]s, parsing
/// each embedded expression with `parse_frag` (the active flavor's fragment
/// parser). Shared by the Default and ML frontends so the `${…}`-scanning and
/// escape handling exist in exactly one place. [STRING-INTERPOLATION]
pub(crate) fn lower_interpolation(
    raw: &str,
    parse_frag: impl Fn(&str) -> Expr,
) -> Vec<InterpolatedPart> {
    let inner = unquote(raw);
    let bytes = inner.as_bytes();
    let mut parts = Vec::new();
    let mut text_start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes.get(i) == Some(&b'$') && bytes.get(i + 1) == Some(&b'{') {
            if i > text_start {
                if let Some(text) = inner.get(text_start..i) {
                    parts.push(InterpolatedPart::Text(text.to_string()));
                }
            }
            // Find the `}` that closes this `${`, honouring nested braces so
            // `${match x { a => 1 b => 2 }}` captures the whole match.
            let mut depth = 1i32;
            let mut j = i + 2;
            while let Some(byte) = bytes.get(j) {
                match byte {
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            if let Some(frag) = inner.get(i + 2..j) {
                parts.push(InterpolatedPart::Expr(parse_frag(frag)));
            }
            i = j + 1;
            text_start = i;
        } else {
            i += 1;
        }
    }
    if let Some(text) = inner.get(text_start..) {
        if !text.is_empty() {
            parts.push(InterpolatedPart::Text(text.to_string()));
        }
    }
    parts
}

/// Strip surrounding quotes and resolve backslash escapes in one pass (so a
/// literal `\\` can never be re-interpreted): `\n` `\r` `\t` newline/CR/tab,
/// `\e` the ANSI ESC (0x1B, used by the terminal-color helpers), `\0` NUL,
/// `\"` and `\\` the literals. An unrecognised escape is kept verbatim.
pub(crate) fn unquote(s: &str) -> String {
    // Strip only a MATCHED surrounding `"…"` pair, atomically. The Default
    // tree-sitter token carries both quotes; the ML lexer already drops them, so
    // its raw never starts with `"`. Stripping the delimiters *independently*
    // would eat the closing `"` of a string whose content ends in an escaped
    // quote (`"he said \"hi\""` → ML raw `he said \"hi\"` ends in `"`), diverging
    // the ML IR from the Default twin — the regression Osprey2 hit on
    // validation_pipeline. Requiring the pair leaves an unmatched lone quote in
    // place, which is correct for both a quote-less ML raw and a salvaged token.
    let trimmed = s
        .strip_prefix('"')
        .and_then(|x| x.strip_suffix('"'))
        .unwrap_or(s);
    let mut out = String::with_capacity(trimmed.len());
    let mut chars = trimmed.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('e') => out.push('\u{1b}'),
            Some('0') => out.push('\0'),
            Some('"') => out.push('"'),
            // An escaped backslash, or a trailing lone backslash at end of input.
            Some('\\') | None => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
        }
    }
    out
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    reason = "test assertions: an out-of-bounds index is a test failure, not a production panic"
)]
mod tests {
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
    fn interpolation_splits_text_expr_text_and_handles_nested_braces() {
        let parts = lower_interpolation("\"v ${1 + 2} end\"", frag);
        assert_eq!(parts.len(), 3);
        assert!(matches!(parts[0], InterpolatedPart::Text(ref t) if t == "v "));
        assert!(
            matches!(parts[1], InterpolatedPart::Expr(Expr::Identifier(ref e)) if e == "1 + 2")
        );
        assert!(matches!(parts[2], InterpolatedPart::Text(ref t) if t == " end"));
        // Nested braces inside `${…}` are captured whole, and an interpolation
        // ending exactly at `}` leaves no trailing text part.
        let nested = lower_interpolation("\"${match x { a => 1 }}\"", frag);
        assert_eq!(nested.len(), 1);
        assert!(matches!(nested[0], InterpolatedPart::Expr(_)));
    }
}
