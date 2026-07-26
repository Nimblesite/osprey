# Algebraic effects examples

Every example in this directory has a Default (`.osp`) and ML (`.ospml`) form.
Each pair is one program written in Osprey's two first-class syntax flavors. The
golden harness runs both and compares them with the same `.osp.expectedoutput`
file after trimming only the outer whitespace.

## Start with the two handler behaviors

A handler region with no `resume` uses direct substitution: the arm's value
becomes the current `perform` result and the caller continues. Start with
[recoverable_errors.osp](recoverable_errors.osp) or its
[ML twin](recoverable_errors.ospml) to see one order workflow run under two
fallback policies.

A handler region containing `resume` runs as a suspended continuation.
`resume(value)` continues from `perform`; returning from the selected branch
without `resume` stops the remaining computation. See
[abort_vs_resume.osp](abort_vs_resume.osp) and
[abort_vs_resume.ospml](abort_vs_resume.ospml), then the retry examples.

Mode is currently selected for the whole handler region. A resuming operation
can therefore change how a non-resuming sibling operation behaves. This is
tracked by [issue #177](https://github.com/Nimblesite/osprey/issues/177). The
examples keep sibling operations in one mode and use resume-or-stop branches
inside one operation arm when they need exception-style early exit.

## Learning path

| Behavior | Default | ML | Handler path |
| --- | --- | --- | --- |
| Supply different defaults for one workflow | [recoverable errors](recoverable_errors.osp) | [recoverable errors](recoverable_errors.ospml) | Direct |
| Turn local `Result` failures into a surrounding recovery policy | [Result and effects](result_and_effects.osp) | [Result and effects](result_and_effects.ospml) | Direct |
| Report every validation failure in one pass | [collect all errors](collect_all_errors.osp) | [collect all errors](collect_all_errors.ospml) | Direct |
| Keep parse, missing and range failures in separate typed channels | [typed error channels](typed_error_channels.osp) | [typed error channels](typed_error_channels.ospml) | Direct |
| Compare continuing with stopping | [abort vs resume](abort_vs_resume.osp) | [abort vs resume](abort_vs_resume.ospml) | Explicit resume |
| Retry until valid, then stop after a budget | [retry until valid](retry_until_valid.osp) | [retry until valid](retry_until_valid.ospml) | Explicit resume |
| Override and restore nested handlers | [handler scoping](handler_scoping.osp) | [handler scoping](handler_scoping.ospml) | Direct and explicit resume |
| Preserve handlers across spawned fibers | [fiber effects](fiber_effects.osp) | [fiber effects](fiber_effects.ospml) | Direct and explicit resume |
| Preserve handler-owned state through HTTP callbacks and fibers | [HTTP state levels](http_state_levels.osp) | [HTTP state levels](http_state_levels.ospml) | Direct |
| Combine effects, state, mock IO, files, logging and fibers | [comprehensive](algebraic_effects_comprehensive.osp) | [comprehensive](algebraic_effects_comprehensive.ospml) | Direct |

## Important limits

- The compiler checks operation inputs and outputs, but does not yet catch every
  missing handler before execution. A missing setup can stop at runtime.
- Explicit resume is deep, single-shot and native-only. Direct substitution
  handlers do not need the continuation runtime and are supported on
  WebAssembly.
- A second resume of one completed continuation is rejected.
- `resume` cannot be captured by a lambda declared inside a handler arm.
- First-class handler values are not implemented yet.
- Direct handlers currently corrupt whole `Result<T, E>` operation values;
  [critical issue #183](https://github.com/Nimblesite/osprey/issues/183) tracks
  that boundary. `result_and_effects` safely matches a local `Result` first and
  asks its effect for a plain replacement value.
- A resuming handler whose completed continuation answer is a dynamic string
  currently leaks under ARC. The paired assertion suite keeps this visible as
  [critical issue #185](https://github.com/Nimblesite/osprey/issues/185).

The paired assertion suites in [tests/effects](../../../tests/effects/README.md)
cover internal values, counts and ordering that stdout cannot prove. Their
known-failure skips also link the current critical argument and `Result` bugs.

## Run everything in this area

From the repository root:

```sh
zsh crates/diff_examples.sh effects
target/release/osprey test tests/effects
```
