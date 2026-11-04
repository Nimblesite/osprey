use crate::testutil::{accepts, rejects_with};
use osprey_syntax::Flavor;

// ---------- Returned closures that apply a captured callback [EFFECTS-PROVENANCE] ----------
//
// `one(f) = fn(w) => f(w)` is what the ML flavor's `one f w = f w` curries to.
// The captured `f` belongs to `one`'s caller while `w` belongs to the returned
// closure, so substituting `f` must not re-attribute `w` to another binder:
// that once left a fully handled application undischargeable at the entry.

const DEFAULT_PRELUDE: &str = "effect Ask { value: fn() -> int }\n\
     effect Alarm { ring: fn() -> int }\n\
     fn work() = wrapAdd(perform Ask.value(), 1)\n\
     fn ring() = perform Alarm.ring()\n\
     fn run(g) = g()\n\
     let answer = handler Ask { value => 41 }\n\
     fn one(f) = fn(w) => f(w)\n\
     fn two(f) = fn(skip) => fn(w) => f(w)\n";

const ML_PRELUDE: &str = "effect Ask\n    value : Unit => int\n\
     effect Alarm\n    ring : Unit => int\n\
     work () = wrapAdd (perform Ask.value ()) 1\n\
     ring () = perform Alarm.ring ()\n\
     run g = g ()\n\
     answer = handler Ask\n    value => 41\n\
     one f w = f w\n\
     two f skip w = f w\n";

/// Each flavor's prelude followed by its spelling of one entry binding.
fn programs(default: &str, ml: &str) -> [(Flavor, String); 2] {
    [
        (Flavor::Default, format!("{DEFAULT_PRELUDE}{default}\n")),
        (Flavor::Ml, format!("{ML_PRELUDE}{ml}\n")),
    ]
}

#[test]
fn a_returned_closure_hands_its_own_argument_to_a_captured_handler() {
    for (flavor, source) in programs("let n = one(answer)(work)", "n = one answer work") {
        accepts(flavor, source);
    }
}

#[test]
fn a_doubly_returned_closure_hands_its_own_argument_to_a_captured_handler() {
    for (flavor, source) in programs("let n = two(answer)(0)(work)", "n = two answer 0 work") {
        accepts(flavor, source);
    }
}

#[test]
fn a_returned_closure_keeps_its_own_arguments_effect() {
    for (flavor, source) in programs("let n = one(run)(ring)", "n = one run ring") {
        rejects_with(flavor, source, "Alarm.ring");
    }
}

#[test]
fn a_captured_handler_discharges_only_its_own_effect() {
    for (flavor, source) in programs("let n = one(answer)(ring)", "n = one answer ring") {
        rejects_with(flavor, source, "Alarm.ring");
    }
}
