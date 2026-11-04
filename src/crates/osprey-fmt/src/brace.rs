//! The Default-flavor formatter: C-style, brace-driven indentation.
//!
//! The Default flavor delimits blocks with `{ … }` and is whitespace-insensitive,
//! so indentation is purely cosmetic and is recomputed from bracket nesting: a
//! line sits one level deeper for every unclosed `{`/`(`/`[` above it, and a line
//! that *begins* by closing brackets dedents to match its opener. Comments adopt
//! the depth of the surrounding block.

use crate::scan::{scan_source, Line};
use crate::{finalize, indent_to};

/// Reformat Default-flavor `src`, returning the canonical, reindented text.
pub(crate) fn format(src: &str) -> String {
    let lines: Vec<Line> = scan_source(src);
    let mut depth = 0i32;
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    for line in &lines {
        if line.is_blank() {
            out.push(String::new());
            continue;
        }
        let here = (depth - line.leading_closers).max(0);
        out.push(indent_to(here) + &line.content);
        depth = (depth + line.open_delta).max(0);
    }
    finalize(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reindents_a_braced_function_to_four_spaces() {
        let src = "fn main() = {\nprint(1)\n}\n";
        assert_eq!(format(src), "fn main() = {\n    print(1)\n}\n");
    }

    #[test]
    fn over_indented_input_is_normalised_down() {
        let src = "fn classify(n) = match n {\n            0 => \"zero\"\n     _ => \"many\"\n}\n";
        let want = "fn classify(n) = match n {\n    0 => \"zero\"\n    _ => \"many\"\n}\n";
        assert_eq!(format(src), want);
    }

    #[test]
    fn nested_blocks_step_by_one_level_each() {
        let src = "fn f() = {\nmatch x {\nA => {\ng()\n}\n}\n}\n";
        let want =
            "fn f() = {\n    match x {\n        A => {\n            g()\n        }\n    }\n}\n";
        assert_eq!(format(src), want);
    }

    #[test]
    fn is_idempotent() {
        let src = "fn main() = {\n        print( 1 )\n   }\n";
        let once = format(src);
        assert_eq!(format(&once), once);
    }

    /// A literal written across several lines carries its own newlines and
    /// interior spacing as *data*. Reindenting them rewrote what the program
    /// printed — caught only because the reparse guard is loud.
    #[test]
    fn a_multi_line_string_literal_is_not_reindented() {
        let src = "fn f() = {\nprint(\"one\n  two  two\nthree\")\nx\n}\n";
        let want = "fn f() = {\n    print(\"one\n  two  two\nthree\")\n    x\n}\n";
        assert_eq!(format(src), want);
        assert_eq!(format(&format(src)), format(src), "not idempotent");
    }

    #[test]
    fn brackets_inside_strings_do_not_shift_indentation() {
        let src = "fn f() = {\nprint(\"}{}{\")\nx\n}\n";
        let want = "fn f() = {\n    print(\"}{}{\")\n    x\n}\n";
        assert_eq!(format(src), want);
    }
}
