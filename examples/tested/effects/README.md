# Algebraic Effects Examples

This directory is part of the golden example suite. Every runnable `.osp` file
has a sibling `.expectedoutput` file, and `crates/diff_examples.sh` compares the
program output byte-for-byte after trimming outer whitespace.

## Coverage

- `algebraic_effects_comprehensive.osp` covers multiple effects, effect sets,
  handlers, handler-owned state, mock IO, files, logging, and fibers.
- `handler_scoping.osp` covers nested handler override and forward-referenced
  functions that perform effects.
- `fiber_effects.osp` covers effects across spawned fibers.
- `http_state_levels.osp` covers handler-owned state across HTTP callback and
  fiber boundaries.
- `recoverable_errors.osp`, `result_and_effects.osp`, and
  `collect_all_errors.osp` cover direct value substitution with handler-owned
  state.
- `typed_error_channels.osp` covers nested handlers for independent operations.

## Explicit Resume Coverage

The paired Default/ML assertion suites under `tests/effects/resume/` cover
LIFO unwinding, Unit operations, aborting without resume, outer-handler
bridging, value rewriting, whole-Result resume values, and generic effect
instantiations. Each suite retains its former stdout transcript as an internal
oracle and adds handler-state assertions.

The remaining `abort_vs_resume.osp` and `retry_until_valid.osp` golden examples
compare resuming and non-resuming branches in one continuation-mode handler.

## Running

From the repo root:

```sh
zsh crates/diff_examples.sh effects
target/release/osprey test tests/effects/resume
```
