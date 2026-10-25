//! `finally`, `mask` and `instance` parse to the same AST in both flavors.
//! Implements [EFFECTS-FINALIZATION], [EFFECTS-MASK], [EFFECTS-INSTANCE] and
//! [FLAVOR-ML-EFFECT-SCOPE].

use osprey_syntax::{parse_program_with_flavor, Flavor};

fn canonical(source: &str, flavor: Flavor) -> String {
    let parsed = parse_program_with_flavor(source, flavor);
    assert!(
        parsed.errors.is_empty(),
        "{flavor} syntax errors: {:?}",
        parsed.errors
    );
    osprey_ast::canonical::without_positions(&parsed.program)
}

fn rejection(source: &str, flavor: Flavor) -> String {
    let parsed = parse_program_with_flavor(source, flavor);
    parsed
        .errors
        .iter()
        .map(|error| error.message.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

const DEFAULT: &str = concat!(
    "effect Log { info: fn(string) -> Unit }\n",
    "let audit = instance Log\n",
    "let answer = {\n",
    "    handle Log {\n",
    "        info m => print(m)\n",
    "        finally => print(\"closed\")\n",
    "    }\n",
    "    mask Log { perform audit.info(\"one\") }\n",
    "}\n",
);

const ML: &str = concat!(
    "effect Log\n    info : string => Unit\n",
    "audit = instance Log\n",
    "answer =\n",
    "    handle Log\n",
    "        info m => print m\n",
    "        finally => print \"closed\"\n",
    "    mask Log\n",
    "        perform audit.info \"one\"\n",
);

#[test]
fn scope_forms_lower_to_the_same_ast_in_both_flavors() {
    let default = canonical(DEFAULT, Flavor::Default);
    assert!(default.contains("Mask"), "{default}");
    assert!(default.contains("Instance"), "{default}");
    assert_eq!(default, canonical(ML, Flavor::Ml));
}

#[test]
fn an_ml_finally_clause_takes_no_parameter() {
    let source = ML.replace("finally =>", "finally x =>");
    assert!(rejection(&source, Flavor::Ml).contains("one finally clause with no parameter"));
}

#[test]
fn an_ml_mask_needs_an_indented_body() {
    let source = "effect Log\n    info : string => Unit\nanswer = mask Log\n";
    assert!(rejection(source, Flavor::Ml).contains("expected an indented body after `mask Effect`"));
}

#[test]
fn python_comprehension_targets_shadow_builtin_aliases_without_leaking() {
    for (default, python) in [
        (
            "let ys = toList(map(callbacks, fn(len) => len(xs)))\nlet n = length(xs)\n",
            "ys = [len(xs) for len in callbacks]\nn = len(xs)\n",
        ),
        (
            "let ys = toList(map(callbacks, fn(range) => range(7)))\nlet after = range(0, 3)\n",
            "ys = [range(7) for range in callbacks]\nafter = range(3)\n",
        ),
        (
            "let ys = toList(map(filter(callbacks, fn(len) => len(xs) > 0), fn(len) => len(xs)))\n",
            "ys = [len(xs) for len in callbacks if len(xs) > 0]\n",
        ),
        (
            "let ys = toList(map(range(0, 3), fn(range) => range))\n",
            "ys = [range for range in range(3)]\n",
        ),
    ] {
        assert_eq!(
            canonical(python, Flavor::Python),
            canonical(default, Flavor::Default),
            "{python}"
        );
    }
}

#[test]
fn python_nested_comprehension_iterators_see_earlier_targets() {
    let program = canonical(
        "ys = [value for len in callbacks for value in len(xs)]\n",
        Flavor::Python,
    );
    assert!(
        !program.contains("\"length\""),
        "a comprehension callback became the builtin: {program}"
    );
    assert_eq!(program.matches("\"fold\"").count(), 2);
    assert_ne!(
        canonical("ys = [x for x in groups for x in x]\n", Flavor::Python),
        ""
    );
}

#[test]
fn python_comprehension_targets_obey_reserved_binding_checks() {
    for name in ["length", "intDiv"] {
        let source = format!("ys = [{name} for {name} in items]\n");
        assert!(
            rejection(&source, Flavor::Python).contains("shadows the builtin"),
            "accepted {name}"
        );
    }
}
