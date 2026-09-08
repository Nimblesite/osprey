# Resumption multiplicity tests

Multiplicity says **how many times** an effect request may be answered, where
stage says *when*. It is declared per operation — `abort`, `once` (the default)
or `many`, with `replayable` marking an operation safe to re-perform — and the
whole axis exists because a handler that resumes twice re-runs the remainder of
the handled computation, so a body that sends an email sends it twice. See
[MULTI-AXIS](../../../docs/specs/0035-StagedEffects.md#multiplicity--multi-axis)
and [plan 0028](../../../docs/plans/0028-resumption-multiplicity.md).

This directory holds the executable half of the `[MULTI-FALSIFY]` gate: the
programs that must be **accepted**. Programs that must be **rejected** live in
[`examples/failscompilation`](../../../examples/failscompilation), one fixture
per check, each with its exact diagnostic.

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

- **the shared helper.** One unannotated `applyTwice(f, x)` serves a callback
  over a `replayable` operation and one over a plain `once` operation in the
  same program, with no multiplicity annotation and no multiplicity variable
  inferred — multiplicity is read from the declaration, so `[MULTI-STAGE-POLY]`
  falls out of erasure. This is as much of `[MULTI-FALSIFY]` case 4 as has a
  representation.
- **the contextual keywords.** `abort`, `once`, `many` and `replayable` are
  declared as operation NAMES, performed, handled, and bound as ordinary values
  in the same file. Nothing in this axis is reserved outside the marker slot of
  an operation declaration.

## Recorded gate outcomes

| `[MULTI-FALSIFY]` | Required | Measured |
|---|---|---|
| 1. Single-op retry | Accepted; one email | **Accepted.** This suite, green in both flavors and byte-exact under the default, `gc` and `arc` backends, with zero live objects at ARC exit |
| 2. Backtracking over impure code | Rejected, naming the non-replayable operation | **Rejected** at the handle site by `[MULTI-REPLAY-CHECK]`, in both flavors — `examples/failscompilation/multi_many_over_nonreplayable.ospo` |
| 3. Backtracking over pure code | Accepted; every alternative produced | **Cannot be written.** A `many` operation is rejected because no re-entrant continuation exists: native resume is one suspended stack, switched to and never switched back. Blocked on [plan 0016](../../../docs/plans/0016-algebraic-effects-and-handlers.md) |
| 4. Shared `map` under both multiplicities | Compiles with no annotation on the helper | **Accepted for `once` and `replayable`**, which is every multiplicity with a representation; the `many` half is blocked with case 3 |

Every row was measured in both flavors. `replayable` is read rather than
ignored: a `many` handler whose body performs only a `replayable` operation
passes the replay check and is then stopped by the missing representation, not
by the row.

Cases 3 and 4 are the gate working, not the gate failing. Rejecting a declared
`many` until the representation exists is the truthful answer — a `many` that
compiled to a one-shot continuation would be a silently wrong program, which is
the one outcome the language will not produce.

## A defect case 1 found

Writing case 1 in its natural spelling — a handler arm reading a promoted `mut`
straight into an inferred `Result`-returning helper — was rejected at lowering
with "`attempts` has no resolved signature". `genfn::alias_target` classified any
identifier absent from the scope as a bare callee name, and two ordinary
bindings are deliberately absent from it: a handler-promoted `mut` and a
file-scope binding read inside a function body. Both were aliased as functions.
Fixed at the classification site; pinned by
`a_handler_arm_reads_a_mut_cell_into_an_inferred_result_helper` in
[`crates/osprey-cli/tests/effect_installer_defects.rs`](../../../crates/osprey-cli/tests/effect_installer_defects.rs).

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
