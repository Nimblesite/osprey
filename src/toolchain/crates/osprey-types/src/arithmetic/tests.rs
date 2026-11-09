use crate::testutil::typecheck;
use osprey_ast::Position;
use osprey_syntax::Flavor;

/// [ARITH-EFFECT-CONST] One overflowing fold is one located diagnostic,
/// however many constant expressions enclose it.
#[test]
fn a_nested_constant_overflow_is_reported_once_at_its_operator() {
    for (flavor, source, column) in [
        (
            Flavor::Default,
            "print(((9223372036854775807 + 1) * 2) - 3)\n",
            28,
        ),
        (
            Flavor::Ml,
            "print (((9223372036854775807 + 1) * 2) - 3)\n",
            29,
        ),
    ] {
        let errors = typecheck(flavor, source);
        let overflow: Vec<_> = errors
            .iter()
            .filter(|e| e.message == "constant arithmetic overflows: 9223372036854775807 + 1")
            .collect();
        assert_eq!(overflow.len(), 1, "{flavor}: {errors:?}");
        assert_eq!(
            overflow.first().and_then(|e| e.position),
            Some(Position { line: 1, column }),
            "{flavor}: {errors:?}"
        );
    }
}
