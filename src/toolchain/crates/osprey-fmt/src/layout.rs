//! The ML-flavor formatter: indentation-significant, layout-driven.
//!
//! Unlike the Default flavor, ML indentation *is* the block structure — the
//! lexer turns column changes into Indent/Dedent tokens — so the formatter must
//! not invent or flatten nesting. It renders the lexer's actual layout depths
//! with four spaces per level. A standalone comment takes the depth of the
//! code it precedes.

use crate::scan::{scan_source, Comments, Line};
use crate::{finalize, indent_to};

/// Reformat ML-flavor `src`, re-gridding its layout to a four-space step.
pub(crate) fn format(src: &str) -> String {
    let lines: Vec<Line> = scan_source(src, Comments::ml());
    let layout = code_layout(src, &lines);
    let out = render(&lines, &layout);
    finalize(&out)
}

/// Recover from a block suffix that was accidentally dedented by Format
/// Document's caller. Each syntax-error line is tried as the start of the
/// suffix; a repair is accepted only when adding one layout level makes the
/// complete document parse again.
pub(crate) fn repair_dedented_suffix(
    src: &str,
    errors: &[osprey_syntax::SyntaxError],
) -> Option<String> {
    let lines: Vec<&str> = src.split('\n').collect();
    for start in errors.iter().filter_map(|error| {
        error
            .position
            .line
            .checked_sub(1)
            .and_then(|line| usize::try_from(line).ok())
    }) {
        let repaired = lines
            .iter()
            .enumerate()
            .map(|(idx, line)| {
                if idx >= start && !line.is_empty() {
                    format!("{}{}", " ".repeat(crate::INDENT_WIDTH), line)
                } else {
                    (*line).to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let parsed = osprey_syntax::parse_program_with_flavor(&repaired, crate::Flavor::Ml);
        if parsed.errors.is_empty() {
            return Some(repaired);
        }
    }
    None
}

/// Match the physical layout to scanned lines, which can span a string literal.
fn code_layout<'a>(src: &'a str, lines: &[Line]) -> Vec<(Option<i32>, Option<&'a str>)> {
    let layout = osprey_syntax::ml_line_layout(src).into_iter();
    let mut physical = src
        .split('\n')
        .zip(layout.chain(std::iter::repeat((None, false))));
    lines
        .iter()
        .map(|line| {
            let (raw, (depth, in_doc)) = physical.next().unwrap_or_default();
            let count = line.content.split('\n').count();
            for _ in 1..count {
                let _ = physical.next();
            }
            (depth, (in_doc && count == 1).then_some(raw))
        })
        .collect()
}

/// Emit each line at its depth: code lines use their computed depth; comments
/// borrow the depth of the next code line (or the previous one at end of file).
fn render(lines: &[Line], layout: &[(Option<i32>, Option<&str>)]) -> Vec<String> {
    lines
        .iter()
        .enumerate()
        .map(|(idx, line)| {
            if line.is_blank() {
                return String::new();
            }
            let (depth, original) = layout.get(idx).copied().unwrap_or_default();
            // A block doc comment can include a fenced example with meaningful whitespace.
            original.map_or_else(
                || indent_to(depth.unwrap_or_else(|| comment_depth(layout, idx))) + &line.content,
                str::to_owned,
            )
        })
        .collect()
}

/// The depth a standalone comment should adopt: the next code line's depth, or
/// the previous code line's depth when the comment trails the file.
fn comment_depth(layout: &[(Option<i32>, Option<&str>)], idx: usize) -> i32 {
    let after = layout.iter().skip(idx + 1).find_map(|(d, _)| *d);
    let before = || layout.iter().take(idx).rev().find_map(|(d, _)| *d);
    after.or_else(before).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regrids_layout_to_four_space_steps() {
        let src = "main () =\n  a = 1\n  b = 2\n";
        let want = "main () =\n    a = 1\n    b = 2\n";
        assert_eq!(format(src), want);
    }

    #[test]
    fn deeper_nesting_steps_one_level_per_indent() {
        let src = "f () =\n  match x\n      A => 1\n      B => 2\n";
        let want = "f () =\n    match x\n        A => 1\n        B => 2\n";
        assert_eq!(format(src), want);
    }

    #[test]
    fn siblings_at_equal_columns_stay_siblings() {
        let src = "outer\n   one\n   two\n   three\n";
        let want = "outer\n    one\n    two\n    three\n";
        assert_eq!(format(src), want);
    }

    #[test]
    fn standalone_comment_takes_following_code_depth() {
        let src = "f () =\n      // note\n      x = 1\n";
        let want = "f () =\n    // note\n    x = 1\n";
        assert_eq!(format(src), want);
    }

    #[test]
    fn is_idempotent() {
        let src = "main () =\n        a = 1\n   nested =\n             deep\n";
        let once = format(src);
        assert_eq!(format(&once), once);
    }
}
