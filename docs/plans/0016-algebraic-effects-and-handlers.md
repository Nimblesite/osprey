# Plan 0016 — Algebraic effects and handlers

Finish algebraic effects: scoped row polymorphism; owned, reusable continuations with a runnable `control abort`; structured concurrency over them (scopes, cancellation, finalizers, deadlines, turns, `reentrant`, `atomic`); and the debugger and editor views of them. Local and implicit effects follow once stabilization mode is lifted.

Contracts: [Algebraic Effects](../specs/language/0017-AlgebraicEffects.md) (including implicit effects and host operations), [compile-time effects](../specs/language/0056-Metaprogramming.md#compile-time-effects-meta-effects), [Structured Concurrency](../specs/language/0036-StructuredConcurrency.md), [Fibers](../specs/language/0011-LightweightFibersAndConcurrency.md), [WebAssembly target](../specs/compiler/0022-WebAssemblyTarget.md), [Language Server](../specs/tooling/0020-LanguageServerAndEditors.md) and [Debugger](../specs/tooling/0021-Debugger.md).

## Invariants

- [EFFECTS-INV-MULTISHOT-REAL]: multi-shot (`many`) continuations are built for real. No agent may refuse, defer, scope down, stub, simulate (re-dispatch or loops), feature-flag or downgrade them to `once`, even while compiler support is unfinished; tests assert multi-shot behaviour, and a test that is red because support is missing stays red ([COMPILER-RED-FAILS], [EXAMPLE-CLEANARCH-INV-NO-SKIP]).
- [EFFECTS-INV-MULTISHOT-PORTABLE]: `many` is a compiler feature, not a platform trick. The captured remainder is an ordinary heap value (continuation-passing frames or closures) that any backend can call again; the implementation contains no copying, saving or switching of machine stacks, no `ucontext`, `setjmp`/`longjmp`, assembly or register names, no `#if` on architecture or operating system, and no thread-backed continuation. A design that needs any of these is rejected, however well it works on one machine, and a target never gets a weaker `many`.
- [EFFECTS-INV-MULTISHOT-GENERAL]: one mechanism reads one fact, an operation declared `many`. The compiler and runtime never name, count or special-case an effect, operation, handler shape, example or use case; retry, backtracking search, what-if authorization, validation fixes and pagination are handlers written in ordinary Osprey source.
- [EFFECTS-INV-MULTISHOT-WASM]: every `many` program that runs natively compiles and runs on wasm32 and prints the same bytes. No separate golden, no skipped or moved test ([MULTI-WASM-MANY], [WASM-TARGET-EFFECTS-MANY], [CI-MULTISHOT-WASM]); a change that makes a `many` program fail or differ on wasm32 is a defect in that change.

## Scoped row polymorphism

- [ ] Put latent effect rows into `Type::Fun` (`src/toolchain/crates/osprey-types/src/ty.rs` has no row field today) and quantify, unify and discharge scoped row tails independently (`osprey-types/src/effect_rows/{check,solver}.rs`): upper bounds, duplicate labels, exact operation and instance discharge, occurs checks, shared-tail unification (`<E|r> = <F|r>` has no finite solution; freshening must not discard the shared-tail constraint), callbacks returning callbacks, record fields, shadowing and separate compilation [EFFECTS-ROW-POLY], [EFFECTS-GENERIC-ROWS], [STAGE-POLY-PREREQ]. Check: new cases in `osprey-types/src/effect_rows_tests.rs` and `effect_rows_exports_tests.rs` pass, and one `!e` helper instantiates at distinct call sites with distinct rows; old inference cases are regression evidence only.
- [ ] Invocation-time specialization: an escaping callable or lazy iterator carrying a statically handled latent operation is discharged against its invocation handler, not its construction handler (`osprey-types/src/effect_rows/validate_handlers.rs`, where `validate_static_escape` fails closed today; `osprey-codegen/src/effects.rs`) [EFFECTS-STATIC-DISCHARGE], [STAGE-LOWER-ORDER]. Check: an escaped callable or lazy iterator under a static handler compiles with no effect dispatch in the emitted IR; cases in `osprey-cli/tests/staged_hygiene.rs`.

## Continuation ownership and lifetime

- [ ] Owned continuation values: bare `resume` denotes the continuation; transferring it removes the former owner's right; `drop(k)`; a `once` call consumes it and a `many` call borrows a reusable snapshot (`osprey-ast/src/resume.rs` counts syntactic uses today; `osprey-types/src/multiplicity.rs`; the codegen and runtime continuation layout) [EFFECTS-CONTINUATION-OWNERSHIP], [EFFECTS-RESUME-NESTING]. Check: both-flavor positive and negative cases for saved resumptions invoked after the arm returns, borrowed escape, double consumption, deep nesting, cancellation, abandonment, HTTP callbacks and fiber ownership, plus ARC zero leak on transferred and dropped continuations.
- [ ] A runnable `control abort`: the arm abandons the region, runs its finalizers inner to outer, releases the owned operands and its `perform` never returns. Remove `unimplemented_abort` in `osprey-types/src/multiplicity.rs` and add the unwinding and release to `src/toolchain/compiler/runtime/effects_runtime.c` / `effects_coro.c`, after the cancellation finalizers below [MULTI-DECL-ABORT-RESULT], [MULTI-HANDLE-ABORT-MODE], [MULTI-COST-ABORT], [EFFECTS-FINALIZATION]. Check: a runnable abort case in `osprey-cli/tests/staged_multiplicity.rs` replaces the current rejection assertions and proves finalizer order and operand release.
- [ ] Abandoned native frames: a dropped, cancelled or aborted continuation releases every owned operand and runs its finalizers exactly once under default, GC and ARC; pthread cancellation or exit alone is not evidence [MULTI-COST-ABORT], [EFFECTS-FINALIZATION], [CANCEL-KILLSAFE]. Check: finalizer order and the `[osp-arc] exit: 0 live objects` sentinel (`src/toolchain/crates/run_test_corpus.sh arc`) for abandoned continuations.

## Reusable continuations

- [ ] Let a `many` continuation cross an iterator stage (`range(..) |> fold(..)`) and lower a captured effect with type parameters by instantiating its request layout per use; both are rejected by name today (`what_a_many_continuation_cannot_cross_is_rejected_by_name` in `osprey-cli/tests/staged_many.rs`). Check: those programs run and print every branch, and their rejection cases are deleted.

## Structured concurrency

This work shares the unwinding above: `abandon_body()` in `src/toolchain/compiler/runtime/effects_coro.c` already runs handler finalizers inner to outer, and the fiber runtime's `select` primitive has a source-order deterministic tie-break. Constraints: native-only until a wasm story exists for fibers (`many` is not part of that runtime and already runs on wasm32); deterministic mode ([CONCURRENCY-DETERMINISTIC]) is sequential, so `cancel` before the victim's driving `await` still dequeues it and turn order is spawn order; staged effects keep captured-state ownership and alias evidence through discharge, and residual memory access still requires the same synchronization and exclusivity proof; cycles are rejected statically, with no Orleans-style runtime deadlock timeout.

- [ ] `scope` keyword and `scope_expression` in `src/toolchain/tree-sitter-osprey/grammar.js` and `src/toolchain/tree-sitter-osprey-ml/grammar.js`, lowered to one `Expr::Scope` (`osprey-syntax/src/default/expr.rs`, `osprey-syntax/src/ml/`, `osprey-ast/src/lib.rs`) [CANCEL-SCOPE]. Check: `cargo test -p osprey-syntax` parses `scope { ... }` to `Expr::Scope` in both flavors.
- [ ] Scope registry in `src/toolchain/compiler/runtime/fiber_runtime.c`: `spawn` attaches to the innermost scope, normal exit waits for children, scope exit reclaims its fiber and channel handle memory, and an implicit root scope wraps program entry [CANCEL-SCOPE]. Check: `run_test_corpus.sh gc` and `arc` reproduce every existing golden byte for byte.
- [ ] `cancel` and `join` keywords, `Expr::Cancel`/`Expr::Join`, and `Outcome<T>` with `join` and `cancel` builtins in `osprey-types/src/builtins.rs`; cancellation is observed only at suspension points (`await`, `send`, `recv`, `sleep`, `yield`, control operations), declines to resume (drops the continuation and runs finalizers) and completes the fiber as cancelled (`fiber_runtime.c`, `effects_coro.c`) [CANCEL-REQUEST], [CANCEL-POINTS], [CANCEL-DELIVERY], [CANCEL-JOIN]. Check: a both-flavor suite in `src/osprey/tests/regressions/fiber/`.
- [ ] Await propagation: `await` of a cancelled fiber unwinds the awaiter as cancelled; at program entry it runs entry finalizers and exits nonzero with a one-line diagnostic naming the spawn site [CANCEL-JOIN]. Check: a must-reject fixture that no pattern can match cancellation, and a golden pinning the exit status and diagnostic.
- [ ] `cancel` runs the existing handler finalizers innermost first through `abandon_body()`, with one-shot shielding so a further cancel cannot interrupt cleanup, including at its suspension points [CANCEL-FINALLY]. Check: cancelling a fiber suspended two handler regions deep records finalizer order and shielding.
- [ ] Channel wait-queue removal in `fiber_runtime.c`: cancelling a blocked sender or receiver removes it without consuming, duplicating or losing an element [CANCEL-KILLSAFE]. Check: a golden proves the element count is conserved under both corpus backends.
- [ ] Turn completion under performer cancellation: a turn whose arm is already running completes and its answer is discarded; a performer still queued is dequeued without the arm running [CANCEL-KILLSAFE]. Check: a both-flavor regression in `src/osprey/tests/regressions/fiber/`.
- [ ] `within(ms) { ... } -> Result<T, TimedOut>` and `race(a, b) -> T`: grammar and `Expr` variants implemented over the existing `select` primitive with its source-order tie-break [CANCEL-DEADLINE]. Check: a golden shared with `src/osprey/tests/regressions/fiber/channel_select.test.osp`.
- [ ] Turns: name the shipped `in_flight` serialization as turns and replace it with a flat-combining turn queue with a single-CAS fast path, after a `src/toolchain/benchmarks/cases/<name>/` microbenchmark compares it with the current round-trip serialization; deterministic mode keeps total turn order equal to the spawn order of performers [SERIAL-TURN], [SERIAL-CONTENTION].
- [ ] Turn graph over effect-row summaries in `osprey-types/src/effect_rows/`: reject waiting cycles between distinct owners with a named-cycle diagnostic [SERIAL-REENTRANCY]. Check: must-reject fixtures for a direct cycle and a cycle through a helper lambda.
- [ ] `reentrant` surface: a `reentrant` operation returns `Unit`, and performing it inside its own active turn enqueues a new turn [SERIAL-REENTRANCY]. Check: a must-reject fixture for a non-`Unit` `reentrant`.
- [ ] `atomic { ... }` and `retry`: grammar and `Expr` variants; multi-region turn acquisition in canonical order (optimistic logging later); row-checked isolation that rejects `spawn`, `await`, `send`, `recv`, `sleep` and outward operations inside the block [SERIAL-ATOMIC]. Check: must-reject fixtures for a suspending operation inside `atomic` and `retry` outside it, and a bounded-buffer blocking-condition test via `retry` in both flavors.
- [ ] Spec bookkeeping: move `0036-StructuredConcurrency.md` sections from "normative target" to shipped as each item lands, replace [CONCURRENCY-CANCEL-DESIGN] in `0011-LightweightFibersAndConcurrency.md` with the real semantics and keep [EFFECTS-FIBER-PERFORM] truthful. Check: `make ci` green.

## Targets, traces and editor

- [ ] Reproduce or close the review lead of 2026-11-06 (unconfirmed): static-handler monotonicity is not transitive, and invalid unused arms are erased before they are checked (`osprey-ast/src/stage.rs`, `lower_static.rs`). Check: a confirmed case becomes a GitHub bug with a failing test.
- [ ] Run the 63 resumable-effect corpus programs the macOS host target skips with "a suspended continuation cannot cross or outlive a synchronous host call" (`src/osprey/tests/effects/{multiplicity,resume,scope}`, `src/osprey/tests/modules/{ml_layout_depth,module_effects_rows,signature_ascription}`; `osprey-cli/src/target_capabilities.rs`). Check: they run on the macOS host and match their goldens.
- [ ] `once` continuations on wasm32 when the stack-switching backend provides them, with answers identical to native; until then the rejection names the operation (`osprey-cli/src/target_capabilities.rs`, `osprey-cli/tests/target_capabilities.rs`, `osprey-cli/tests/staged_multiplicity.rs` `ONCE_ON_WASM`) [MULTI-WASM], [STAGE-WASM]. Check: `ONCE_ON_WASM` compiles and runs, a `MANY_ON_WASM` twin keeps compiling and running ([EFFECTS-INV-MULTISHOT-WASM]), and capability rejections still name the operation.
- [ ] Effect trace: a debug build and the DAP view record the resolved perform site, the actual runtime handler instance and the resumption branch, innermost first, excluding static-discharged sites (`osprey-debug`, debug codegen, the VS Code extension's DAP view) [MULTI-TRACE], [DEBUGGER-EFFECT-TRACE]. Check: a paused session shows the trace and selecting an entry navigates to the perform or arm.
- [ ] LSP replay-boundary view (`osprey-lsp/src/effects/implementations.rs`, `osprey-lsp/src/effects/tests.rs`) [LSP-EFFECT-MULTIPLICITY]. Check: on a `many` handler arm `textDocument/implementation` returns the perform sites that arm can answer, and a handler region whose arms are all tail-resumptive over `once` operations publishes the "declaring Log static would remove it from the runtime" hint; tests pin both reports.

## Local and implicit effects

Gated by stabilization mode: no step starts until the user lifts it. The contract: effects need no declaration when their use implies one, host operations can be substituted like any other effect, and a substitution can be aimed at one function; a build that substitutes nothing compiles to the same IR as today.

### Surface syntax

- [ ] Fix the spelling of a directed substitution (provisionally `handle static E within f { arms }`) in Default, ML and Python; each flavor parses it with its fixed grammar. Check: grammar tests in every flavor and `failscompilation/` programs for a missing target, a target that is not a function and `within` on a non-static handler where the plan decides that is refused.
- [ ] Name the builtin effects and their operations in one table of the effects spec, cited by the prelude [EFFECTS-HOST-OPS].

### Implicit effects [EFFECTS-IMPLICIT]

- [ ] Resolve an undeclared effect name to an implied effect owned by the performing module [EFFECTS-IMPLICIT-SCOPE]. Check: the spec's `Audit` example runs; rejected twins for a handler-only module, the same name implied in two modules and handled unqualified, and a module path that names no implied effect.
- [ ] Infer operation types by unification over performs and arms, generalised like top-level functions [EFFECTS-IMPLICIT-OPS]. Check: one corpus program implies an effect with three operations of different types used from two modules; rejected twins for an unconstrained operation type, an arm the interface lacks (with the suggestion in the expected diagnostic) and a conflicting perform type; a warning test for an operation no perform uses.
- [ ] Mode limits [EFFECTS-IMPLICIT-MODE]. Check: `handle static` on an implied effect discharges with zero residue; rejected twins for `resume` in an implied operation's arm and for a generic use.
- [ ] Equivalence gate [EFFECTS-IMPLICIT-SAME]. Check: CI inserts the implied declaration into every corpus program with an implicit effect and compares IR, output and diagnostics byte for byte.
- [ ] Editor [LSP-EFFECTS-IMPLICIT]. Check: language-server tests for hover, go to definition, **Declare effect** and the arm quick fix, each applying the action and asserting unchanged diagnostics.

### Host operations as effects [EFFECTS-HOST]

- [ ] Model each host operation as an operation of its builtin effect with a default interpretation at the entry [EFFECTS-HOST-DEFAULT]. Check: the whole corpus compiles to identical IR before and after ([META-EFFECTS-PERF-HOST]).
- [ ] Substitute a host operation in a test. Check: corpus tests replace console output, the clock and a file read with static handlers and assert what was recorded, under every memory mode and wasm32.
- [ ] Target capability [EFFECTS-HOST-CAPABILITY]. Check: on wasm32 a file read covered by a substitute on every path builds and runs, and an uncovered one is rejected at the perform.

### Directed substitution [META-EFFECTS-LOCAL]

- [ ] Region and function targets [META-EFFECTS-LOCAL-TARGET]. Check: the spec's `report` example; a program where the same operation is substituted within one function and left real elsewhere, asserted from both paths; overlapping region and function substitutions resolved innermost first; rejected twins for an unknown and an ambiguous target.
- [ ] Selection [META-EFFECTS-LOCAL-SELECT]: substitutions stated by a test or entry, and a target profile in `osprey.toml` naming a substitution module. Check: the selection changes the cache key and `osprey expand` prints the substitutions in force.
- [ ] Unchanged performing code [META-EFFECTS-LOCAL-SUBSTITUTE]. Check: the performing module's inferred rows and diagnostics are equal with and without the substitution.

### Performance gates [META-EFFECTS-PERF]

- [ ] [META-EFFECTS-PERF-ZERO]: for each corpus static substitution the emitted IR equals, apart from names, the hand-written direct call of the arm's code.
- [ ] [META-EFFECTS-PERF-DYNAMIC]: a dynamic substitute's dispatch appears only in builds that select it, checked by IR inspection of a build without it.
- [ ] A benchmark in `src/toolchain/benchmarks/` comparing a local-effect program under a static substitute with the hand-written version; the result is recorded here with the machine named.

## Proof obligations

Each is stated in the proof kernel where the rule is within its reach and fails on a relevant mutation of the implementation ([formula and proof contract](../specs/tooling/MathToCode.md)).

- [ ] [EFFECTS-IMPLICIT-SAME]: inserting the implied declaration preserves the checked program.
- [ ] [META-EFFECTS-LOCAL-TARGET]: a function-targeted substitution equals installing the same handler around every call of that function in the region.
- [ ] [EFFECTS-HOST-DEFAULT]: the default interpretation of each builtin effect is the runtime call the builtin made before this plan.

## Constraints

- Every task lands with executable positive and negative cases in both flavors; local and implicit effect programs also run under ownership, and existing corpus programs are expanded before new ones are added.
- [META-NO-HARM](../specs/language/0056-Metaprogramming.md#the-existing-language-is-untouched-meta-no-harm) holds at every step: corpus output, diagnostics and IR are byte-identical for programs that use no local or implicit effects.
- Corpus, `failscompilation/`, IR and equivalence checks run in the existing CI phases without a new required job.
- No backend silently weakens multiplicity or cleanup guarantees; a missing capability is rejected before linking. Static discharge stays a separate checked pass, and MLIR stays an optional backend that cannot change source semantics.
- Keep the existing implementation where it satisfies the contract: no separate handler-object hierarchy when an ordinary closure is sufficient, and never a skip in place of a failure to claim a milestone.
