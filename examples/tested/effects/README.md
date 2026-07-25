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

## Explicit Resume Examples

- `resume_lifo_audit.osp` shows post-`resume` code unwinding in LIFO order.
- `resume_unit_markers.osp` shows `resume()` for a `Unit` operation.
- `resume_abort_early_exit.osp` shows an arm returning without `resume`, which
  aborts the suspended continuation and becomes the whole handler result.
- `resume_outer_handler_bridge.osp` shows a resumed body keeping outer handlers
  installed.
- `resume_value_rewrite.osp` shows the handler choosing operation results and
  observing the final answer after each continuation returns.
- `abort_vs_resume.osp` and `retry_until_valid.osp` compare resuming and
  non-resuming branches in one continuation-mode handler.

## Running

From the repo root:

```sh
zsh crates/diff_examples.sh effects
zsh crates/diff_examples.sh resume_
```
