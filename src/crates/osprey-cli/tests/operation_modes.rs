//! Declared operation modes determine continuation behavior in both flavors.
//! Implements [EFFECTS-HANDLER-ARMS] and [EFFECTS-RESUME].

#[path = "common/effect_execution.rs"]
mod effect_execution;

use effect_execution::assert_flavored_output;

fn assert_flavors(name: &str, default: &str, ml: &str, python: &str, expected: &str) {
    assert_flavored_output(name, "osp", default, expected);
    assert_flavored_output(name, "ospml", ml, expected);
    assert_flavored_output(name, "py", python, expected);
}

#[test]
fn declared_modes_distinguish_value_return_control_return_and_resumption() {
    // A Python handler class is a named function, so, like a Default `fn`, it
    // cannot take a builtin's name such as `replace`.
    assert_flavors(
        "declared_modes",
        r#"
effect ValueAsk { value: fn() -> int }
effect ControlAsk { control value: fn() -> int }
fn valueWork() = satAdd(perform ValueAsk.value(), 1)
fn controlWork() = satAdd(perform ControlAsk.value(), 1)
let fixed = handler ValueAsk { value => 41 }
let around = handler ControlAsk { value => satAdd(resume(41), 100) }
let noResume = handler ControlAsk { value => 0 }
let deadResume = handler ControlAsk {
    value => match false { true => resume(0) false => 0 }
}
let replacement = fixed(valueWork)
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
valueWork () = satAdd (perform ValueAsk.value ()) 1
controlWork () = satAdd (perform ControlAsk.value ()) 1
fixed = handler ValueAsk
    value => 41
around = handler ControlAsk
    value => satAdd (resume 41) 100
noResume = handler ControlAsk
    value => 0
deadResume = handler ControlAsk
    value => match false
        true => resume 0
        false => 0
replacement = fixed valueWork
resumed = around controlWork
returned = noResume controlWork
deadBranch = deadResume controlWork
print "value=${replacement}\nresume=${resumed}\nno-resume=${returned}\ndead-resume=${deadBranch}"
"#,
        r#"@effect
class ValueAsk:
    def value() -> int: ...
@effect
class ControlAsk:
    @control
    def value() -> int: ...
def valueWork():
    return satAdd(perform(ValueAsk.value()), 1)
def controlWork():
    return satAdd(perform(ControlAsk.value()), 1)
@handler(ValueAsk)
class fixed:
    def value():
        return 41
@handler(ControlAsk)
class around:
    def value():
        return satAdd(resume(41), 100)
@handler(ControlAsk)
class noResume:
    def value():
        return 0
@handler(ControlAsk)
class deadResume:
    def value():
        return resume(0) if False else 0
replacement = fixed(valueWork)
resumed = around(controlWork)
returned = noResume(controlWork)
deadBranch = deadResume(controlWork)
print(f"value={replacement}\nresume={resumed}\nno-resume={returned}\ndead-resume={deadBranch}")
"#,
        "value=42\nresume=142\nno-resume=0\ndead-resume=0\n",
    );
}

#[test]
fn mixed_effect_operations_keep_their_own_modes() {
    assert_flavors(
        "mixed_modes",
        r#"
effect Mixed { plain: fn() -> int control stop: fn() -> int }
fn work() = {
    let first = perform Mixed.plain()
    satAdd(first, perform Mixed.stop())
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
    satAdd first (perform Mixed.stop ())
stopping = handler Mixed
    plain => 41
    stop => 0
continuing = handler Mixed
    plain => 41
    stop => resume 1
print "${stopping work}:${continuing work}"
"#,
        r#"@effect
class Mixed:
    def plain() -> int: ...
    @control
    def stop() -> int: ...
def work():
    first = perform(Mixed.plain())
    return satAdd(first, perform(Mixed.stop()))
@handler(Mixed)
class stopping:
    def plain():
        return 41
    def stop():
        return 0
@handler(Mixed)
class continuing:
    def plain():
        return 41
    def stop():
        return resume(1)
print(f"{stopping(work)}:{continuing(work)}")
"#,
        "0:42\n",
    );
}

#[test]
fn value_and_control_arms_forward_to_the_outer_activation() {
    assert_flavors(
        "forward_modes",
        r#"
effect ValueAsk { value: fn() -> int }
effect ControlAsk { control value: fn() -> int }
let value = {
    handle ValueAsk { value => 40 }
    handle ValueAsk { value => satAdd(perform ValueAsk.value(), 1) }
    satAdd(perform ValueAsk.value(), 1)
}
let control = {
    handle ControlAsk { value => resume(40) }
    handle ControlAsk { value => resume(satAdd(perform ControlAsk.value(), 1)) }
    satAdd(perform ControlAsk.value(), 1)
}
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
    handle ValueAsk
        value => satAdd (perform ValueAsk.value ()) 1
    satAdd (perform ValueAsk.value ()) 1
control =
    handle ControlAsk
        value => resume 40
    handle ControlAsk
        value => resume (satAdd (perform ControlAsk.value ()) 1)
    satAdd (perform ControlAsk.value ()) 1
print "${value}:${control}"
"#,
        r#"@effect
class ValueAsk:
    def value() -> int: ...
@effect
class ControlAsk:
    @control
    def value() -> int: ...
@handler(ValueAsk)
class ValueOuter:
    def value():
        return 40
@handler(ValueAsk)
class ValueInner:
    def value():
        return satAdd(perform(ValueAsk.value()), 1)
@handler(ControlAsk)
class ControlOuter:
    def value():
        return resume(40)
@handler(ControlAsk)
class ControlInner:
    def value():
        return resume(satAdd(perform(ControlAsk.value()), 1))
def valueWork():
    with ValueOuter:
        with ValueInner:
            return satAdd(perform(ValueAsk.value()), 1)
def controlWork():
    with ControlOuter:
        with ControlInner:
            return satAdd(perform(ControlAsk.value()), 1)
print(f"{valueWork()}:{controlWork()}")
"#,
        "42:42\n",
    );
}

#[test]
fn deep_resumption_handles_later_requests_and_generic_payloads() {
    assert_flavors(
        "deep_generic",
        r#"
effect Ask<T> { control value: fn() -> T }
let numbers = handler Ask<int> { value => resume(20) }
let names = handler Ask<string> { value => resume("Ada") }
fn twice() = {
    let first = perform Ask<int>.value()
    let second = perform Ask<int>.value()
    satAdd(first, second)
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
    satAdd first second
total = numbers twice
name = names (\() => perform Ask<string>.value ())
print "${total}:${name}"
"#,
        r#"@effect
class Ask[T]:
    @control
    def value() -> T: ...
@handler(Ask[int])
class numbers:
    def value():
        return resume(20)
@handler(Ask[str])
class names:
    def value():
        return resume("Ada")
def twice():
    first = perform(Ask[int].value())
    second = perform(Ask[int].value())
    return satAdd(first, second)
total = numbers(twice)
name = names(lambda: perform(Ask[str].value()))
print(f"{total}:{name}")
"#,
        "40:Ada\n",
    );
}

#[test]
fn python_generic_static_handlers_discharge_exact_instantiations() {
    assert_flavored_output("python_static", "py", PYTHON_STATIC, "42:Ada!\n");
}

const PYTHON_STATIC: &str = r#"@static_effect
class Read[T]:
    def read(value: T) -> T: ...
@static_handler(Read[int])
class Numbers:
    def read(value):
        return satAdd(value, 1)
@static_handler(Read[str])
class Names:
    def read(value):
        return value + "!"
def number():
    with Numbers:
        return perform(Read[int].read(41))
name = Names(lambda: perform(Read[str].read("Ada")))
print(f"{number()}:{name}")
"#;

#[test]
fn python_fibers_run_inferred_callbacks_with_local_handlers() {
    assert_flavored_output("python_fibers", "py", PYTHON_FIBERS, "21\n");
}

const PYTHON_FIBERS: &str = r#"@effect
class Ask:
    def value() -> int: ...
@handler(Ask)
class Answer:
    def value():
        return 21
def child():
    yield_fiber()
    return Answer(lambda: perform(Ask.value()))
def main():
    task = spawn(child)
    result = await_fiber(task)
    print(f"{result}")
"#;
