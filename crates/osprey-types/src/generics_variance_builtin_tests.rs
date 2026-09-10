//! The **built-in variance table** of [TYPE-VARIANCE-ASSIGN], asserted through
//! the assignment sites rather than read off the declaration
//! ([TYPE-VARIANCE-COERCION], docs/specs/0004-TypeSystem.md).
//!
//! "Built-in constructors' declared variance: `Result<out T, out E>`,
//! `List<out T>`, `Fiber<out T>`, `Map<K, out V>` (keys invariant); `Channel<T>`
//! and `Ptr` are invariant. Function types are structurally contravariant in
//! parameters and covariant in returns."
//!
//! The last sentence is the one with teeth: a function value assigned DIRECTLY
//! matches assignably, parameters flipped and return coerced. The constructor
//! entries above it do not bite today, because the only coercion in the language
//! is representation-changing and therefore barred from argument positions — so
//! `List<out T>` and the invariant `Channel<T>` accept and refuse the same
//! programs. Both halves are pinned here: the table's entries are a contract for
//! the day that changes, and the function rules are checked as live behaviour.

use crate::generics_variance_assign_tests::{accepted, blocked, flows, sites};
use crate::testutil::plain_cases;

/// Producers for every built-in shape under test, at both instantiations.
const BUILTINS: &str = "fn listInt() -> List<int> = [1]\n\
    fn listRes() -> List<Result<int, MathError>> = [20 * 5]\n\
    fn mapInt() -> Map<string, int> = { \"a\": 1 }\n\
    fn mapRes() -> Map<string, Result<int, MathError>> = { \"a\": 20 * 5 }\n\
    fn resInt() -> Result<int, MathError> = 20 * 5\n\
    fn one() = 1\n\
    fn fiberInt() -> Fiber<int> = spawn one()\n\
    fn chanInt() -> Channel<int> = Channel(2)\n\
    fn chanRes() -> Channel<Result<int, MathError>> = Channel(2)\n\
    fn takesInt(v: int) = true\n\
    fn takesRes(v: Result<int, MathError>) = true\n\
    fn givesInt(v: int) = 1\n\
    fn givesRes(v: int) = v * 2\n";

// ---------------------------------------------------------------------------
// The constructor entries: identical instantiations flow, coercions do not
// ---------------------------------------------------------------------------

plain_cases! {
    /// `List<out T>` accepts its own element type…
    list_accepts_the_identical_element: flows(BUILTINS, "List<int>", "listInt()");

    /// …refuses the coercion in both directions…
    list_refuses_the_coercion_in_both_directions: blocked(BUILTINS, "List<Result<int, MathError>>", "listInt()"),
    blocked(BUILTINS, "List<int>", "listRes()");

    /// …and refuses an unrelated element.
    list_rejects_an_unrelated_element: blocked(BUILTINS, "List<string>", "listInt()");

    /// `Result<out T, out E>`: neither channel carries the coercion inward.
    result_refuses_the_coercion_in_either_channel: blocked( BUILTINS, "Result<Result<int, MathError>, MathError>", "resInt()", ),
    blocked( BUILTINS, "Result<int, Result<MathError, MathError>>", "resInt()", );

    /// The identical `Result` instantiation flows.
    result_accepts_the_identical_instantiation: flows(BUILTINS, "Result<int, MathError>", "resInt()");

    /// `Fiber<out T>`: same story for a fiber's answer.
    fiber_refuses_the_coercion_and_accepts_its_own_answer: blocked(BUILTINS, "Fiber<Result<int, MathError>>", "fiberInt()"),
    blocked(BUILTINS, "Fiber<string>", "fiberInt()"),
    flows(BUILTINS, "Fiber<int>", "fiberInt()");

    /// `Map<K, out V>`: the value channel refuses the coercion…
    map_refuses_the_coercion_in_its_value: blocked(BUILTINS, "Map<string, Result<int, MathError>>", "mapInt()"),
    blocked(BUILTINS, "Map<string, int>", "mapRes()");

    /// The key channel cannot be exercised for variance at all: the shipped map
    /// surface fixes keys to `string` ([BUILTIN-MAP-GET], spec 0012), so
    /// `Map<int, int>` has no producer and `Map<K, out V>`'s "(keys invariant)"
    /// describes a parameter no program can instantiate. Recorded as a spec/
    /// implementation conflict in plan 0015 rather than asserted as behaviour; what
    /// IS assertable is that a non-string key is refused.
    a_non_string_map_key_is_refused: blocked(BUILTINS, "Map<int, int>", "mapInt()");

    /// The identical map instantiation flows.
    map_accepts_the_identical_instantiation: flows(BUILTINS, "Map<string, int>", "mapInt()");

    /// `Channel<T>` is invariant, and behaves exactly as the covariant entries do.
    channel_refuses_the_coercion_in_both_directions: blocked(BUILTINS, "Channel<Result<int, MathError>>", "chanInt()"),
    blocked(BUILTINS, "Channel<int>", "chanRes()"),
    flows(BUILTINS, "Channel<int>", "chanInt()");
}

/// The table's covariant and invariant entries are indistinguishable today —
/// `List<out T>` and `Channel<T>` answer identically. This is the built-in twin
/// of `the_three_markers_agree_on_every_assignment_outcome`, and the tripwire
/// for the same spec sentence.
#[test]
fn the_covariant_and_invariant_builtins_agree() {
    let covariant = accepted(&sites(BUILTINS, "List<Result<int, MathError>>", "listInt()")[0]);
    let invariant = accepted(&sites(BUILTINS, "Channel<Result<int, MathError>>", "chanInt()")[0]);
    assert!(
        !covariant && !invariant,
        "[TYPE-VARIANCE-COERCION] says an argument position never coerces: \
         List(out)={covariant}, Channel(invariant)={invariant}"
    );
}

// ---------------------------------------------------------------------------
// Function types — the half of the table that DOES bite
// ---------------------------------------------------------------------------

plain_cases! {
    /// "structurally contravariant in parameters": a function accepting a `Result`
    /// stands where one accepting an `int` is expected, because the parameter
    /// recursion applies the coercion at a direct site.
    a_function_slot_is_contravariant_in_its_parameter: flows(BUILTINS, "(int) -> bool", "takesRes");

    /// The widened direction is refused.
    a_function_slot_rejects_a_widened_parameter: blocked(BUILTINS, "(Result<int, MathError>) -> bool", "takesInt");

    /// "and covariant in returns": a function returning `int` stands where one
    /// returning `Result<int, E>` is expected.
    a_function_slot_is_covariant_in_its_return: flows(BUILTINS, "(int) -> Result<int, MathError>", "givesInt");

    /// The unwrapping direction is refused.
    a_function_slot_never_unwraps_its_return: blocked(BUILTINS, "(int) -> int", "givesRes");

    /// The identical shape flows.
    a_function_slot_accepts_the_identical_shape: flows(BUILTINS, "(int) -> bool", "takesInt");

    /// Arity is checked before any of this.
    a_function_slot_rejects_an_arity_mismatch: blocked(BUILTINS, "(int, int) -> bool", "takesInt");
}

/// The function rules and the constructor rules are the SAME relation applied
/// at different depths: directly assigned, the parameter coercion works; the
/// identical function wrapped in a `List` argument position does not.
#[test]
fn the_function_rules_stop_at_an_argument_position() {
    let direct = accepted(&sites(BUILTINS, "(int) -> bool", "takesRes")[0]);
    let wrapped = accepted(
        &sites(
            &format!("{BUILTINS}fn listTakesRes() -> List<(Result<int, MathError>) -> bool> = [takesRes]\n"),
            "List<(int) -> bool>",
            "listTakesRes()",
        )[0],
    );
    assert!(
        direct && !wrapped,
        "the parameter coercion must apply at a direct site and NOT inside a \
         `List` argument: direct={direct}, wrapped={wrapped}"
    );
}
