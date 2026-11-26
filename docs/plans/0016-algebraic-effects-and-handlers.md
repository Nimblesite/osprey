# Plan 0016 — Algebraic effects and handlers

**Contract:** [Algebraic Effects](../specs/language/0017-AlgebraicEffects.md), including implicit effects and host operations, and [compile-time effects](../specs/language/0056-Metaprogramming.md#compile-time-effects-meta-effects). Tooling tasks also touch [Language Server](../specs/tooling/0020-LanguageServerAndEditors.md), [Debugger](../specs/tooling/0021-Debugger.md), [WebAssembly target](../specs/compiler/0022-WebAssemblyTarget.md).
**Depends on:** [Plan 0026 — Structured Concurrency](0026-structured-concurrency.md) (region unwinding and cancellation-time finalization for `control abort`).

## Invariant — [EFFECTS-INV-MULTISHOT-REAL]

Multi-shot (`many`) continuations are built for real: resumed any number of times, with independent branches. No agent may refuse, defer, scope down, stub, simulate (re-dispatch or loops), feature-flag or downgrade them to `once`, even while the compiler support is unfinished. Tests always assert multi-shot behaviour. A test that is red because the support is missing stays red; it is never weakened, skipped or deleted ([COMPILER-RED-FAILS]). Violating this is an egregious defect. See also [EXAMPLE-CLEANARCH-INV-NO-SKIP] in [the Clean Architecture todo spec](../specs/applications/CleanArchTodo.md).

## Invariant — [EFFECTS-INV-MULTISHOT-PORTABLE]

Multi-shot continuations are a compiler feature, not a platform trick. The compiler lowers the captured remainder to an ordinary heap value (continuation-passing frames or closures) that any backend can call again, so the same program gives the same answers on every target: native, wasm32, Windows and mobile. The implementation contains **no platform-specific code**:

- no copying, saving or switching of machine stacks;
- no `ucontext`, `setjmp`/`longjmp`, assembly or register names;
- no `#if` on architecture or operating system;
- no dependence on threads to represent a continuation.

A design that needs any of these is rejected, however well it works on one machine. A target never gets a weaker `many`: it runs the portable representation ([MULTI-WASM]).

## Invariant — [EFFECTS-INV-MULTISHOT-GENERAL]

The compiler has one mechanism for multi-shot continuations, and it reads one fact: an operation is declared `many`. It never names, counts or special-cases an effect, an operation, a handler shape, an example or a use case. Retry, backtracking search, what-if authorization, validation fixes and pagination plans are handlers written in ordinary Osprey source; the compiler does not know them. A change that adds a named case for any of them to the compiler or runtime is rejected.

## Invariant — [EFFECTS-INV-MULTISHOT-WASM]

wasm32 fully supports multi-shot continuations ([MULTI-WASM-MANY], [WASM-TARGET-EFFECTS-MANY]). Every `many` program that runs natively compiles and runs on wasm32 and prints the same bytes. No agent may reject, skip, stub, feature-flag or downgrade `many` on wasm32, give it a separate golden, or move its tests out of the required `CI` job ([CI-MULTISHOT-WASM]). A change that makes a `many` program fail or differ on wasm32 is a defect in that change, and fixing the compiler is the only remedy.

## Prerequisites in place

Callable handler values, block-scoped `handle`, declared value/control modes, return transformations, static handler selection, `finally`/`mask`/`instance`, transport fixes, reactive signals, checker-backed LSP effect-requirement and implementation views, and the closed-program open-row prototype are implemented; see audit for evidence. The compile-time rejections for `many` replay safety and for `abort`/`once` misuse already exist and stay.

## Phase 1 — Scoped row polymorphism

- [ ] Put latent effect rows into `Type::Fun` and independently quantify, unify and discharge scoped row tails ([EFFECTS-ROW-POLY], [EFFECTS-GENERIC-ROWS], [STAGE-POLY-PREREQ]). Files: `src/toolchain/crates/osprey-types/src/ty.rs` (`Type::Fun` has no row field today), `src/toolchain/crates/osprey-types/src/effect_rows/{check,solver}.rs`. Cover upper bounds, duplicate labels, exact operation/instance discharge, occurs checks, shared-tail unification (`<E|r> = <F|r>` has no finite solution; freshening must not discard the shared-tail constraint), callbacks returning callbacks, record fields, shadowing, separate compilation. Reuse old inference cases as regression evidence only. Acceptance: new cases in `src/toolchain/crates/osprey-types/src/effect_rows_tests.rs` and `src/toolchain/crates/osprey-types/src/effect_rows_exports_tests.rs` pass; one `!e` helper instantiates at distinct call sites with distinct rows.

- [ ] Implement invocation-time specialization so an escaping callable or lazy iterator carrying a statically handled latent operation is discharged against its invocation handler, not its construction handler ([EFFECTS-STATIC-DISCHARGE], [STAGE-LOWER-ORDER]). Files: `src/toolchain/crates/osprey-types/src/effect_rows/validate_handlers.rs` (`validate_static_escape` fails closed today), `src/toolchain/crates/osprey-codegen/src/effects.rs`. Acceptance: an escaped callable/lazy iterator under a static handler compiles with no effect dispatch in the emitted IR; add cases to `src/toolchain/crates/osprey-cli/tests/staged_hygiene.rs`.

## Phase 2 — Continuation ownership and lifetime

- [ ] Implement owned continuation values: bare `resume` denotes the continuation; transferring it removes the former owner's right; `drop(k)`; a `once` call consumes it, a `many` call borrows a reusable snapshot ([EFFECTS-CONTINUATION-OWNERSHIP], [EFFECTS-RESUME-NESTING]). Files: `src/toolchain/crates/osprey-ast/src/resume.rs` (only counts syntactic uses today), `src/toolchain/crates/osprey-types/src/multiplicity.rs`, codegen/runtime continuation layout. Cover saved resumptions invoked after the arm returns, rejected borrowed escape, double consumption, deep nesting, cancellation, abandonment, HTTP callbacks, fiber ownership. Audit default/GC/ARC; output correctness alone cannot prove resource correctness (zero ARC leak). Acceptance: both-flavor positive and negative cases for each shape above, plus ARC zero-leak on transferred and dropped continuations.

- [ ] Give `control abort` a runnable case: an `abort` arm abandons the region, runs its finalizers inner to outer, releases owned operands, and its `perform` never returns ([MULTI-DECL-ABORT-RESULT], [MULTI-HANDLE-ABORT-MODE], [MULTI-COST-ABORT], [EFFECTS-FINALIZATION]). Files: remove `unimplemented_abort` in `src/toolchain/crates/osprey-types/src/multiplicity.rs`; add unwinding/release in `src/toolchain/compiler/runtime/effects_runtime.c` / `effects_coro.c`. Blocked on [Plan 0026](0026-structured-concurrency.md) unwinding. Acceptance: a runnable abort case in `src/toolchain/crates/osprey-cli/tests/staged_multiplicity.rs` replaces the current rejection assertions and proves finalizers and operand release.

- [ ] Audit abandoned native continuation frames: prove a dropped/cancelled continuation releases every owned operand and runs its finalizers exactly once; pthread cancellation/exit alone is not evidence ([MULTI-COST-ABORT], [EFFECTS-FINALIZATION]). Acceptance: finalizer order and zero ARC leak asserted for abandoned continuations under default/GC/ARC; shared with [Plan 0026](0026-structured-concurrency.md) kill-safety.

## Phase 3 — Reusable continuations

- [ ] Let a `many` continuation cross an iterator stage (`range(..) |> fold(..)`), and lower a captured effect with type parameters by instantiating its request layout per use. Both are rejected by name today (`what_a_many_continuation_cannot_cross_is_rejected_by_name`). Acceptance: those programs run and print every branch, and their rejection cases are deleted.

## Phase 4 — Targets, traces, editor

- [ ] Reproduce or close the review lead (2026-11-06, unconfirmed): static-handler monotonicity is not transitive, and invalid unused arms are erased before they are checked (`osprey-ast/src/stage.rs`, `lower_static.rs`). A confirmed case becomes a GitHub bug.
- [ ] Run the 63 resumable-effect corpus programs on the macOS host target. They are skipped there with "a suspended continuation cannot cross or outlive a synchronous host call" (`tests/effects/{multiplicity,resume,scope}`, `tests/modules/{ml_layout_depth,module_effects_rows,signature_ascription}`).
- [ ] Run `once` continuation programs on wasm32 when the stack-switching backend provides them, with answers identical to native; keep rejecting that missing capability before linking, naming the operation ([MULTI-WASM], [STAGE-WASM]). `many` is not part of this task: it already runs on wasm32 and must keep doing so ([EFFECTS-INV-MULTISHOT-WASM]). Files: `src/toolchain/crates/osprey-cli/src/target_capabilities.rs`, `src/toolchain/crates/osprey-cli/tests/target_capabilities.rs`, `src/toolchain/crates/osprey-cli/tests/staged_multiplicity.rs`. Acceptance: `ONCE_ON_WASM` compiles and runs; `MANY_ON_WASM` keeps compiling and running; capability rejections still name the operation.

- [ ] Ship the effect trace: a debug build and DAP view record the resolved perform site, actual runtime handler instance and resumption branch, innermost first; static-discharged sites are excluded ([MULTI-TRACE], [DEBUGGER-EFFECT-TRACE]). Files: `src/toolchain/crates/osprey-debug`, debug codegen, VS Code extension DAP view. Acceptance: a paused session shows the trace and selecting an entry navigates to the perform or arm.

- [ ] Ship the LSP replay-boundary view ([LSP-EFFECT-MULTIPLICITY]): on a `many` handler arm, `textDocument/implementation` returns the perform sites that arm can answer (its replay set); a handler region whose arms are all tail-resumptive over `once` operations publishes the "declaring Log static would remove it from the runtime" hint. Files: `src/toolchain/crates/osprey-lsp/src/effects/implementations.rs`, `src/toolchain/crates/osprey-lsp/src/effects/tests.rs`. Acceptance: tests pin both reports.

## Local and implicit effects

Write a program directly, performing each effect where the work happens, and run it against test doubles without editing it. Effects need no declaration when their use implies one, host operations can be substituted like any other effect, and a substitution can be aimed at one function. A build that substitutes nothing compiles to the same IR as today. This work remains gated by stabilization mode.

### Surface syntax

- [ ] Fix the spelling of a directed substitution (provisionally `handle static E within f { arms }`) in Default, ML and Python. It must parse with each flavor's fixed grammar. Acceptance: grammar tests in every flavor and `failscompilation/` programs for a missing target, a target that is not a function and a `within` on a non-static handler where the plan decides that is refused.
- [ ] Name the builtin effects and their operations ([EFFECTS-HOST-OPS]), one table in the effects spec, cited by the prelude.

### Implicit effects `[EFFECTS-IMPLICIT]`

- [ ] Resolve an undeclared effect name to an implied effect owned by the performing module ([EFFECTS-IMPLICIT-SCOPE]). Acceptance: the `Audit` example of the spec runs; rejected twins for a handler-only module, the same name implied in two modules and handled unqualified, and a module path that names no implied effect.
- [ ] Infer operation types by unification over performs and arms, generalised like top-level functions ([EFFECTS-IMPLICIT-OPS]). Acceptance: one corpus program implies an effect with three operations of different types used from two modules; rejected twins for an unconstrained operation type, an arm the interface lacks (with the suggestion in the expected diagnostic) and a conflicting perform type; a warning test for an operation no perform uses.
- [ ] Mode limits ([EFFECTS-IMPLICIT-MODE]). Acceptance: `handle static` on an implied effect discharges with zero residue; rejected twins for `resume` in an implied operation's arm and for a generic use.
- [ ] Equivalence gate ([EFFECTS-IMPLICIT-SAME]): CI inserts the implied declaration into every corpus program with an implicit effect and compares IR, output and diagnostics byte for byte.
- [ ] Editor ([LSP-EFFECTS-IMPLICIT]): hover, go to definition, **Declare effect** and the arm quick fix, each with a language-server test that applies the action and asserts unchanged diagnostics.

### Host operations as effects `[EFFECTS-HOST]`

- [ ] Model each host operation as an operation of its builtin effect with a default interpretation at the entry ([EFFECTS-HOST-DEFAULT]). Acceptance: the whole corpus compiles to identical IR before and after ([META-EFFECTS-PERF-HOST]).
- [ ] Substitute a host operation in a test. Acceptance: corpus tests that replace console output, the clock and a file read with static handlers and assert what was recorded, under every memory mode and wasm32.
- [ ] Target capability ([EFFECTS-HOST-CAPABILITY]). Acceptance: on wasm32, a file read covered by a substitute on every path builds and runs; an uncovered one is rejected at the perform.

### Directed substitution `[META-EFFECTS-LOCAL]`

- [ ] Region and function targets ([META-EFFECTS-LOCAL-TARGET]). Acceptance: the `report` example of the spec; a program where the same operation is substituted within one function and left real elsewhere, asserted from both paths; overlapping region and function substitutions resolved innermost first; rejected twins for an unknown and an ambiguous target.
- [ ] Selection ([META-EFFECTS-LOCAL-SELECT]): substitutions stated by a test or entry, and a target profile in `osprey.toml` naming a substitution module. Acceptance: the selection changes the cache key, and `osprey expand` prints the substitutions in force.
- [ ] Unchanged performing code ([META-EFFECTS-LOCAL-SUBSTITUTE]). Acceptance: the performing module's inferred rows and diagnostics are compared with and without the substitution and are equal.

### Performance gates `[META-EFFECTS-PERF]`

- [ ] [META-EFFECTS-PERF-ZERO]: for each corpus static substitution, the emitted IR equals, apart from names, the hand-written direct call of the arm's code.
- [ ] [META-EFFECTS-PERF-DYNAMIC]: a dynamic substitute's dispatch appears only in builds that select it, checked by IR inspection of a build without it.
- [ ] A benchmark in `src/toolchain/benchmarks/` comparing a local-effect program under a static substitute with the hand-written version; the result is recorded here with the machine named.

## Proof obligations

Each is stated in the proof kernel where the rule is within its reach, and must fail on a relevant mutation of the implementation:

- [ ] [EFFECTS-IMPLICIT-SAME]: inserting the implied declaration preserves the checked program.
- [ ] [META-EFFECTS-LOCAL-TARGET]: a function-targeted substitution equals installing the same handler around every call of that function in the region.
- [ ] [EFFECTS-HOST-DEFAULT]: the default interpretation of each builtin effect is the runtime call the builtin made before this plan.

## Constraints

- Each task needs executable positive and negative cases in both flavors. Local and implicit effect programs also run under ownership; existing corpus programs are expanded before new ones are added.
- [META-NO-HARM](../specs/language/0056-Metaprogramming.md#the-existing-language-is-untouched-meta-no-harm) holds at every step: corpus output, diagnostics and IR are byte-identical for programs that use no local or implicit effects.
- Corpus, `failscompilation/`, IR and equivalence checks run in the existing CI phases without a new required job.
- No backend may silently weaken multiplicity or cleanup guarantees; reject before linking when a capability is absent.
- Keep static discharge a separate checked pass; MLIR stays an optional backend and cannot change source semantics.
- Keep the existing implementation where it satisfies the contract; do not add a separate handler-object hierarchy when an ordinary closure is sufficient.
- Never weaken a failure into a skip to claim a milestone; ownership/IR checks stay where observable output cannot detect a defect.
