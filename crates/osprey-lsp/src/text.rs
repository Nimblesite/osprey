//! Position-aware text helpers shared by hover, definition and references.
//!
//! LSP positions are `(line, character)` with `character` counted in the
//! negotiated [`PositionEncoding`]. Osprey identifiers are ASCII, but these
//! helpers honour the encoding so multi-byte text still lines up.
//!
//! These primitives (word-at-position, whole-word occurrences, encoding-aware
//! column measurement) are language-agnostic and belong in `lspkit-vfs`; they
//! live here only until lspkit provides them. Tracked upstream at
//! <https://github.com/Nimblesite/lspkit/issues/2>. See [LSP-REUSE-LSPKIT].

use lspkit_vfs::PositionEncoding;

/// An identifier found under a cursor, with its span in the line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordSpan {
    /// The identifier text.
    pub word: String,
    /// Start character offset within the line (negotiated encoding).
    pub start: u32,
    /// End character offset within the line (negotiated encoding).
    pub end: u32,
}

/// A whole-word occurrence of an identifier within a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Occurrence {
    /// Zero-based line.
    pub line: u32,
    /// Start character offset within the line (negotiated encoding).
    pub start: u32,
    /// End character offset within the line (negotiated encoding).
    pub end: u32,
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Width of `c` in `encoding`'s character units.
#[must_use]
pub fn char_width(c: char, encoding: PositionEncoding) -> u32 {
    match encoding {
        PositionEncoding::Utf8 => u32::try_from(c.len_utf8()).unwrap_or(1),
        PositionEncoding::Utf16 => u32::try_from(c.len_utf16()).unwrap_or(1),
        // UTF-32 counts code points; `PositionEncoding` is `#[non_exhaustive]`,
        // so anything new also counts as one unit per `char`.
        _ => 1,
    }
}

/// Length of `s` in `encoding`'s character units.
#[must_use]
pub fn measure(s: &str, encoding: PositionEncoding) -> u32 {
    s.chars().map(|c| char_width(c, encoding)).sum()
}

/// The prefix of `line` up to (but excluding) character offset `character`.
#[must_use]
pub fn prefix_to(line: &str, character: u32, encoding: PositionEncoding) -> &str {
    let mut offset = 0u32;
    for (idx, c) in line.char_indices() {
        if offset >= character {
            return line.get(..idx).unwrap_or(line);
        }
        offset = offset.saturating_add(char_width(c, encoding));
    }
    line
}

/// The identifier under `character` within `line`, or `None` if the cursor is
/// not over an identifier character.
#[must_use]
pub fn word_at(line: &str, character: u32, encoding: PositionEncoding) -> Option<WordSpan> {
    let mut offset = 0u32;
    let mut run_start = 0u32;
    let mut run = String::new();
    let mut found: Option<WordSpan> = None;
    for c in line.chars() {
        let w = char_width(c, encoding);
        if is_ident(c) {
            if run.is_empty() {
                run_start = offset;
            }
            run.push(c);
        } else {
            found = found.or_else(|| take_if_covers(&run, run_start, offset, character));
            run.clear();
        }
        offset = offset.saturating_add(w);
    }
    found.or_else(|| take_if_covers(&run, run_start, offset, character))
}

/// The identifier or `::`-qualified symbol path under `character`.
///
/// A single `:` remains punctuation (type annotation / named argument); only
/// paired separators join path segments, so `.` continues to mean value field
/// access. [MODULES-FLAVOR-PROJECTION]
#[must_use]
pub fn path_at(line: &str, character: u32, encoding: PositionEncoding) -> Option<WordSpan> {
    line_paths(line, encoding)
        .into_iter()
        .find(|span| character >= span.start && character <= span.end)
}

fn line_paths(line: &str, encoding: PositionEncoding) -> Vec<WordSpan> {
    let chars: Vec<char> = line.chars().collect();
    let mut offsets = Vec::with_capacity(chars.len().saturating_add(1));
    let mut offset = 0u32;
    offsets.push(offset);
    for character in &chars {
        offset = offset.saturating_add(char_width(*character, encoding));
        offsets.push(offset);
    }
    let mut paths = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        if !chars.get(index).is_some_and(|c| is_ident(*c)) {
            index += 1;
            continue;
        }
        let start = index;
        index = consume_ident(&chars, index);
        while chars.get(index) == Some(&':')
            && chars.get(index + 1) == Some(&':')
            && chars.get(index + 2).is_some_and(|c| is_ident(*c))
        {
            index = consume_ident(&chars, index + 2);
        }
        let Some(word) = chars.get(start..index) else {
            continue;
        };
        paths.push(WordSpan {
            word: word.iter().collect(),
            start: offsets.get(start).copied().unwrap_or(0),
            end: offsets.get(index).copied().unwrap_or(offset),
        });
    }
    paths
}

fn consume_ident(chars: &[char], mut index: usize) -> usize {
    while chars.get(index).is_some_and(|c| is_ident(*c)) {
        index += 1;
    }
    index
}

/// Promote an accumulated identifier run to a [`WordSpan`] when `character`
/// falls within `[start, end]` (inclusive of the trailing edge so a cursor
/// resting just after the word still resolves it).
fn take_if_covers(run: &str, start: u32, end: u32, character: u32) -> Option<WordSpan> {
    if run.is_empty() || character < start || character > end {
        return None;
    }
    Some(WordSpan {
        word: run.to_owned(),
        start,
        end,
    })
}

/// Every whole-word occurrence of `name` across `text`.
#[must_use]
pub fn occurrences(text: &str, name: &str, encoding: PositionEncoding) -> Vec<Occurrence> {
    if name.contains("::") {
        return text
            .lines()
            .enumerate()
            .flat_map(|(line, source)| {
                let line = u32::try_from(line).unwrap_or(u32::MAX);
                line_paths(source, encoding)
                    .into_iter()
                    .filter(move |span| span.word == name)
                    .map(move |span| Occurrence {
                        line,
                        start: span.start,
                        end: span.end,
                    })
            })
            .collect();
    }
    text.lines()
        .enumerate()
        .flat_map(|(idx, line)| line_occurrences(line, idx, name, encoding))
        .collect()
}

fn line_occurrences(
    line: &str,
    line_idx: usize,
    name: &str,
    enc: PositionEncoding,
) -> Vec<Occurrence> {
    let line_no = u32::try_from(line_idx).unwrap_or(u32::MAX);
    let mut offset = 0u32;
    let mut run_start = 0u32;
    let mut run = String::new();
    let mut out = Vec::new();
    for c in line.chars() {
        let w = char_width(c, enc);
        if is_ident(c) {
            if run.is_empty() {
                run_start = offset;
            }
            run.push(c);
        } else {
            push_match(&mut out, &run, run_start, offset, line_no, name);
            run.clear();
        }
        offset = offset.saturating_add(w);
    }
    push_match(&mut out, &run, run_start, offset, line_no, name);
    out
}

fn push_match(out: &mut Vec<Occurrence>, run: &str, start: u32, end: u32, line: u32, name: &str) {
    if run == name {
        out.push(Occurrence { line, start, end });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const U16: PositionEncoding = PositionEncoding::Utf16;

    #[test]
    fn word_at_finds_identifier_under_and_after_cursor() {
        let line = "let total = a + b";
        assert_eq!(
            word_at(line, 5, U16).map(|w| w.word),
            Some("total".to_owned())
        );
        // Trailing edge of the word still resolves it.
        assert_eq!(
            word_at(line, 9, U16).map(|w| w.word),
            Some("total".to_owned())
        );
        assert_eq!(word_at(line, 16, U16).map(|w| w.word), Some("b".to_owned()));
    }

    #[test]
    fn word_at_returns_none_off_identifier() {
        assert_eq!(word_at("a + b", 2, U16), None);
        assert_eq!(word_at("", 0, U16), None);
    }

    #[test]
    fn occurrences_are_whole_word_only() {
        let text = "fn add(a) = a\nlet adder = add(adding)\n";
        let hits = occurrences(text, "add", U16);
        let lines: Vec<u32> = hits.iter().map(|h| h.line).collect();
        assert_eq!(lines, vec![0, 1], "{hits:?}");
    }

    #[test]
    fn char_width_and_measure_follow_the_negotiated_encoding() {
        // An emoji is one UTF-16 surrogate pair (2 units) but four UTF-8 bytes.
        let rocket = '🚀';
        assert_eq!(char_width(rocket, PositionEncoding::Utf16), 2);
        assert_eq!(char_width(rocket, PositionEncoding::Utf8), 4);
        // A `é` is one UTF-16 unit and two UTF-8 bytes.
        assert_eq!(char_width('é', PositionEncoding::Utf16), 1);
        assert_eq!(char_width('é', PositionEncoding::Utf8), 2);
        // `measure` sums the per-char widths in the chosen encoding.
        assert_eq!(measure("aé🚀", PositionEncoding::Utf8), 1 + 2 + 4);
        assert_eq!(measure("aé🚀", PositionEncoding::Utf16), 1 + 1 + 2);
    }

    #[test]
    fn prefix_to_clamps_to_the_full_line_and_respects_utf8_widths() {
        // A character offset past the end returns the whole line unchanged.
        assert_eq!(prefix_to("let x", 100, U16), "let x");
        // In UTF-8 the prefix boundary is measured in bytes: `é` is two units, so
        // offset 3 lands just after `aé`.
        assert_eq!(prefix_to("aébc", 3, PositionEncoding::Utf8), "aé");
        // Offset zero returns an empty prefix.
        assert_eq!(prefix_to("abc", 0, U16), "");
    }

    #[test]
    fn word_at_resolves_multibyte_identifiers_under_utf8_offsets() {
        // `total` after a leading multi-byte char: in UTF-8 the word starts at
        // byte offset 3 (after `é` = 2 bytes plus the space).
        let line = "é total";
        let span = word_at(line, 4, PositionEncoding::Utf8).expect("word");
        assert_eq!(span.word, "total");
        assert_eq!(span.start, 3);
    }

    #[test]
    fn path_at_keeps_double_colons_and_stops_at_field_dots() {
        // [MODULES-FLAVOR-PROJECTION] `::` is qualification; `.` remains value
        // field access and a single `:` remains an annotation separator.
        let line = "billing::Tax::addTax(invoice.total)";
        let path = path_at(line, 10, U16).expect("qualified path");
        assert_eq!(path.word, "billing::Tax::addTax");
        assert_eq!(path.start, 0);
        assert_eq!(path.end, 20);
        assert_eq!(
            path_at(line, 29, U16).map(|span| span.word),
            Some("total".into())
        );
        assert_eq!(
            path_at("x: int", 3, U16).map(|span| span.word),
            Some("int".into())
        );
    }

    #[test]
    fn occurrences_match_a_qualified_path_as_one_symbol() {
        // [MODULES-ABI] Qualified navigation must not degrade into three
        // unrelated whole-word hits.
        let text = "billing::Tax::addTax(1)\nTax::addTax(2)\nbilling::Tax::addTax(3)\n";
        let hits = occurrences(text, "billing::Tax::addTax", U16);
        assert_eq!(hits.len(), 2, "{hits:?}");
        assert_eq!(hits.get(1).map(|hit| hit.line), Some(2));
    }
}
