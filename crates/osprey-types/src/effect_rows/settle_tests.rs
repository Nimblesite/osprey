//! Fixed-program recursive provenance must settle without erasing finite chains.
use super::{advance_return, ReturnHistory, Value};
use crate::effect_rows::{Callable, KnownCallable, Summary};

const FUNCTION: usize = 0;
const SWEEPS: usize = 64;
const SETTLED_TAIL: usize = 8;
const RECURSIVE_STEP: &str =
    "type Step={next:fn()->Step}\nfn step()=Step{next:fn()=>step()}\nlet first=step()\n";

/// One unfolding of `type Step={next:fn()->Step}; fn step()=Step{next:fn()=>step()}`.
/// Parameter indices belong to fixed declarations; creating a new index each
/// sweep would demand stabilization of changing programs rather than one program.
fn fresh_return(depth: usize) -> Value {
    (0..depth).fold(Value::default(), |returned, _| Value {
        fields: [("next".to_owned(), returned_step(returned))]
            .into_iter()
            .collect(),
        ..Value::default()
    })
}

fn returned_step(returned: Value) -> Value {
    Value::from_callable(Callable::Known(Box::new(KnownCallable {
        parameters: Vec::new(),
        summary: Summary::default(),
        returned: Some(Box::new(returned)),
    })))
}

#[test]
fn recursive_returns_settle_at_existing_provenance_limits() {
    crate::effect_rows_tests::assert_accepted(RECURSIVE_STEP);
    let mut history = ReturnHistory::default();
    let mut before: Option<Value> = None;
    let mut returned = Vec::new();
    for sweep in 0..SWEEPS {
        let _ = advance_return(&mut before, fresh_return(sweep), FUNCTION, &mut history);
        returned.push(before.clone());
    }
    let tail = returned.get(SWEEPS - SETTLED_TAIL..).unwrap_or_default();
    assert!(
        tail.windows(2).all(|pair| pair.first() == pair.last()),
        "a return still moving after {SWEEPS} sweeps never reaches a fixed point"
    );
}

#[test]
fn finite_returned_function_spines_keep_their_precise_provenance() {
    for depth in [20, 30] {
        let links = (0..depth - 1)
            .map(|index| format!("fn link{index}()=link{}", index + 1))
            .collect::<Vec<_>>()
            .join("\n");
        let calls = "()".repeat(depth);
        let last = depth - 1;
        crate::effect_rows_tests::assert_accepted(&format!(
            "{links}\nfn link{last}()=7\nlet answer=link0{calls}\n"
        ));
    }
}
