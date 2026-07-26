//! The ML-flavor formatter: indentation-significant, layout-driven.
//!
//! Unlike the Default flavor, ML indentation *is* the block structure — the
//! lexer turns column changes into Indent/Dedent tokens — so the formatter must
//! not invent or flatten nesting. Instead it re-grids the existing indentation
//! onto a clean four-space step: a stack of the source's own indent columns maps
//! each line to a depth, preserving exactly which lines are deeper, shallower, or
//! siblings. Blank and comment lines are layout-transparent; a standalone
//! comment takes the depth of the code it precedes.

use crate::scan::{scan_line, Line};
use crate::{finalize, indent_to};

/// Reformat ML-flavor `src`, re-gridding its layout to a four-space step.
pub(crate) fn format(src: &str) -> String {
    let lines: Vec<Line> = src.split('\n').map(scan_line).collect();
    let depths = code_depths(&lines);
    let out = render(&lines, &depths);
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

/// Map every code line to its nesting depth by walking a stack of the indent
/// columns seen so far; blank and comment lines get `None` (resolved later).
fn code_depths(lines: &[Line]) -> Vec<Option<i32>> {
    let mut stack: Vec<usize> = vec![0];
    lines
        .iter()
        .map(|line| {
            if line.is_blank() || line.is_comment_only() {
                return None;
            }
            push_column(&mut stack, line.leading_ws);
            i32::try_from(stack.len().saturating_sub(1)).ok()
        })
        .collect()
}

/// Adjust the indent stack for a code line at column `col`: pop levels deeper
/// than it, then push it as a new level when it is deeper than the current top.
/// The base level (column 0) is never popped, so depth never goes negative.
fn push_column(stack: &mut Vec<usize>, col: usize) {
    while stack.len() > 1 && stack.last().is_some_and(|top| col < *top) {
        let _ = stack.pop();
    }
    if stack.last().is_some_and(|top| col > *top) {
        stack.push(col);
    }
}

/// Emit each line at its depth: code lines use their computed depth; comments
/// borrow the depth of the next code line (or the previous one at end of file).
fn render(lines: &[Line], depths: &[Option<i32>]) -> Vec<String> {
    lines
        .iter()
        .enumerate()
        .map(|(idx, line)| {
            if line.is_blank() {
                return String::new();
            }
            let depth = depths
                .get(idx)
                .copied()
                .flatten()
                .unwrap_or_else(|| comment_depth(depths, idx));
            indent_to(depth) + &line.content
        })
        .collect()
}

/// The depth a standalone comment should adopt: the next code line's depth, or
/// the previous code line's depth when the comment trails the file.
fn comment_depth(depths: &[Option<i32>], idx: usize) -> i32 {
    let after = depths.iter().skip(idx + 1).find_map(|d| *d);
    let before = || depths.iter().take(idx).rev().find_map(|d| *d);
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
