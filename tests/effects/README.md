# Algebraic effect tests

These assertion suites are executable documentation for Osprey's algebraic
effects. Every behavior is tested in both Default (`.osp`) and ML (`.ospml`)
syntax. The two files in each pair describe the same program and compile through
the same checker and native backend.

## The basic model

An `effect` names an operation and its input and output types. `perform` asks the
nearest matching handler to carry out that operation. `handle … in` installs the
policy around the code that may ask.

The operation result always has the declared type. What happens around that
result depends on whether the handler region uses `resume`.

### Direct substitution

If no arm in a handler region contains `resume`, the arm's return value becomes
the result of `perform`. The caller continues immediately after the operation.
This is useful for defaults, test doubles, collecting validation errors and
replacing infrastructure without passing it through every intermediate
function.

For example, if `Missing.value` returns `int`, this handler makes the failed
operation evaluate to `42`:

```osprey
handle Missing
    value key => 42
in load()
```

Returning `42` here does **not** stop `load`. It supplies a value and lets
`load` continue. The paired [direct recovery suites](errors/README.md) exercise
that rule across nested policies, repeated failures, handler-owned state,
multiple result types, recursion and outer effects.

### Explicit resume and early exit

If any arm in a handler region contains `resume`, Osprey runs the handled code
as a suspended continuation. `resume(value)` supplies the current operation
result and runs the rest of that code. When it finishes, `resume` itself returns
the completed answer to the handler arm.

In that continuation mode, returning from an active branch without calling
`resume` stops the suspended computation. That branch's value becomes the value
of the whole `handle … in` expression. This is Osprey's exception-style early
exit: the operation is named and typed, and the handler chooses whether to
continue or stop.

The common shape is one operation arm with two branches:

```osprey
handle ParseFailure
    parse text => match parseInt(text) {
        Success { value } => resume(value)
        Error { message } => 1
    }
in boot()
```

The success branch continues `boot`; the error branch stops it and makes the
whole handler evaluate to `1`. The paired [resume suites](resume/README.md)
cover recovery, retry, early exit, nested handlers, unwind order, typed values
and repeated suspension.

Handler mode is currently selected for the whole region, not independently for
each operation arm. Mixing a resuming operation with a non-resuming sibling can
therefore change the sibling from substitution to early exit. This is tracked by
[issue #177](https://github.com/Nimblesite/osprey/issues/177). Branching between
`resume` and early exit inside one operation arm is the intended exception-style
pattern.

## Choosing an error shape

| Need | Use | Behavior |
| --- | --- | --- |
| The immediate caller needs the failure | `Result<T, E>` | Match `Success` and `Error` as ordinary data |
| A surrounding policy should supply a default | Direct effect handler | Return the operation value; the caller continues |
| Validation should report every problem | Direct `Unit` operation | Handle each report and continue checking |
| A policy should retry or rewrite a value | Resuming handler | Call `resume(value)` |
| A policy should stop the remaining work | Resuming handler | Return from the selected branch without `resume` |

## What the compiler checks today

The compiler checks that effects, operations, arguments, handler parameters and
operation results agree. It does not yet prove that every call has a matching
handler. A missing handler can compile and then stop at runtime with an
`unhandled effect` diagnostic. Function effect rows also do not yet propagate
through every call.

Explicit resume is deep, single-shot and native-only. A resumed computation may
perform the same effect again because its handler stays installed. Resuming the
same completed continuation twice is rejected. WebAssembly supports direct
substitution handlers, but not the current pthread-backed resume runtime.

## Current critical defects

The suites keep known failures visible as TAP skips rather than reporting them
as successes or making unrelated CI unusable:

- [#182](https://github.com/Nimblesite/osprey/issues/182): resumable operations
  silently replace arguments after the 16th with zero. The paired resume suite
  proves all 16 supported positions and skips the unsafe 17th-position repro.
- [#183](https://github.com/Nimblesite/osprey/issues/183): direct handlers
  corrupt whole `Result<T, E>` operation values. Keep `Result` outside the
  direct operation boundary until this is fixed; explicit-resume transport has
  separate passing coverage.
- [#184](https://github.com/Nimblesite/osprey/issues/184): a four-argument
  curried ML function can silently skip handled effects. The paired golden
  examples use the verified tuple-parameter form while the issue retains the
  failing and passing reproducers.
- [#185](https://github.com/Nimblesite/osprey/issues/185): a resuming handler
  whose completed continuation answer is a dynamic string leaks an ARC object.
  String operation values remain covered with scalar final answers; the leaking
  managed-answer shape is an explicit paired skip.

## Invalid programs

The must-reject corpus under
[`examples/failscompilation`](../../examples/failscompilation) checks both
frontends for:

- the wrong number of operation arguments;
- a direct handler returning the wrong operation type;
- `resume` receiving the wrong value type;
- `resume` outside a handler arm; and
- one generic recovery region being forced to incompatible types.

Default fixtures use names such as `effect_perform_arity_mismatch.ospo`; their
ML twins use the `ml_` prefix and an explicit flavor marker. Each fixture has an
`.expectedoutput` file documenting the intended diagnostic. The harness requires
every program to exit nonzero; focused diagnostic checks compare the captured
message with that documented output.

## Run the tests

```sh
target/release/osprey test tests/effects
zsh crates/diff_examples.sh effects
zsh crates/run_test_corpus.sh gc
OSPREY_ARC_DEBUG=1 zsh crates/run_test_corpus.sh arc
```

The first command runs internal assertions. The golden harness checks the
documented programs in `examples/tested/effects`. The final two commands repeat
the assertion corpus under tracing GC and ARC; ARC also requires zero live
objects at exit.
