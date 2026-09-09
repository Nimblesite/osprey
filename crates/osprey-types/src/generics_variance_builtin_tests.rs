//! The **built-in variance table** of [TYPE-VARIANCE-ASSIGN], asserted through
//! the assignment sites rather than read off the declaration
//! (docs/specs/0004-TypeSystem.md).
//!
//! "Built-in constructors' declared variance: `Result<out T, out E>`,
//! `List<out T>`, `Fiber<out T>`, `Map<K, out V>` (keys invariant); `Channel<T>`
//! and `Ptr` are invariant. Function types are structurally contravariant in
//! parameters and covariant in returns."
//!
//! Each entry in that sentence is a claim about what flows where, so each gets
//! its accepting direction, its refusing direction and its identical-instance
//! baseline — the three together are what distinguish a covariant entry from an
//! invariant one. The relation recursed is the promotion `T -> Result<T, E>`,
//! whose depth-0 control lives in `generics_variance_assign_tests.rs`.

use crate::generics_variance_assign_tests::{blocked, flows};

/// Producers for every built-in shape under test, at both instantiations.
const BUILTINS: &str = "fn listInt() -> List<int> = [1]\n\
    fn listRes() -> List<Result<int, MathError>> = [20 * 5]\n\
    fn mapInt() -> Map<string, int> = { \"a\": 1 }\n\
    fn mapRes() -> Map<string, Result<int, MathError>> = { \"a\": 20 * 5 }\n\
    fn mapIntKeys() -> Map<int, int> = { 1: 2 }\n\
    fn resInt() -> Result<int, MathError> = 20 * 5\n\
    fn one() = 1\n\
    fn fiberInt() -> Fiber<int> = spawn one()\n\
    fn chanInt() -> Channel<int> = Channel(2)\n\
    fn chanRes() -> Channel<Result<int, MathError>> = Channel(2)\n\
    fn takesInt(v: int) = true\n\
    fn takesRes(v: Result<int, MathError>) = true\n\
    fn givesInt(v: int) = 1\n\
    fn givesRes(v: int) = v * 2\n";

/// Built-ins nested inside each other and inside a declared covariant
/// container, so composition is exercised through the table too.
const NESTED_BUILTINS: &str = "type Feed<out T> = Feed { supply: T } | Dry\n\
    type Gate<in T> = Gate { admit: (T) -> bool } | Open\n\
    fn listListInt() -> List<List<int>> = [[1]]\n\
    fn mapListInt() -> Map<string, List<int>> = { \"a\": [1] }\n\
    fn resListInt() -> Result<List<int>, MathError> = [1]\n\
    fn feedListInt() -> Feed<List<int>> = Feed { supply: [1] }\n\
    fn gateListRes() -> Gate<List<Result<int, MathError>>> = Gate { admit: |x| => true }\n\
    fn gateListInt() -> Gate<List<int>> = Gate { admit: |x| => true }\n";

// ---------------------------------------------------------------------------
// List<out T>
// ---------------------------------------------------------------------------

/// A `List<int>` flows into a `List<Result<int, MathError>>` slot.
#[test]
fn list_carries_the_promotion_into_its_element() {
    flows(BUILTINS, "List<Result<int, MathError>>", "listInt()");
}

/// The unwrapping direction is refused.
#[test]
fn list_never_unwraps_its_element() {
    blocked(BUILTINS, "List<int>", "listRes()");
}

/// The baseline.
#[test]
fn list_accepts_the_identical_element() {
    flows(BUILTINS, "List<int>", "listInt()");
}

/// Covariance is not looseness: an unrelated element is refused.
#[test]
fn list_rejects_an_unrelated_element() {
    blocked(BUILTINS, "List<string>", "listInt()");
}

// ---------------------------------------------------------------------------
// Result<out T, out E>
// ---------------------------------------------------------------------------

/// The VALUE channel is covariant.
#[test]
fn result_carries_the_promotion_into_its_value() {
    flows(
        BUILTINS,
        "Result<Result<int, MathError>, MathError>",
        "resInt()",
    );
}

/// The ERROR channel is covariant too — the second half of the table entry,
/// which nothing in the tree exercised.
#[test]
fn result_carries_the_promotion_into_its_error() {
    flows(
        BUILTINS,
        "Result<int, Result<MathError, MathError>>",
        "resInt()",
    );
}

/// Neither channel unwraps.
#[test]
fn result_never_unwraps_either_channel() {
    blocked(BUILTINS, "Result<int, MathError>", "listRes()");
}

// ---------------------------------------------------------------------------
// Fiber<out T>
// ---------------------------------------------------------------------------

/// A `Fiber<int>` flows into a `Fiber<Result<int, MathError>>` slot.
#[test]
fn fiber_carries_the_promotion_into_its_answer() {
    flows(BUILTINS, "Fiber<Result<int, MathError>>", "fiberInt()");
}

/// The baseline.
#[test]
fn fiber_accepts_the_identical_answer() {
    flows(BUILTINS, "Fiber<int>", "fiberInt()");
}

/// An unrelated answer is refused.
#[test]
fn fiber_rejects_an_unrelated_answer() {
    blocked(BUILTINS, "Fiber<string>", "fiberInt()");
}

// ---------------------------------------------------------------------------
// Map<K, out V> — the value is covariant, the key is NOT
// ---------------------------------------------------------------------------

/// The value channel carries the promotion.
#[test]
fn map_carries_the_promotion_into_its_value() {
    flows(
        BUILTINS,
        "Map<string, Result<int, MathError>>",
        "mapInt()",
    );
}

/// The value channel does not unwrap.
#[test]
fn map_never_unwraps_its_value() {
    blocked(BUILTINS, "Map<string, int>", "mapRes()");
}

/// "(keys invariant)" — the key channel refuses the promotion the value channel
/// carries. This pair is the whole content of that parenthesis.
#[test]
fn map_keys_refuse_the_promotion_in_both_directions() {
    blocked(
        BUILTINS,
        "Map<Result<int, MathError>, int>",
        "mapIntKeys()",
    );
}

/// The baseline.
#[test]
fn map_accepts_the_identical_instantiation() {
    flows(BUILTINS, "Map<string, int>", "mapInt()");
}

// ---------------------------------------------------------------------------
// Channel<T> — invariant in both directions
// ---------------------------------------------------------------------------

/// A channel is written as well as read, so neither direction is safe.
#[test]
fn channel_refuses_the_promotion_in_both_directions() {
    blocked(BUILTINS, "Channel<Result<int, MathError>>", "chanInt()");
    blocked(BUILTINS, "Channel<int>", "chanRes()");
}

/// The baseline.
#[test]
fn channel_accepts_the_identical_element() {
    flows(BUILTINS, "Channel<int>", "chanInt()");
}

// ---------------------------------------------------------------------------
// Function types — contravariant parameters, covariant returns
// ---------------------------------------------------------------------------

/// A function accepting a `Result` stands where one accepting an `int` is
/// expected: the parameter position is contravariant.
#[test]
fn a_function_slot_is_contravariant_in_its_parameter() {
    flows(BUILTINS, "(int) -> bool", "takesRes");
}

/// The widened direction is refused.
#[test]
fn a_function_slot_rejects_a_widened_parameter() {
    blocked(BUILTINS, "(Result<int, MathError>) -> bool", "takesInt");
}

/// A function returning `int` stands where one returning `Result<int, E>` is
/// expected: the return position is covariant.
#[test]
fn a_function_slot_is_covariant_in_its_return() {
    flows(BUILTINS, "(int) -> Result<int, MathError>", "givesInt");
}

/// The unwrapping direction is refused.
#[test]
fn a_function_slot_never_unwraps_its_return() {
    blocked(BUILTINS, "(int) -> int", "givesRes");
}

/// The baseline.
#[test]
fn a_function_slot_accepts_the_identical_shape() {
    flows(BUILTINS, "(int) -> bool", "takesInt");
}

// ---------------------------------------------------------------------------
// Composition through the table
// ---------------------------------------------------------------------------

/// `List` inside `List` stays covariant.
#[test]
fn list_composes_with_list() {
    flows(
        NESTED_BUILTINS,
        "List<List<Result<int, MathError>>>",
        "listListInt()",
    );
}

/// A covariant built-in inside a covariant built-in's value channel.
#[test]
fn map_composes_with_list_in_its_value() {
    flows(
        NESTED_BUILTINS,
        "Map<string, List<Result<int, MathError>>>",
        "mapListInt()",
    );
}

/// A covariant built-in inside `Result`'s value channel.
#[test]
fn result_composes_with_list_in_its_value() {
    flows(
        NESTED_BUILTINS,
        "Result<List<Result<int, MathError>>, MathError>",
        "resListInt()",
    );
}

/// A built-in inside a DECLARED covariant container: the two tables are one
/// relation, not two.
#[test]
fn a_declared_covariant_container_composes_with_a_builtin() {
    flows(
        NESTED_BUILTINS,
        "Feed<List<Result<int, MathError>>>",
        "feedListInt()",
    );
}

/// …and inside a declared CONTRAVARIANT container the composition flips.
#[test]
fn a_declared_contravariant_container_flips_a_builtin() {
    flows(NESTED_BUILTINS, "Gate<List<int>>", "gateListRes()");
}

/// The unflipped direction stays refused at that depth.
#[test]
fn a_declared_contravariant_container_rejects_the_unflipped_builtin() {
    blocked(
        NESTED_BUILTINS,
        "Gate<List<Result<int, MathError>>>",
        "gateListInt()",
    );
}
