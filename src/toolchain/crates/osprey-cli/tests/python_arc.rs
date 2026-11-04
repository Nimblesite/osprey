//! Python-flavor ownership regressions run in every native memory backend.
//! The shared executor requires successful exit, exact output and one clean
//! ARC sentinel. Implements [GC-ARC-PERCEUS].

#[path = "common/effect_execution.rs"]
mod effect_execution;

#[test]
fn a_record_captured_per_loop_iteration_is_released_each_iteration() {
    effect_execution::assert_flavored_output(
        "closure_captures_record",
        "py",
        include_str!("../../../../osprey/tests/regressions/python/closure_captures_record.py"),
        "13\n",
    );
}

/// [FLAVOR-PYTHON-EXPRESSIONS] An escape decoding to a brace is that character:
/// it never pairs with its neighbour. The golden is the output of `CPython`.
#[test]
fn an_escaped_brace_in_an_f_string_stays_one_brace_per_escape() {
    effect_execution::assert_flavored_output(
        "fstring_brace_escapes",
        "py",
        include_str!("../../../../osprey/tests/regressions/python/fstring_brace_escapes.py"),
        include_str!(
            "../../../../osprey/tests/regressions/python/fstring_brace_escapes.expectedoutput"
        ),
    );
}
