//! Spec-driven assertions for the **directional** half of
//! [TYPE-VARIANCE-ASSIGN] (docs/specs/0004-TypeSystem.md).
//!
//! "At *assignment sites* (call arguments, annotated bindings, return
//! positions), a variance-declared constructor's arguments are matched
//! directionally: covariant (`out`) arguments recurse expected-accepts-actual,
//! contravariant (`in`) arguments recurse with the roles flipped, invariant
//! arguments unify exactly."
//!
//! `generics_variance_tests.rs` states where a marker may be WRITTEN; this
//! module states what writing it BUYS. The distinction matters because the two
//! shipped fixtures (`variance_covariant_result_payload.ospo` and
//! `variance_invariant_arg_mismatch.ospo`) assert the same rejection under
//! `out T` and under an unannotated parameter — so nothing in the tree today
//! separates a covariant container from an invariant one, and a checker that
//! ignored every marker at assignment sites would pass both.
//!
//! The relation being recursed is the one-way promotion `T -> Result<T, E>`
//! that the direct value site already models (`the_direct_site_promotion_is_the
//! _relation_being_recursed` below is its control). Every claim here is made at
//! ALL THREE sites the spec names, because a relation implemented at one site
//! is not the relation the spec describes.

use crate::testutil::{accepts, rejects};
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

/// An invariant container.
const CELL: &str = "type Cell<T> = Cell { slot: T }\n\
    fn cellInt() -> Cell<int> = Cell { slot: 1 }\n\
    fn cellRes() -> Cell<Result<int, MathError>> = Cell { slot: 20 * 5 }\n";

/// The three containers nested one level inside each other.
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

/// A covariant container over FUNCTION payloads, where the structural rule
/// (contravariant parameters, covariant returns) is the thing recursed into.
const FNPAYLOAD: &str = "type Feed<out T> = Feed { supply: T } | Dry\n\
    fn feedTakesInt() -> Feed<(int) -> bool> = Feed { supply: |x| => true }\n\
    fn feedTakesRes() -> Feed<(Result<int, MathError>) -> bool> = \
        Feed { supply: |x| => true }\n\
    fn feedGivesInt() -> Feed<(int) -> int> = Feed { supply: |x| => 1 }\n\
    fn feedGivesRes() -> Feed<(int) -> Result<int, MathError>> = Feed { supply: |x| => x * 2 }\n";

// ---------------------------------------------------------------------------
// The control: the relation exists at depth 0
// ---------------------------------------------------------------------------

/// The one-way promotion `T -> Result<T, E>` holds at a direct value site. Every
/// rejection below that should have been an acceptance is therefore a failure to
/// RECURSE the relation, not a missing relation.
#[test]
fn the_direct_site_promotion_is_the_relation_being_recursed() {
    flows("", "Result<int, MathError>", "5");
}

/// Its inverse never holds, at depth 0 or anywhere else.
#[test]
fn the_inverse_promotion_never_holds_at_the_direct_site() {
    blocked("fn half() -> Result<int, MathError> = 10 / 2\n", "int", "half()");
}

// ---------------------------------------------------------------------------
// Covariance: `out` arguments recurse expected-accepts-actual
// ---------------------------------------------------------------------------

/// A `Feed<int>` flows into a `Feed<Result<int, MathError>>` slot: the covariant
/// argument recurses the promotion. This is the ENTIRE payoff of writing `out`,
/// and nothing in the tree asserted it before.
#[test]
fn covariance_carries_the_promotion_into_the_argument() {
    flows(FEED, "Feed<Result<int, MathError>>", "feedInt()");
}

/// The unwrapping direction stays refused — "no `Result<T, E>`-to-`T` coercion
/// at any depth".
#[test]
fn covariance_never_unwraps_a_result_payload() {
    blocked(FEED, "Feed<int>", "feedRes()");
}

/// The identical instantiation is the baseline both directions are measured
/// against.
#[test]
fn covariance_accepts_the_identical_instantiation() {
    flows(FEED, "Feed<int>", "feedInt()");
}

/// Covariance is not "anything goes": an unrelated payload is still refused.
#[test]
fn covariance_rejects_an_unrelated_payload() {
    blocked(FEED, "Feed<int>", "feedText()");
    blocked(FEED, "Feed<string>", "feedInt()");
}

// ---------------------------------------------------------------------------
// Contravariance: `in` arguments recurse with the roles flipped
// ---------------------------------------------------------------------------

/// A `Gate<Result<int, MathError>>` flows into a `Gate<int>` slot — the flipped
/// recursion, and the entire payoff of writing `in`.
#[test]
fn contravariance_carries_the_promotion_with_the_roles_flipped() {
    flows(GATE, "Gate<int>", "gateRes()");
}

/// The unflipped direction is refused: a gate admitting only `int` cannot stand
/// where one admitting a `Result` is expected.
#[test]
fn contravariance_rejects_the_covariant_direction() {
    blocked(GATE, "Gate<Result<int, MathError>>", "gateInt()");
}

/// The baseline.
#[test]
fn contravariance_accepts_the_identical_instantiation() {
    flows(GATE, "Gate<int>", "gateInt()");
}

/// And it is not "anything goes" either.
#[test]
fn contravariance_rejects_an_unrelated_payload() {
    blocked(GATE, "Gate<int>", "gateText()");
}

// ---------------------------------------------------------------------------
// Invariance: arguments unify exactly, in BOTH directions
// ---------------------------------------------------------------------------

/// An unannotated parameter refuses the promotion the covariant one carries.
/// This test and `covariance_carries_the_promotion_into_the_argument` are the
/// pair that separates a covariant container from an invariant one; the shipped
/// fixtures assert only the half both share.
#[test]
fn invariance_refuses_the_promotion_in_both_directions() {
    blocked(CELL, "Cell<Result<int, MathError>>", "cellInt()");
    blocked(CELL, "Cell<int>", "cellRes()");
}

/// The baseline.
#[test]
fn invariance_accepts_the_identical_instantiation() {
    flows(CELL, "Cell<int>", "cellInt()");
}

// ---------------------------------------------------------------------------
// Composition: "a nested constructor's argument composes the position"
// ---------------------------------------------------------------------------

/// `out` inside `out` stays covariant, so the promotion reaches two levels down.
#[test]
fn covariance_composes_with_covariance() {
    flows(NESTED, "Feed<Feed<Result<int, MathError>>>", "feedFeedInt()");
}

/// `in` inside `out` flips once: the inner argument recurses backwards.
#[test]
fn contravariance_inside_covariance_flips_once() {
    flows(NESTED, "Feed<Gate<int>>", "feedGateRes()");
}

/// …and the unflipped direction stays refused at that depth.
#[test]
fn contravariance_inside_covariance_rejects_the_unflipped_direction() {
    blocked(NESTED, "Feed<Gate<Result<int, MathError>>>", "feedGateInt()");
}

/// `out` inside `in` flips the outer relation, so the value travels the other
/// way.
#[test]
fn covariance_inside_contravariance_flips_once() {
    flows(NESTED, "Gate<Feed<int>>", "gateFeedRes()");
}

/// Two flips compose back to covariance.
#[test]
fn contravariance_inside_contravariance_composes_back_to_covariance() {
    flows(NESTED, "Gate<Gate<Result<int, MathError>>>", "gateGateInt()");
}

/// An invariant argument stops the recursion whatever encloses it.
#[test]
fn an_invariant_argument_stops_the_recursion() {
    blocked(NESTED, "Feed<Cell<Result<int, MathError>>>", "feedCellInt()");
}

// ---------------------------------------------------------------------------
// Function payloads: contravariant parameters, covariant returns
// ---------------------------------------------------------------------------

/// "Function types are structurally contravariant in parameters": a function
/// accepting a `Result` stands where one accepting an `int` is expected.
#[test]
fn a_function_payload_is_contravariant_in_its_parameter() {
    flows(FNPAYLOAD, "Feed<(int) -> bool>", "feedTakesRes()");
}

/// The opposite direction is refused.
#[test]
fn a_function_payload_rejects_a_widened_parameter() {
    blocked(FNPAYLOAD, "Feed<(Result<int, MathError>) -> bool>", "feedTakesInt()");
}

/// "…and covariant in returns": a function returning `int` stands where one
/// returning `Result<int, E>` is expected.
#[test]
fn a_function_payload_is_covariant_in_its_return() {
    flows(FNPAYLOAD, "Feed<(int) -> Result<int, MathError>>", "feedGivesInt()");
}

/// The spec's own counterexample: "a `Feed<(int) -> Result<int, Error>>` does
/// not match a `Feed<(int) -> int>` slot".
#[test]
fn the_spec_counterexample_stays_refused() {
    blocked(FNPAYLOAD, "Feed<(int) -> int>", "feedGivesRes()");
}

// ---------------------------------------------------------------------------
// The same relation through the ML surface ([FLAVOR-BOUNDARY])
// ---------------------------------------------------------------------------

/// The ML twin of the covariant payoff.
#[test]
fn ml_covariance_carries_the_promotion_into_the_argument() {
    accepts(
        Flavor::Ml,
        "type Feed out T =\n    supply : T\n\
         feedInt : Unit -> Feed<int>\n\
         feedInt () = Feed(supply = 1)\n\
         takes : Feed<Result<int, MathError>> -> int\n\
         takes v = 0\n\
         held = takes (feedInt ())\n\
         print \"${held}\"\n",
    );
}

/// The ML twin of the contravariant payoff.
#[test]
fn ml_contravariance_carries_the_promotion_with_the_roles_flipped() {
    accepts(
        Flavor::Ml,
        "type Gate in T =\n    admit : T -> bool\n\
         gateRes : Unit -> Gate<Result<int, MathError>>\n\
         gateRes () = Gate(admit = \\x => true)\n\
         takes : Gate<int> -> int\n\
         takes v = 0\n\
         held = takes (gateRes ())\n\
         print \"${held}\"\n",
    );
}

/// The ML twin of the invariant refusal.
#[test]
fn ml_invariance_refuses_the_promotion() {
    rejects(
        Flavor::Ml,
        "type Cell T =\n    slot : T\n\
         cellInt : Unit -> Cell<int>\n\
         cellInt () = Cell(slot = 1)\n\
         takes : Cell<Result<int, MathError>> -> int\n\
         takes v = 0\n\
         held = takes (cellInt ())\n\
         print \"${held}\"\n",
    );
}

// ---------------------------------------------------------------------------
// The collapse detector
// ---------------------------------------------------------------------------

/// True when the checker accepts the program outright.
fn accepted(src: &str) -> bool {
    crate::testutil::typecheck(Flavor::Default, src).is_empty()
}

/// [TYPE-VARIANCE-ASSIGN] opens by saying variance DIRECTS assignability, then
/// closes by saying the recursion "bottoms out in exact unification". Read
/// strictly, the second sentence empties the first: if every leaf must match
/// exactly, `out T`, `in T` and an unannotated parameter accept and refuse
/// exactly the same programs, and the marker's only teeth are
/// [TYPE-VARIANCE-POSITIONS].
///
/// This test does not take a side. It asserts only that the two readings are
/// distinguishable in the tree — that SOMETHING about assignment changes when
/// the marker changes. If it fails, the first sentence of [TYPE-VARIANCE-ASSIGN]
/// describes nothing the compiler does and the spec is the stale source; the
/// two shipped fixtures cannot see this, because both assert the one direction
/// all three markers agree on.
#[test]
fn the_marker_must_change_at_least_one_assignment_outcome() {
    let covariant = accepted(&sites(FEED, "Feed<Result<int, MathError>>", "feedInt()")[0]);
    let contravariant = accepted(&sites(GATE, "Gate<int>", "gateRes()")[0]);
    let invariant_forward = accepted(&sites(CELL, "Cell<Result<int, MathError>>", "cellInt()")[0]);
    let invariant_flipped = accepted(&sites(CELL, "Cell<int>", "cellRes()")[0]);

    assert!(
        covariant != invariant_forward || contravariant != invariant_flipped,
        "variance is inert at assignment sites: out={covariant}, in={contravariant}, \
         invariant={invariant_forward}/{invariant_flipped} — all three markers accept and \
         refuse the same programs, so [TYPE-VARIANCE-ASSIGN]'s directional rule has no \
         observable content and only [TYPE-VARIANCE-POSITIONS] is load-bearing"
    );
}

/// The other half of the same question, asked of the built-in table: `List` is
/// declared `out` and `Channel` invariant, so at least one program must
/// separate them.
#[test]
fn a_builtin_marker_must_change_at_least_one_assignment_outcome() {
    const BUILTINS: &str = "fn listInt() -> List<int> = [1]\n\
        fn chanInt() -> Channel<int> = Channel(2)\n";
    let covariant = accepted(&sites(BUILTINS, "List<Result<int, MathError>>", "listInt()")[0]);
    let invariant = accepted(&sites(BUILTINS, "Channel<Result<int, MathError>>", "chanInt()")[0]);

    assert!(
        covariant != invariant,
        "`List<out T>` and `Channel<T>` are indistinguishable at assignment sites \
         (both {covariant}), so the built-in variance table in [TYPE-VARIANCE-ASSIGN] \
         records nothing the compiler consults"
    );
}
