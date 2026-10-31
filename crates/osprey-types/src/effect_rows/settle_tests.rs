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

/// Fields one fixed declaration can return: a finite domain, so the sequence
/// below draws only on shapes the same program could produce.
const FIELDS: usize = 10;

/// A return whose every sweep differs from all earlier ones, as the proof
/// kernel's mutually recursive descent produced: sweep `n` returns the fields
/// set in the Gray code of `n`, so no value repeats within `2^FIELDS` sweeps
/// and no revisit ever triggers the join.
fn never_revisiting_return(sweep: usize) -> Value {
    let gray = sweep ^ (sweep >> 1);
    Value {
        fields: (0..FIELDS)
            .filter(|field| gray & (1 << field) != 0)
            .map(|field| (format!("field{field}"), Value::channel(field)))
            .collect(),
        ..Value::default()
    }
}

#[test]
fn recursive_returns_that_never_revisit_still_settle() {
    let mut history = ReturnHistory::new([FUNCTION].into());
    let mut before: Option<Value> = None;
    let mut returned = Vec::new();
    for sweep in 0..SWEEPS {
        let _ = advance_return(
            &mut before,
            never_revisiting_return(sweep),
            FUNCTION,
            &mut history,
        );
        returned.push(before.clone());
    }
    let tail = returned.get(SWEEPS - SETTLED_TAIL..).unwrap_or_default();
    assert!(
        tail.windows(2).all(|pair| pair.first() == pair.last()),
        "a return that never revisits is still moving after {SWEEPS} sweeps"
    );
}

/// `relay0 … relay{n-1}` hand a callback around one cycle of `n` functions,
/// closed by the last relay's fallback call back to the first.
fn mutual_relay(functions: usize, handled: bool) -> String {
    let relays = (0..functions - 1)
        .map(|index| format!("fn relay{index}(stop)=relay{}(stop)", index + 1))
        .collect::<Vec<_>>()
        .join("\n");
    let last = functions - 1;
    let call = "let callback=relay0(true)\ncallback()";
    let body = if handled {
        format!("handle Alarm {{ ring => 7 }}\n{call}")
    } else {
        call.to_owned()
    };
    format!(
        "effect Alarm{{ring:fn()->int}}\n{relays}\n\
         fn relay{last}(stop)=if stop {{ fn()=>perform Alarm.ring() }} else {{ relay0(true) }}\n\
         let answer={{\n{body}\n}}\n"
    )
}

#[test]
fn long_mutual_recursion_cycles_keep_their_precise_callback() {
    for functions in [4, 20, 30] {
        crate::effect_rows_tests::assert_accepted(&mutual_relay(functions, true));
        crate::effect_rows_tests::assert_rejected_with(
            &mutual_relay(functions, false),
            &["Alarm.ring"],
        );
    }
}

/// `spine0 … spine{n-1}` each return a closure calling the next, so the entry
/// unwraps `n - 1` closures; the last spine's fallback reaches back to the
/// first, making all of them one cycle whose return grows for `n` sweeps.
fn mutual_spine(functions: usize, tail: &str, handled: bool) -> String {
    let spines = (0..functions - 1)
        .map(|index| format!("fn spine{index}(stop)=fn()=>spine{}(true)", index + 1))
        .collect::<Vec<_>>()
        .join("\n");
    let last = functions - 1;
    let call = format!("spine0(true){}", "()".repeat(last));
    let body = if handled {
        format!("handle Alarm {{ ring => 7 }}\n{call}")
    } else {
        call
    };
    format!(
        "effect Alarm{{ring:fn()->int}}\nfn finish(callback)=0\n{spines}\n\
         fn spine{last}(stop)={{\nlet ignored=if stop {{ 0 }} else {{ finish(spine0(true)) }}\n{tail}\n}}\n\
         let answer={{\n{body}\n}}\n"
    )
}

#[test]
fn long_recursive_spines_keep_their_precise_provenance() {
    for functions in [4, 20, 30] {
        crate::effect_rows_tests::assert_accepted(&mutual_spine(functions, "7", false));
        crate::effect_rows_tests::assert_accepted(&mutual_spine(
            functions,
            "perform Alarm.ring()",
            true,
        ));
        crate::effect_rows_tests::assert_rejected_with(
            &mutual_spine(functions, "perform Alarm.ring()", false),
            &["Alarm.ring"],
        );
    }
}
