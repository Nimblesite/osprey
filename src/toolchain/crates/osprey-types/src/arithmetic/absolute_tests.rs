use crate::testutil::{accepts, typecheck};
use osprey_syntax::Flavor;

const INVALID_DEFAULT: &[&str] = &[
    "let bad = abs(true)",
    "let bad = abs([1])",
    "let bad = abs(Success { value: 1 })",
    "let f = abs\nlet bad = f(\"text\")",
    "fn apply(f, x) = f(x)\nlet bad = apply(abs, true)",
];
const INVALID_ML: &[&str] = &[
    "bad = abs true",
    "bad = abs [1]",
    "bad = abs (Success(value = 1))",
    "f = abs\nbad = f \"text\"",
    "apply (f, x) = f x\nbad = apply (abs, true)",
];

#[test]
fn absolute_rejects_nonnumeric_values_without_fabricating_effect_errors() {
    for (flavor, sources) in [(Flavor::Default, INVALID_DEFAULT), (Flavor::Ml, INVALID_ML)] {
        for source in sources {
            let errors = typecheck(flavor, source);
            assert_eq!(errors.len(), 1, "{flavor:?}: {source}: {errors:?}");
            assert!(
                errors.iter().all(|error| error
                    .message
                    .starts_with("operator `abs` requires int or float")),
                "{errors:?}"
            );
        }
    }
}

#[test]
fn absolute_float_values_are_total_and_integer_values_still_require_policy() {
    for (flavor, total, fallible) in [
        (
            Flavor::Default,
            "let magnitude = abs\nlet x = magnitude(-1.5)",
            "let magnitude = abs\nlet x = magnitude(-9223372036854775808)",
        ),
        (
            Flavor::Ml,
            "magnitude = abs\nx = magnitude (-1.5)",
            "magnitude = abs\nx = magnitude (-9223372036854775808)",
        ),
    ] {
        accepts(flavor, total);
        let errors = typecheck(flavor, fallible);
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("file-scope initializer")),
            "{errors:?}"
        );
        assert!(
            errors.iter().any(|error| error
                .message
                .contains("unhandled effect operations at program entry: Arith.overflow")),
            "{errors:?}"
        );
    }
}
