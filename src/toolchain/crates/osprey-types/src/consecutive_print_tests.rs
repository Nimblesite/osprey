//! Assertions for [TYPE-WARNINGS-CONSECUTIVE-PRINT]: two or more `print`
//! calls in a row, at the top level or in any block, are one warning at the
//! first of them, in every flavor.

use crate::{consecutive_prints, CONSECUTIVE_PRINT};
use osprey_syntax::Flavor;

fn warned(flavor: Flavor, source: &str) -> Vec<(String, u32)> {
    crate::lint_test_support::warned(flavor, source, consecutive_prints, CONSECUTIVE_PRINT)
}

fn joined(count: usize) -> String {
    format!("{count} `print` calls in a row: join them into one interpolated `print`")
}

#[test]
fn prints_in_a_row_are_one_warning_per_run_in_every_block() {
    let source = "let a = 1\nprint(\"a\")\nprint(\"b\")\nlet b = 2\nprint(\"c\")\nfn show(x) = {\n    print(\"x\")\n    print(\"y\")\n    print(\"z\")\n    let y = x\n    print(\"${y}\")\n}\nfn tail(x) = {\n    print(\"head\")\n    print(\"value ${x}\")\n}\nshow(b)\ntail(a)\n";
    assert_eq!(
        warned(Flavor::Default, source),
        [(joined(2), 2), (joined(3), 7), (joined(2), 14)]
    );
}

#[test]
fn a_single_interpolated_print_is_quiet() {
    let source = "let a = 1\nprint(\"a=${a}\")\nlet b = a\nprint(\"b=${b}\")\nfn show(x) = print(\"x=${x}\")\nshow(b)\n";
    assert_eq!(warned(Flavor::Default, source), Vec::<(String, u32)>::new());
}

#[test]
fn ml_and_python_prints_in_a_row_are_reported_too() {
    let ml = "print \"a\"\nprint \"b\"\nx = 1\nprint \"${x}\"\n";
    let python = "# osprey: flavor=python\nprint(\"a\")\nprint(\"b\")\nprint(\"c\")\n";
    assert_eq!(warned(Flavor::Ml, ml), [(joined(2), 1)]);
    assert_eq!(warned(Flavor::Python, python), [(joined(3), 2)]);
}
