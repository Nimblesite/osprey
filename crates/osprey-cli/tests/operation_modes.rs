//! Declared operation modes determine continuation behavior in both flavors.
//! Implements [EFFECTS-HANDLER-ARMS] and [EFFECTS-RESUME].

#[path = "common/effect_execution.rs"]
mod effect_execution;

use effect_execution::assert_flavored_output;

fn assert_both(name: &str, default: &str, ml: &str, expected: &str) {
    assert_flavored_output(name, "osp", default, expected);
    assert_flavored_output(name, "ospml", ml, expected);
}

#[test]
fn declared_modes_distinguish_value_return_control_return_and_resumption() {
    assert_both(
        "declared_modes",
        r#"
effect ValueAsk { value: fn() -> int }
effect ControlAsk { control value: fn() -> int }
fn valueWork() = (perform ValueAsk.value() + 1) ?: 0
fn controlWork() = (perform ControlAsk.value() + 1) ?: 0
let replace = handler ValueAsk { value => 41 }
let around = handler ControlAsk { value => (resume(41) + 100) ?: 0 }
let noResume = handler ControlAsk { value => 0 }
let deadResume = handler ControlAsk {
    value => match false { true => resume(0) false => 0 }
}
let replacement = replace(valueWork)
let resumed = around(controlWork)
let returned = noResume(controlWork)
let deadBranch = deadResume(controlWork)
print("value=${replacement}\nresume=${resumed}\nno-resume=${returned}\ndead-resume=${deadBranch}")
"#,
        r#"
effect ValueAsk
    value : Unit => int
effect ControlAsk
    control value : Unit => int
valueWork () = (perform ValueAsk.value () + 1) ?: 0
controlWork () = (perform ControlAsk.value () + 1) ?: 0
replace = handler ValueAsk
    value => 41
around = handler ControlAsk
    value => ((resume 41) + 100) ?: 0
noResume = handler ControlAsk
    value => 0
deadResume = handler ControlAsk
    value => match false
        true => resume 0
        false => 0
replacement = replace valueWork
resumed = around controlWork
returned = noResume controlWork
deadBranch = deadResume controlWork
print "value=${replacement}\nresume=${resumed}\nno-resume=${returned}\ndead-resume=${deadBranch}"
"#,
        "value=42\nresume=142\nno-resume=0\ndead-resume=0\n",
    );
}

#[test]
fn mixed_effect_operations_keep_their_own_modes() {
    assert_both(
        "mixed_modes",
        r#"
effect Mixed { plain: fn() -> int control stop: fn() -> int }
fn work() = {
    let first = perform Mixed.plain()
    (first + perform Mixed.stop()) ?: 0
}
let stopping = handler Mixed { plain => 41 stop => 0 }
let continuing = handler Mixed { plain => 41 stop => resume(1) }
print("${stopping(work)}:${continuing(work)}")
"#,
        r#"
effect Mixed
    plain : Unit => int
    control stop : Unit => int
work () =
    first = perform Mixed.plain ()
    (first + perform Mixed.stop ()) ?: 0
stopping = handler Mixed
    plain => 41
    stop => 0
continuing = handler Mixed
    plain => 41
    stop => resume 1
print "${stopping work}:${continuing work}"
"#,
        "0:42\n",
    );
}

#[test]
fn value_and_control_arms_forward_to_the_outer_activation() {
    assert_both(
        "forward_modes",
        r#"
effect ValueAsk { value: fn() -> int }
effect ControlAsk { control value: fn() -> int }
let value = handle ValueAsk value => 40 in
    handle ValueAsk value => (perform ValueAsk.value() + 1) ?: 0 in
        (perform ValueAsk.value() + 1) ?: 0
let control = handle ControlAsk value => resume(40) in
    handle ControlAsk value => resume((perform ControlAsk.value() + 1) ?: 0) in
        (perform ControlAsk.value() + 1) ?: 0
print("${value}:${control}")
"#,
        r#"
effect ValueAsk
    value : Unit => int
effect ControlAsk
    control value : Unit => int
value =
    handle ValueAsk
        value => 40
    in
        handle ValueAsk
            value => (perform ValueAsk.value () + 1) ?: 0
        in (perform ValueAsk.value () + 1) ?: 0
control =
    handle ControlAsk
        value => resume 40
    in
        handle ControlAsk
            value => resume ((perform ControlAsk.value () + 1) ?: 0)
        in (perform ControlAsk.value () + 1) ?: 0
print "${value}:${control}"
"#,
        "42:42\n",
    );
}

#[test]
fn deep_resumption_handles_later_requests_and_generic_payloads() {
    assert_both(
        "deep_generic",
        r#"
effect Ask<T> { control value: fn() -> T }
let numbers = handler Ask<int> { value => resume(20) }
let names = handler Ask<string> { value => resume("Ada") }
fn twice() = {
    let first = perform Ask<int>.value()
    let second = perform Ask<int>.value()
    (first + second) ?: 0
}
let total = numbers(twice)
let name = names(|| => perform Ask<string>.value())
print("${total}:${name}")
"#,
        r#"
effect Ask T
    control value : Unit => T
numbers = handler Ask<int>
    value => resume 20
names = handler Ask<string>
    value => resume "Ada"
twice () =
    first = perform Ask<int>.value ()
    second = perform Ask<int>.value ()
    (first + second) ?: 0
total = numbers twice
name = names (\() => perform Ask<string>.value ())
print "${total}:${name}"
"#,
        "40:Ada\n",
    );
}
