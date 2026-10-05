# Algebraic effect tests

These Default (`.osp`) and ML (`.ospml`) suites exercise the current effects implementation. Each pair shares an expected output. Start with the [runnable guide](../../examples/handlers/README.md); the [specification](../../docs/specs/0017-AlgebraicEffects.md) defines the contract and [plan 0016](../../docs/plans/0016-algebraic-effects-and-handlers.md) tracks unfinished work.

## Request work and choose its implementation

```osprey
effect Read { value: fn() -> int }
fn work() = perform Read.value()
let live = handler Read { value => 41 }
let test = handler Read { value => 7 }
print("${live(work)} ${test(work)}")
```

`handler` creates a callable value. Calling it with `work` installs its implementation before calling the function. `handle Read { value => 41 }` inside a block instead handles the rest of that block. The removed `handle ... in/do ...` forms are rejected.

An ordinary operation is a **value operation**: its arm returns the operation result and the caller continues. The [recovery suites](errors/README.md) test defaults, validation reports, shared state, generic results, and nested policies. The [injection suites](injection/README.md) run the same work under real and test storage/logging implementations.

## Control operations choose whether to continue

```osprey
effect Ask { control value: fn() -> int }
fn work() = perform Ask.value()
let continueWith = handler Ask { value => resume(41) }
let stop = handler Ask { value => 0 }
print("${continueWith(work)} ${stop(work)}")
```

`control` on the declaration selects continuation behavior. `resume(value)` supplies the operation result and runs the remaining work; its return is the completed handler answer. Returning without resuming abandons that work. Adding or removing an unreachable `resume` never changes an operation's mode.

A `return value => expression` clause transforms normal completion. A control arm's answer bypasses that clause. See the [resume suites](resume/README.md) and [answer-transform examples](../../examples/handlers/README.md#transforming-the-answer).

## Scope and checking

The innermost active matching handler answers an operation, including through helpers and callbacks. A partial handler covers only its declared arms and resolved generic instantiation. Arms run outside their own activation, allowing an operation to forward to an outer handler. Deep resumption reinstalls the suspended scope.

The compiler checks operation arguments, arm results, and required handlers at program entry. A function's effect annotation constrains its requirements; it does not install an implementation. Constructing a closure inside a handler does not handle calls made after that closure escapes. The current open-row callback support is a closed-program prototype; independently quantified rows remain unfinished.

## Static handlers and target support

`handle static E` and resolved `handler static E` values specialize all-value effects during compilation. The staged suites check captures, callback requirements, partial shadowing, and rejection of residual runtime dispatch. Removing dispatch does not imply all computation happens at compile time.

Value handlers and static discharge have portable native, WebAssembly, and mobile paths. Dynamic control operations currently require native execution, including control arms that never resume. Native resumption is deep and single-shot. The [multiplicity suites](multiplicity/README.md) distinguish retrying an operation from replaying the remaining computation; `many` remains unsupported.

## Run the tests

```sh
cargo build --release -p osprey-cli
target/release/osprey test tests/effects/errors
target/release/osprey test tests/effects/injection
target/release/osprey test tests/effects/resume
```

The differential corpus runner also compares both flavors across memory backends. Rejection fixtures live in [`examples/failscompilation`](../../examples/failscompilation). Compiler integration suites cover handler values, operation modes, answer transformations, and static selection.
