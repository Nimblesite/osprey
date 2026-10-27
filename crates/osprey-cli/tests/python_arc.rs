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
        include_str!("../../../tests/regressions/python/closure_captures_record.py"),
        "13\n",
    );
}
