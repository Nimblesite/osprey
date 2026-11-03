# Plan 0016 — Algebraic effects and handlers

**Contract:** [Algebraic Effects](../specs/language/0017-AlgebraicEffects.md). Tooling tasks also touch [Language Server](../specs/tooling/0020-LanguageServerAndEditors.md), [Debugger](../specs/tooling/0021-Debugger.md), [WebAssembly target](../specs/compiler/0022-WebAssemblyTarget.md).
**Depends on:** [Plan 0026 — Structured Concurrency](0026-structured-concurrency.md) (region unwinding and cancellation-time finalization for `control abort`).

## Prerequisites in place

Callable handler values, block-scoped `handle`, declared value/control modes, return transformations, static handler selection, `finally`/`mask`/`instance`, transport fixes, reactive signals, checker-backed LSP effect-requirement and implementation views, and the closed-program open-row prototype are implemented; see audit for evidence. The compile-time rejections for `many` replay safety and for `abort`/`once` misuse already exist and stay.

## Phase 1 — Scoped row polymorphism

- [ ] Put latent effect rows into `Type::Fun` and independently quantify, unify and discharge scoped row tails ([EFFECTS-ROW-POLY], [EFFECTS-GENERIC-ROWS], [STAGE-POLY-PREREQ]). Files: `crates/osprey-types/src/ty.rs` (`Type::Fun` has no row field today), `crates/osprey-types/src/effect_rows/{check,solver}.rs`. Cover upper bounds, duplicate labels, exact operation/instance discharge, occurs checks, shared-tail unification (`<E|r> = <F|r>` has no finite solution; freshening must not discard the shared-tail constraint), callbacks returning callbacks, record fields, shadowing, separate compilation. Reuse old inference cases as regression evidence only. Acceptance: new cases in `crates/osprey-types/src/effect_rows_tests.rs` and `crates/osprey-types/src/effect_rows_exports_tests.rs` pass; one `!e` helper instantiates at distinct call sites with distinct rows.

- [ ] Implement invocation-time specialization so an escaping callable or lazy iterator carrying a statically handled latent operation is discharged against its invocation handler, not its construction handler ([EFFECTS-STATIC-DISCHARGE], [STAGE-LOWER-ORDER]). Files: `crates/osprey-types/src/effect_rows/validate_handlers.rs` (`validate_static_escape` fails closed today), `crates/osprey-codegen/src/effects.rs`. Acceptance: an escaped callable/lazy iterator under a static handler compiles with no effect dispatch in the emitted IR; add cases to `crates/osprey-cli/tests/staged_hygiene.rs`.

## Phase 2 — Continuation ownership and lifetime

- [ ] Implement owned continuation values: bare `resume` denotes the continuation; transferring it removes the former owner's right; `drop(k)`; a `once` call consumes it, a `many` call borrows a reusable snapshot ([EFFECTS-CONTINUATION-OWNERSHIP], [EFFECTS-RESUME-NESTING]). Files: `crates/osprey-ast/src/resume.rs` (only counts syntactic uses today), `crates/osprey-types/src/multiplicity.rs`, codegen/runtime continuation layout. Cover saved resumptions invoked after the arm returns, rejected borrowed escape, double consumption, deep nesting, cancellation, abandonment, HTTP callbacks, fiber ownership. Audit default/GC/ARC; output correctness alone cannot prove resource correctness (zero ARC leak). Acceptance: both-flavor positive and negative cases for each shape above, plus ARC zero-leak on transferred and dropped continuations.

- [ ] Give `control abort` a runnable case: an `abort` arm abandons the region, runs its finalizers inner to outer, releases owned operands, and its `perform` never returns ([MULTI-DECL-ABORT-RESULT], [MULTI-HANDLE-ABORT-MODE], [MULTI-COST-ABORT], [EFFECTS-FINALIZATION]). Files: remove `unimplemented_abort` in `crates/osprey-types/src/multiplicity.rs`; add unwinding/release in `compiler/runtime/effects_runtime.c` / `effects_coro.c`. Blocked on [Plan 0026](0026-structured-concurrency.md) unwinding. Acceptance: a runnable abort case in `crates/osprey-cli/tests/staged_multiplicity.rs` replaces the current rejection assertions and proves finalizers and operand release.

- [ ] Audit abandoned native continuation frames: prove a dropped/cancelled continuation releases every owned operand and runs its finalizers exactly once; pthread cancellation/exit alone is not evidence ([MULTI-COST-ABORT], [EFFECTS-FINALIZATION]). Acceptance: finalizer order and zero ARC leak asserted for abandoned continuations under default/GC/ARC; shared with [Plan 0026](0026-structured-concurrency.md) kill-safety.

## Phase 3 — Reusable continuations

- [ ] Build a reusable continuation representation (CPS/frames or another portable backend) that preserves the captured remainder independently per branch, replacing the single-shot pthread stack ([MULTI-HANDLE-MANY], [MULTI-HANDLE-MANY-LEXICAL], [MULTI-COST]). Files: `compiler/runtime/effects_coro.c` / `effects_runtime.c`, `crates/osprey-codegen`. Remove `unimplemented_many` in `crates/osprey-types/src/multiplicity.rs`. Keep the existing handle-site rejections (`many` over a non-replayable body, captured mutable state, or a fiber-spanning continuation) as fixtures ([MULTI-REPLAY-CHECK], [MULTI-REPLAY-STATE], [MULTI-REPLAY-FIBER]). Acceptance: a pure search yields all alternatives; independent branch state; no replay of work before capture; retrying one operation does not replay later work; add a positive `many` case to `crates/osprey-cli/tests/staged_multiplicity.rs` and `tests/effects/multiplicity/`.

## Phase 4 — Targets, traces, editor

- [ ] Run continuation programs on native and wasm32 with identical answers; support `once` on wasm32 when the stack-switching backend provides it and keep `many` portable (no permanent ban); keep rejecting missing capabilities before linking, naming the operation ([MULTI-WASM], [STAGE-WASM]). Files: `crates/osprey-cli/src/target_capabilities.rs`, `crates/osprey-cli/tests/target_capabilities.rs`, `crates/osprey-cli/tests/staged_multiplicity.rs`. Acceptance: `ONCE_ON_WASM` compiles and runs; `MANY_ON_WASM` runs once a portable representation exists; capability rejections still name the operation.

- [ ] Ship the effect trace: a debug build and DAP view record the resolved perform site, actual runtime handler instance and resumption branch, innermost first; static-discharged sites are excluded ([MULTI-TRACE], [DEBUGGER-EFFECT-TRACE]). Files: `crates/osprey-debug`, debug codegen, VS Code extension DAP view. Acceptance: a paused session shows the trace and selecting an entry navigates to the perform or arm.

- [ ] Ship the LSP replay-boundary view ([LSP-EFFECT-MULTIPLICITY]): on a `many` handler arm, `textDocument/implementation` returns the perform sites that arm can answer (its replay set); a handler region whose arms are all tail-resumptive over `once` operations publishes the "declaring Log static would remove it from the runtime" hint. Files: `crates/osprey-lsp/src/effects/implementations.rs`, `crates/osprey-lsp/src/effects/tests.rs`. Acceptance: tests pin both reports.

- [ ] Align `tree-sitter-osprey-ml/grammar.js` `operation_declaration` with the parser: the editor grammar accepts a multiplicity without `control` (`many op : …`), which the ML parser (`crates/osprey-syntax/src/ml/parser/effect_declarations.rs`, `operation_markers`) and [MULTI-DECL] reject; nest `multiplicity` under `operation_mode` in the editor grammar ([MULTI-DECL]). Acceptance: `many op : int => int` fails to parse as an operation declaration in the ML grammar corpus.

## Constraints

- Each task needs executable positive and negative cases in both flavors.
- No backend may silently weaken multiplicity or cleanup guarantees; reject before linking when a capability is absent.
- Keep static discharge a separate checked pass; MLIR stays an optional backend and cannot change source semantics.
- Keep the existing implementation where it satisfies the contract; do not add a separate handler-object hierarchy when an ordinary closure is sufficient.
- Never weaken a failure into a skip to claim a milestone; ownership/IR checks stay where observable output cannot detect a defect.
