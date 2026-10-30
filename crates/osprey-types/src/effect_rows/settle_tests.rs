//! A fixed point whose returns never revisit an earlier value must still
//! settle: the proof kernel's mutually recursive descent produced a fresh
//! widened return on every sweep and never converged.
use super::{ReturnHistory, Value};

const FUNCTION: usize = 0;
const SWEEPS: usize = 64;
const SETTLED_TAIL: usize = 8;

/// A record whose one field holds a callable that differs on every sweep, so
/// no return ever repeats an earlier one exactly.
fn fresh_return(sweep: usize) -> Value {
    Value {
        fields: [("next".to_owned(), Value::parameter(1, sweep))]
            .into_iter()
            .collect(),
        ..Value::default()
    }
}

#[test]
fn returns_that_never_revisit_still_settle() {
    let mut history = ReturnHistory::default();
    let mut before: Option<Value> = None;
    let mut returned = Vec::new();
    for sweep in 0..SWEEPS {
        history.sweep();
        let value = history.settle(FUNCTION, before.as_ref(), fresh_return(sweep));
        returned.push(value.clone());
        before = Some(value);
    }
    let tail = returned.get(SWEEPS - SETTLED_TAIL..).unwrap_or_default();
    assert!(
        tail.windows(2).all(|pair| pair.first() == pair.last()),
        "a return still moving after {SWEEPS} sweeps never reaches a fixed point"
    );
}
