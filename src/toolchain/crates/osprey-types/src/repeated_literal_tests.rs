//! Assertions for [TYPE-WARNINGS-REPEATED-LITERAL]: a string or list literal of 12 or
//! more characters written 3 or more times in one file is a warning, in
//! every flavor, once per literal, at its first use.

use crate::{repeated_literals, REPEATED_LITERAL};
use osprey_syntax::{parse_program_with_flavor, Flavor};

fn warned(flavor: Flavor, source: &str) -> Vec<(String, u32)> {
    let parsed = parse_program_with_flavor(source, flavor);
    assert!(parsed.errors.is_empty(), "{source}\n{:?}", parsed.errors);
    repeated_literals(&parsed.program)
        .into_iter()
        .map(|warning| {
            assert_eq!(warning.rule, REPEATED_LITERAL);
            let line = warning.position.map_or(0, |position| position.line);
            (warning.message, line)
        })
        .collect()
}

#[test]
fn a_long_string_written_three_times_names_a_constant_once() {
    let source = "fn a() = \"connection refused\"\nfn b() = \"connection refused\"\nfn c() = \"connection refused\"\nfn d() = \"connection refused\"\n";
    assert_eq!(
        warned(Flavor::Default, source),
        [(
            "the literal `\"connection refused\"` is written 4 times in this file; name it once as a constant".to_owned(),
            1
        )]
    );
}

#[test]
fn short_rare_and_distinct_literals_are_quiet() {
    // 11 characters is below the threshold; two uses are below the count;
    // literals that differ by one character are different literals; a
    // pattern literal is not an expression and cannot be a constant.
    let source = "fn a() = \"eleven char\"\nfn b() = \"eleven char\"\nfn c() = \"eleven char\"\nfn d() = \"written twice only\"\nfn e() = \"written twice only\"\nfn f() = \"almost the same 1\"\nfn g() = \"almost the same 2\"\nfn h() = \"almost the same 3\"\nfn k(s) = match s { \"pattern literal!\" => 1 \"pattern literal?\" => 2 _ => 0 }\nfn m(s) = match s { \"pattern literal!\" => 1 _ => 0 }\nfn n(s) = match s { \"pattern literal!\" => 1 _ => 0 }\n";
    assert_eq!(warned(Flavor::Default, source), Vec::<(String, u32)>::new());
}

#[test]
fn a_repeated_list_literal_warns_beside_its_strings() {
    let source = "fn a() = [100, 200, 300, 400]\nfn b() = [100, 200, 300, 400]\nfn c() = [100, 200, 300, 400]\nfn d() = [\"left-column\", \"right-column\"]\nfn e() = [\"left-column\", \"right-column\"]\nfn f() = [\"left-column\", \"right-column\"]\nfn g() = [x, 2, 3, 4, 5, 6]\nfn h() = [x, 2, 3, 4, 5, 6]\nfn i() = [x, 2, 3, 4, 5, 6]\n";
    assert_eq!(
        warned(Flavor::Default, source),
        [
            ("the literal `[100, 200, 300, 400]` is written 3 times in this file; name it once as a constant".to_owned(), 1),
            ("the literal `[\"left-column\", \"right-column\"]` is written 3 times in this file; name it once as a constant".to_owned(), 4),
            ("the literal `\"right-column\"` is written 3 times in this file; name it once as a constant".to_owned(), 4),
        ]
    );
}

#[test]
fn ml_and_python_report_the_same_literal() {
    let ml = "a () = \"connection refused\"\nb () = \"connection refused\"\nc () = \"connection refused\"\n";
    let python = "# osprey: flavor=python\ndef a():\n    return \"connection refused\"\n\n\ndef b():\n    return \"connection refused\"\n\n\ndef c():\n    return \"connection refused\"\n";
    let expected = "the literal `\"connection refused\"` is written 3 times in this file; name it once as a constant".to_owned();
    assert_eq!(warned(Flavor::Ml, ml), [(expected.clone(), 1)]);
    assert_eq!(warned(Flavor::Python, python), [(expected, 2)]);
}

#[test]
fn a_very_long_literal_is_shortened_in_the_message() {
    let text = "a sentence long enough that the warning must not quote all of it";
    let source = format!("fn a() = \"{text}\"\nfn b() = \"{text}\"\nfn c() = \"{text}\"\n");
    assert_eq!(
        warned(Flavor::Default, &source),
        [(
            "the literal `\"a sentence long enough that the…` is written 3 times in this file; name it once as a constant".to_owned(),
            1
        )]
    );
}
