# Resumption multiplicity tests

Control operations declare how often their continuation may be resumed: `control abort`, `control once` (the default for `control`), or `control many`. `replayable` is a separate promise about repeating an operation. Retrying a payment request inside an arm is different from resuming twice and repeating the email in the remaining computation.

The [effects specification](../../../docs/specs/0017-AlgebraicEffects.md) defines multiplicity and replay. [Plan 0016](../../../docs/plans/0016-algebraic-effects-and-handlers.md) tracks delivery; reusable `many` continuations remain unsupported. This directory holds accepted cases; rejection fixtures live in [`examples/failscompilation`](../../../examples/failscompilation).

## What the paired suite pins

`multiplicity.test.{osp,ospml}` are twins sharing one golden. Two cases are
`[MULTI-FALSIFY]` case 1 and its mirror image:

- a `once` arm that **retries internally** — consulting the gateway a second
  time inside the arm — still answers its request exactly once, so the body's
  `Email.deliver` runs once whether or not the charge was retried;
- a `once` arm that **declines to resume** drops the continuation, so the
  receipt in the un-run remainder is never sent at all. `once` is affine, not
  linear: resuming zero times is always permitted.

The retry arm also holds two `resume` sites on different `match` branches. That
is the positive control for `[MULTI-HANDLE-ONCE]`, which is a rule about one
control **path**: reading it as "does this arm contain a `resume`" would reject
this program, and the shape is the sanctioned exception-style handler.

The effect is `Email.deliver`, not the spec's `Email.send`, only because `send`
is a hard keyword in the ML flavor and the twins must be the same program.

Two further cases cover the rest of the surface:

- **the shared helper.** One unannotated `applyTwice(f, x)` serves a callback over `Random.next`, a replayable control operation with default `once` multiplicity, and a callback over the value operation `Email.deliver`. This proves the helper works with both operation contracts. It does not establish quantified multiplicity or support for `many`.
- **the contextual keywords.** `abort`, `once`, `many` and `replayable` are
  declared as operation NAMES, performed, handled, and bound as ordinary values
  in the same file. Nothing in this axis is reserved outside the marker slot of
  an operation declaration.

## Recorded gate outcomes

| `[MULTI-FALSIFY]` | Required | Measured |
|---|---|---|
| 1. Single-op retry | Accepted; one email | **Accepted.** This suite, green in both flavors and byte-exact under the default, `gc` and `arc` backends, with zero live objects at ARC exit |
| 2. Backtracking over impure code | Rejected, naming the non-replayable operation | **Rejected** at the handle site by `[MULTI-REPLAY-CHECK]`, in both flavors — `examples/failscompilation/multi_many_over_nonreplayable.ospo` |
| 3. Backtracking over pure code | Accepted; every alternative produced | **Unsupported.** The native runtime can resume a captured continuation once and return its answer, but cannot clone or reuse it for another branch. Blocked on [plan 0016](../../../docs/plans/0016-algebraic-effects-and-handlers.md) |
| 4. Shared `map` under both multiplicities | Compiles with no annotation on the helper | **Partial evidence only.** The shared helper accepts a replayable `once` control operation and a value operation. Its `many` case remains blocked with case 3 |

Every row was measured in both flavors. `replayable` is read rather than
ignored: a `many` handler whose body performs only a `replayable` operation
passes the replay check and is then stopped by the missing representation, not
by the row.

Cases 3 and the `many` half of case 4 remain unmet acceptance requirements. Rejecting unsupported programs prevents an incorrect one-shot implementation from being presented as reusable continuations.

## Run them

```sh
target/release/osprey tests/effects/multiplicity/multiplicity.test.osp --run --quiet
zsh crates/run_test_corpus.sh gc
OSPREY_ARC_DEBUG=1 zsh crates/run_test_corpus.sh arc
```

Both need `resume`, which `wasm32` has no representation for, so both are listed
in [`tests/WASM_UNPORTABLE.txt`](../../WASM_UNPORTABLE.txt) — naming
`Charge.charge`, the operation whose request cannot be suspended, rather than the
`resume` keyword ([MULTI-WASM]).
