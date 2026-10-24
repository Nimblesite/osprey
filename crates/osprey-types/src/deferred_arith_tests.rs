//! Deferred-arithmetic overload resolution.
//!
//! `abs(a - b) <= atol + rtol * abs(want)` was rejected with four bare
//! "cannot unify float with int": the callee `abs` records its obligation
//! before the argument that feeds it is inferred, so the `abs` site settled
//! first, defaulted the shared operand to int, and every float use of it
//! failed. A deferred site may only settle once something decides it — an
//! operand, or a result another use already pinned — and the integer default
//! is the last resort, not the first guess. Implements [ARITH-EFFECT],
//! [FLOAT-OPERANDS].

use crate::testutil::{accepts, typecheck};
use osprey_syntax::Flavor;

/// The tolerance helper from the ports: every operand is float, so the whole
/// helper is float, whichever order the sites settle in.
#[test]
fn a_tolerance_helper_of_abs_and_products_is_all_float() {
    accepts(
        Flavor::Default,
        "fn near(got, want, rtol, atol) = abs(got - want) <= atol + rtol * abs(want)\n\
         print(toString(near(1.0, 1.0, 0.1, 0.1)))",
    );
    accepts(
        Flavor::Ml,
        "near got want rtol atol = abs (got - want) <= atol + rtol * abs want\n\
         print (toString (near 1.0 1.0 0.1 0.1))",
    );
}

/// The minimized form: `abs` over a deferred subtraction, compared with a
/// deferred product on a bare parameter.
#[test]
fn an_abs_site_waits_for_the_subtraction_feeding_it() {
    accepts(
        Flavor::Default,
        "fn f(a, b, d) = abs(a - b) <= d * b\nprint(toString(f(1.0, 2.0, 3.0)))",
    );
}

/// A float literal the result is compared with decides open operands even
/// when no call site ever pins them: the helper is float, not the integer
/// default. `lonely` is never called, so only the literal can decide it.
#[test]
fn a_compared_float_literal_decides_open_operands() {
    accepts(
        Flavor::Default,
        "fn f(a, b) = (a - b) <= 1.0\nprint(toString(f(1.5, 0.5)))",
    );
    accepts(
        Flavor::Default,
        "fn f(a, b) = abs(a - b) <= 1.0\nprint(toString(f(1.5, 0.5)))",
    );
    accepts(Flavor::Default, "fn lonely(a, b) = (a - b) <= 1.0");
    accepts(Flavor::Default, "fn lonely(a, b) = abs(a - b) <= 1.0");
}

/// An integer call keeps the integer overload — and its `Arith.overflow`
/// request — exactly as before.
#[test]
fn an_integer_call_keeps_the_integer_overload() {
    let errors = typecheck(
        Flavor::Default,
        "fn f(a, b, d) = abs(a - b) <= d * b\nprint(toString(f(1, 2, 3)))",
    );
    assert!(
        errors.iter().all(|e| e.message.contains("Arith.overflow")),
        "{errors:?}"
    );
}

/// One definition still gets ONE overload: the same helper used at int and
/// float in a single program is a type error, not a silent reinterpretation.
#[test]
fn one_helper_still_gets_one_overload() {
    let errors = typecheck(
        Flavor::Default,
        "fn plus(a, b) = a + b\nprint(toString(plus(1, 2)))\nprint(toString(plus(1.5, 2.5)))",
    );
    assert!(
        errors.iter().any(|e| e.message.contains("cannot unify")),
        "{errors:?}"
    );
}

#[test]
fn collection_addition_rejects_incompatible_elements_and_depths() {
    for (flavor, source) in [
        (Flavor::Default, "let mixed = [1] + [[2]]\n"),
        (Flavor::Default, "let mixed = {\"a\": 1} + {\"b\": \"two\"}\n"),
        (Flavor::Python, "mixed = [1] + [[2]]\n"),
        (Flavor::Python, "mixed = {\"a\": 1} + {\"b\": \"two\"}\n"),
    ] {
        let errors = typecheck(flavor, source);
        assert!(errors.iter().any(|error| error.message.contains("cannot unify")),
            "incompatible collection layouts must fail before codegen: {flavor:?} {source}: {errors:?}");
    }
}
