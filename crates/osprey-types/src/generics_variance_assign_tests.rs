//! Spec-driven assertions for assignment sites under declared variance
//! ([TYPE-VARIANCE-ASSIGN], [TYPE-VARIANCE-COERCION],
//! docs/specs/0004-TypeSystem.md).
//!
//! `generics_variance_tests.rs` states where a marker may be WRITTEN. This
//! module states what writing it does to ASSIGNMENT — and the answer the spec
//! now gives explicitly is: nothing, today. The language's only coercion
//! (`T -> Result<T, E>`, an implicit `Success`) changes the value's
//! representation, and nothing rebuilds a container's contents, so it is barred
//! from argument positions. With no other subtyping in the language, `out T`,
//! `in T` and an unannotated parameter accept and refuse exactly the same
//! programs.
//!
//! That is a claim worth pinning precisely because it is invisible: the two
//! shipped fixtures (`variance_covariant_result_payload.ospo` and
//! `variance_invariant_arg_mismatch.ospo`) assert the one direction all three
//! markers already agree on, so nothing in the tree separated them, and nothing
//! would have noticed a checker that quietly started coercing through argument
//! positions and silently misread a payload. Every claim is made at ALL THREE
//! sites the spec names, because a rule enforced at one site is not the rule.

use crate::testutil::{accepts, plain_cases, rejects, spec_cases};
use osprey_syntax::Flavor;

/// The three assignment sites [TYPE-VARIANCE-ASSIGN] names, for a value of the
/// type `actual` produces flowing into a slot annotated `expected`.
pub(crate) fn sites(decls: &str, expected: &str, actual: &str) -> [String; 3] {
    [
        format!("{decls}fn takes(v: {expected}) = 0\nprint(\"${{takes({actual})}}\")\n"),
        format!("{decls}let held: {expected} = {actual}\nprint(\"bound\")\n"),
        format!("{decls}fn gives() -> {expected} = {actual}\nprint(\"declared\")\n"),
    ]
}

/// Assert the value flows into the slot at every assignment site.
pub(crate) fn flows(decls: &str, expected: &str, actual: &str) {
    for src in sites(decls, expected, actual) {
        accepts(Flavor::Default, &src);
    }
}

/// Assert the value is refused at every assignment site.
pub(crate) fn blocked(decls: &str, expected: &str, actual: &str) {
    for src in sites(decls, expected, actual) {
        rejects(Flavor::Default, &src);
    }
}

/// True when the checker accepts the program outright.
pub(crate) fn accepted(src: &str) -> bool {
    crate::testutil::typecheck(Flavor::Default, src).is_empty()
}

/// A covariant container, plus producers at both instantiations.
const FEED: &str = "type Feed<out T> = Feed { supply: T } | Dry\n\
    fn feedInt() -> Feed<int> = Feed { supply: 1 }\n\
    fn feedRes() -> Feed<Result<int, MathError>> = Feed { supply: 20 * 5 }\n\
    fn feedText() -> Feed<string> = Feed { supply: \"s\" }\n";

/// A contravariant container, same shape.
const GATE: &str = "type Gate<in T> = Gate { admit: (T) -> bool } | Open\n\
    fn gateInt() -> Gate<int> = Gate { admit: |x| => true }\n\
    fn gateRes() -> Gate<Result<int, MathError>> = Gate { admit: |x| => true }\n\
    fn gateText() -> Gate<string> = Gate { admit: |x| => true }\n";

/// An invariant container — the control the other two are measured against.
const CELL: &str = "type Cell<T> = Cell { slot: T }\n\
    fn cellInt() -> Cell<int> = Cell { slot: 1 }\n\
    fn cellRes() -> Cell<Result<int, MathError>> = Cell { slot: 20 * 5 }\n\
    fn cellText() -> Cell<string> = Cell { slot: \"s\" }\n";

/// The containers nested one level inside each other.
const NESTED: &str = "type Feed<out T> = Feed { supply: T } | Dry\n\
    type Gate<in T> = Gate { admit: (T) -> bool } | Open\n\
    type Cell<T> = Cell { slot: T }\n\
    fn feedFeedInt() -> Feed<Feed<int>> = Feed { supply: Feed { supply: 1 } }\n\
    fn feedGateInt() -> Feed<Gate<int>> = Feed { supply: Gate { admit: |x| => true } }\n\
    fn feedGateRes() -> Feed<Gate<Result<int, MathError>>> = \
        Feed { supply: Gate { admit: |x| => true } }\n\
    fn feedCellInt() -> Feed<Cell<int>> = Feed { supply: Cell { slot: 1 } }\n\
    fn gateFeedRes() -> Gate<Feed<Result<int, MathError>>> = Gate { admit: |x| => true }\n\
    fn gateGateInt() -> Gate<Gate<int>> = Gate { admit: |x| => true }\n";

/// A covariant container over FUNCTION payloads.
const FNPAYLOAD: &str = "type Feed<out T> = Feed { supply: T } | Dry\n\
    fn feedTakesInt() -> Feed<(int) -> bool> = Feed { supply: |x| => true }\n\
    fn feedTakesRes() -> Feed<(Result<int, MathError>) -> bool> = \
        Feed { supply: |x| => true }\n\
    fn feedGivesInt() -> Feed<(int) -> int> = Feed { supply: |x| => 1 }\n\
    fn feedGivesRes() -> Feed<(int) -> Result<int, MathError>> = Feed { supply: |x| => x * 2 }\n";

// ---------------------------------------------------------------------------
// [TYPE-VARIANCE-COERCION] — the coercion exists, at depth 0 only
// ---------------------------------------------------------------------------

plain_cases! {
    /// "A bare `T` satisfies a `Result<T, E>` slot (an implicit `Success`)."
    the_coercion_holds_at_a_direct_value_site: flows("", "Result<int, MathError>", "5");

    /// "the inverse never holds anywhere."
    the_inverse_coercion_never_holds: blocked( "fn half() -> Result<int, MathError> = 20 * 5\n", "int", "half()", );
}

// ---------------------------------------------------------------------------
// [TYPE-VARIANCE-COERCION] — and never inside an argument position
// ---------------------------------------------------------------------------

plain_cases! {
    /// "`Feed<int>` does **not** satisfy a `Feed<Result<int, MathError>>` slot,
    /// under `out T`" — the coercion would have to rebuild the container.
    a_covariant_argument_does_not_carry_the_coercion: blocked(FEED, "Feed<Result<int, MathError>>", "feedInt()");

    /// "…under `in T`" — the flipped recursion does not smuggle it in either.
    a_contravariant_argument_does_not_carry_the_coercion: blocked(GATE, "Gate<int>", "gateRes()");

    /// "…or unannotated."
    an_invariant_argument_does_not_carry_the_coercion: blocked(CELL, "Cell<Result<int, MathError>>", "cellInt()");

    /// The unwrapping direction is refused under every marker too — the half the
    /// shipped fixtures already cover, kept here so both directions sit together.
    no_marker_unwraps_a_result_payload: blocked(FEED, "Feed<int>", "feedRes()"),
    blocked(GATE, "Gate<Result<int, MathError>>", "gateInt()"),
    blocked(CELL, "Cell<int>", "cellRes()");

    /// The identical instantiation is what all three markers DO accept.
    every_marker_accepts_the_identical_instantiation: flows(FEED, "Feed<int>", "feedInt()"),
    flows(GATE, "Gate<int>", "gateInt()"),
    flows(CELL, "Cell<int>", "cellInt()");

    /// And an unrelated payload is refused under every marker.
    every_marker_rejects_an_unrelated_payload: blocked(FEED, "Feed<int>", "feedText()"),
    blocked(GATE, "Gate<int>", "gateText()"),
    blocked(CELL, "Cell<int>", "cellText()");
}

// ---------------------------------------------------------------------------
// Composition — depth does not unlock the coercion
// ---------------------------------------------------------------------------

plain_cases! {
    /// `out` inside `out` recurses, and still bottoms out exact.
    covariance_inside_covariance_still_bottoms_out_exact: blocked( NESTED, "Feed<Feed<Result<int, MathError>>>", "feedFeedInt()", ),
    flows(NESTED, "Feed<Feed<int>>", "feedFeedInt()");

    /// `in` inside `out` flips the recursion — and changes no outcome.
    contravariance_inside_covariance_still_bottoms_out_exact: blocked(NESTED, "Feed<Gate<int>>", "feedGateRes()"),
    blocked( NESTED, "Feed<Gate<Result<int, MathError>>>", "feedGateInt()", );

    /// `out` inside `in`, the other flip.
    covariance_inside_contravariance_still_bottoms_out_exact: blocked(NESTED, "Gate<Feed<int>>", "gateFeedRes()");

    /// Two flips compose back to covariance, which is still exact.
    contravariance_inside_contravariance_still_bottoms_out_exact: blocked( NESTED, "Gate<Gate<Result<int, MathError>>>", "gateGateInt()", ),
    flows(NESTED, "Gate<Gate<int>>", "gateGateInt()");

    /// An invariant argument under a covariant one.
    an_invariant_argument_under_a_covariant_one_is_exact: blocked( NESTED, "Feed<Cell<Result<int, MathError>>>", "feedCellInt()", );
}

// ---------------------------------------------------------------------------
// Function payloads: exact under a container, assignable when assigned direct
// ---------------------------------------------------------------------------

plain_cases! {
    /// "Function payloads match exactly for the same reason" — the parameter
    /// position does not flip inside a container.
    a_function_payload_matches_exactly_inside_a_container: blocked(FNPAYLOAD, "Feed<(int) -> bool>", "feedTakesRes()"),
    blocked( FNPAYLOAD, "Feed<(Result<int, MathError>) -> bool>", "feedTakesInt()", );

    /// The spec's own counterexample, and its mirror: neither return direction
    /// matches under a container.
    a_function_payloads_return_matches_exactly_inside_a_container: blocked(FNPAYLOAD, "Feed<(int) -> int>", "feedGivesRes()"),
    blocked( FNPAYLOAD, "Feed<(int) -> Result<int, MathError>>", "feedGivesInt()", );

    /// The identical function payload flows, so the refusals above are about the
    /// shape and not about function payloads being rejected wholesale.
    an_identical_function_payload_flows: flows(FNPAYLOAD, "Feed<(int) -> bool>", "feedTakesInt()");
}

// ---------------------------------------------------------------------------
// The agreement test: the consequence, pinned
// ---------------------------------------------------------------------------

/// "`out T`, `in T` and an unannotated parameter accept and refuse **exactly
/// the same programs** at assignment sites today."
///
/// This test is the tripwire for that sentence. It goes red the day someone
/// implements a directional relation that actually bites — at which point the
/// spec's consequence paragraph is stale and must be rewritten with it, rather
/// than the language quietly gaining a coercion that reads a payload at the
/// wrong representation.
#[test]
fn the_three_markers_agree_on_every_assignment_outcome() {
    let outcomes: Vec<(&str, bool, bool)> = vec![
        (
            "covariant",
            accepted(&sites(FEED, "Feed<Result<int, MathError>>", "feedInt()")[0]),
            accepted(&sites(FEED, "Feed<int>", "feedRes()")[0]),
        ),
        (
            "contravariant",
            accepted(&sites(GATE, "Gate<Result<int, MathError>>", "gateInt()")[0]),
            accepted(&sites(GATE, "Gate<int>", "gateRes()")[0]),
        ),
        (
            "invariant",
            accepted(&sites(CELL, "Cell<Result<int, MathError>>", "cellInt()")[0]),
            accepted(&sites(CELL, "Cell<int>", "cellRes()")[0]),
        ),
    ];
    for (marker, forward, backward) in &outcomes {
        assert!(
            !forward && !backward,
            "{marker}: expected both directions refused ([TYPE-VARIANCE-COERCION]), \
             got forward={forward} backward={backward}"
        );
    }
}

// ---------------------------------------------------------------------------
// The same rules through the ML surface ([FLAVOR-BOUNDARY])
// ---------------------------------------------------------------------------

spec_cases! {
    /// The ML twin: a covariant ML container refuses the coercion too.
    ml_a_covariant_argument_does_not_carry_the_coercion: rejects(Ml, "type Feed out T =\n    supply : T\n\
         feedInt : Unit -> Feed<int>\n\
         feedInt () = Feed(supply = 1)\n\
         takes : Feed<Result<int, MathError>> -> int\n\
         takes v = 0\n\
         held = takes (feedInt ())\n\
         print \"${held}\"\n");

    /// The ML twin of the identical-instantiation acceptance, so the refusal above
    /// is not an ML parsing accident.
    ml_an_identical_instantiation_flows: accepts(Ml, "type Feed out T =\n    supply : T\n\
         feedInt : Unit -> Feed<int>\n\
         feedInt () = Feed(supply = 1)\n\
         takes : Feed<int> -> int\n\
         takes v = 0\n\
         held = takes (feedInt ())\n\
         print \"${held}\"\n");

    /// The ML twin of the direct-site coercion.
    ml_the_coercion_holds_at_a_direct_value_site: accepts(Ml, "takes : Result<int, MathError> -> int\n\
         takes v = 0\n\
         held = takes 5\n\
         print \"${held}\"\n");
}
