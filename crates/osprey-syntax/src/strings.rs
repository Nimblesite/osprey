//! Shared string decoding, interpolation splitting, and exact source mapping.
//! Implements [STRING-INTERPOLATION] and [TYPE-WARNINGS-UNUSED].

use crate::Flavor;
use osprey_ast::{Expr, InterpolatedPart, Position};
use std::ops::Range;

#[path = "strings_positions.rs"]
mod positions;

/// The synthetic binding used by each flavor's interpolation parser.
pub(crate) const fn fragment_binding(flavor: Flavor) -> &'static str {
    match flavor {
        Flavor::Default => "let __frag__ = ",
        Flavor::Ml => "__frag__ = ",
    }
}

struct Decoded {
    text: String,
    /// Each decoded byte boundary maps to a byte boundary in the raw literal.
    offsets: Vec<usize>,
}

/// One decoded expression and its exact spelling inside the enclosing token.
pub(crate) struct FragmentMap<'a> {
    pub(crate) text: String,
    raw: &'a str,
    offsets: Vec<usize>,
    base: Position,
}

impl FragmentMap<'_> {
    /// Map a span in the synthetic prefixed program to raw-token byte offsets.
    pub(crate) fn map_range(&self, range: Range<usize>, prefix: usize) -> Option<Range<usize>> {
        let start = range.start.checked_sub(prefix)?;
        let end = range.end.checked_sub(prefix)?;
        Some(*self.offsets.get(start)?..*self.offsets.get(end)?)
    }

    /// Map frontend coordinates, including their flavor's column convention.
    pub(crate) fn map_position(
        &self,
        inner: Position,
        prefix: usize,
        flavor: Flavor,
    ) -> Option<Position> {
        let column = usize::try_from(inner.column)
            .ok()?
            .checked_sub(if inner.line == 1 { prefix } else { 0 })?;
        let line = usize::try_from(inner.line.checked_sub(1)?).ok()?;
        let start = line_start(&self.text, line)?;
        let text = self.text.get(start..)?.split('\n').next()?;
        let offset = start + column_byte(text, column, flavor)?;
        let raw = self.raw.get(..*self.offsets.get(offset)?)?;
        Some(advance_position(self.base, raw, flavor))
    }
}

fn line_start(text: &str, line: usize) -> Option<usize> {
    if line == 0 {
        return Some(0);
    }
    text.match_indices('\n')
        .nth(line - 1)
        .map(|(offset, _)| offset + 1)
}

fn column_byte(text: &str, column: usize, flavor: Flavor) -> Option<usize> {
    match flavor {
        Flavor::Default => text.is_char_boundary(column).then_some(column),
        Flavor::Ml => text
            .char_indices()
            .map(|(offset, _)| offset)
            .chain(std::iter::once(text.len()))
            .nth(column),
    }
}

fn advance_position(base: Position, text: &str, flavor: Flavor) -> Position {
    let lines = text.bytes().filter(|byte| *byte == b'\n').count();
    let tail = text.rsplit('\n').next().unwrap_or_default();
    let width = match flavor {
        Flavor::Default => tail.len(),
        Flavor::Ml => tail.chars().count(),
    };
    Position {
        line: base
            .line
            .saturating_add(u32::try_from(lines).unwrap_or(u32::MAX)),
        column: if lines == 0 { base.column } else { 0 }
            .saturating_add(u32::try_from(width).unwrap_or(u32::MAX)),
    }
}

/// Actual interpolation fragments, with one shared decoded/raw mapping used
/// by compiler positions, warning highlights, and source edits.
pub(crate) fn fragments(raw: &str, base: Position) -> Vec<FragmentMap<'_>> {
    let decoded = decode(raw);
    interpolation_ranges(&decoded.text)
        .into_iter()
        .filter_map(|range| fragment_map(raw, &decoded, range, base))
        .collect()
}

fn fragment_map<'a>(
    raw: &'a str,
    decoded: &Decoded,
    range: Range<usize>,
    base: Position,
) -> Option<FragmentMap<'a>> {
    Some(FragmentMap {
        text: decoded.text.get(range.clone())?.to_owned(),
        raw,
        offsets: decoded.offsets.get(range.start..=range.end)?.to_vec(),
        base,
    })
}

/// Split text and expressions without losing source bytes to escape decoding.
/// Both frontends pass the complete literal, including its surrounding quotes.
pub(crate) fn lower_interpolation(
    raw: &str,
    base: Option<Position>,
    flavor: Flavor,
    parse_frag: impl Fn(&str) -> Expr,
) -> Vec<InterpolatedPart> {
    let decoded = decode(raw);
    let mut parts = Vec::new();
    let mut start = 0;
    for range in interpolation_ranges(&decoded.text) {
        text_part(
            &decoded.text,
            start..range.start.saturating_sub(2),
            &mut parts,
        );
        if let Some(fragment) = fragment_map(raw, &decoded, range.clone(), base.unwrap_or_default())
        {
            let mut expression = parse_frag(&fragment.text);
            if base.is_some() {
                positions::rebase_expr(&mut expression, &fragment, flavor);
            }
            parts.push(InterpolatedPart::Expr(expression));
        }
        start = range.end.saturating_add(1);
    }
    text_part(&decoded.text, start..decoded.text.len(), &mut parts);
    parts
}

fn text_part(text: &str, range: Range<usize>, parts: &mut Vec<InterpolatedPart>) {
    if let Some(text) = text.get(range).filter(|text| !text.is_empty()) {
        parts.push(InterpolatedPart::Text(text.to_owned()));
    }
}

fn interpolation_ranges(text: &str) -> Vec<Range<usize>> {
    let bytes = text.as_bytes();
    let mut ranges = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes.get(i) == Some(&b'$') && bytes.get(i + 1) == Some(&b'{') {
            let end = fragment_end(bytes, i + 2);
            ranges.push(i + 2..end);
            i = end.saturating_add(1);
        } else {
            i += 1;
        }
    }
    ranges
}

fn fragment_end(bytes: &[u8], start: usize) -> usize {
    let mut depth = 1usize;
    let mut index = start;
    while let Some(byte) = bytes.get(index) {
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
        index += 1;
    }
    index
}

/// Strip a matched quote pair and resolve escapes once. Unrecognized escapes
/// remain verbatim; every output boundary retains its original byte offset.
fn decode(raw: &str) -> Decoded {
    let (text, start) = raw
        .strip_prefix('"')
        .and_then(|text| text.strip_suffix('"'))
        .map_or((raw, 0), |text| (text, 1));
    let mut decoded = Decoded {
        text: String::new(),
        offsets: vec![start],
    };
    let mut chars = text.char_indices();
    while let Some((index, character)) = chars.next() {
        if character == '\\' {
            if let Some((next, escaped)) = chars.next() {
                append_escape(&mut decoded, text, start, index, next, escaped);
                continue;
            }
        }
        decoded.text.push(character);
        decoded
            .offsets
            .extend((1..=character.len_utf8()).map(|byte| start + index + byte));
    }
    decoded
}

fn append_escape(
    decoded: &mut Decoded,
    text: &str,
    start: usize,
    index: usize,
    next: usize,
    escaped: char,
) {
    let character = match escaped {
        'n' => Some('\n'),
        'r' => Some('\r'),
        't' => Some('\t'),
        'e' => Some('\u{1b}'),
        '0' => Some('\0'),
        '"' => Some('"'),
        '\\' => Some('\\'),
        _ => None,
    };
    let end = next + escaped.len_utf8();
    if let Some(character) = character {
        decoded.text.push(character);
        decoded.offsets.push(start + end);
    } else if let Some(raw) = text.get(index..end) {
        decoded.text.push_str(raw);
        decoded
            .offsets
            .extend((index + 1..=end).map(|byte| start + byte));
    }
}

/// Decode plain strings with the same semantics as interpolation.
pub(crate) fn unquote(raw: &str) -> String {
    decode(raw).text
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
        let parts = lower_interpolation("\"v ${1 + 2} end\"", None, Flavor::Default, frag);
        assert_eq!(parts.len(), 3);
        assert!(matches!(parts[0], InterpolatedPart::Text(ref t) if t == "v "));
        assert!(
            matches!(parts[1], InterpolatedPart::Expr(Expr::Identifier(ref e)) if e == "1 + 2")
        );
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
                Flavor::Default => before.len(),
                Flavor::Ml => before.chars().count(),
            };
            assert_eq!(
                map.map_position(
                    Position {
                        line: 1,
                        column: u32::try_from(prefix + beta)
                            .unwrap_or_else(|error| panic!("{error}"))
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
        let multiline = fragments("\"${first\nsecond}\"", Position { line: 2, column: 3 });
        assert_eq!(
            multiline[0].map_position(Position { line: 2, column: 0 }, 14, Flavor::Default),
            Some(Position { line: 3, column: 0 })
        );
    }
}
