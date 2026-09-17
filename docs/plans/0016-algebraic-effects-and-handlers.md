# Plan 0016 — Managing effects

**Contract:** [Algebraic Effects](../specs/0017-AlgebraicEffects.md).
**Try the prototype:** [runnable comparison](../../examples/handlers/README.md).
**Status, 2026-09-17:** specification agreed with OspreyOpus1 on TMC and pushed as `053eae19`; breaking implementation underway. Legacy tests and examples are retained but do not constrain the new semantics. This plan
owns delivery of handlers, effect rows, staging and continuations. It replaces
plans 0008, 0024 and 0028 and the effects checklist formerly duplicated in 0013.
Specifications define the required language; this plan records implementation
status and evidence. A requirement being specified does not mean it works today.

## Outcome

An application declares the operations it needs; its caller chooses how to
provide them. Start with logging, configuration, storage and test substitutes.
The same business function must work under different handlers without edits,
and a missing implementation must be a useful compile error. Default users
should be able to begin with ordinary functions and calls: create `h`, then
call `h(work)`. Installing a handler for the rest of a block needs no `in`/`do`.

Advanced users retain explicit continuation control, safe repeated resumption,
scoped instances, result transformations and compile-time interpretation. These
are parts of the same effects model, not competing frameworks. No claim of
parity, completeness or superior performance is earned by syntax alone.

## Current evidence

The replacement grammar accepts callable `handler` values and block-scoped
`handle` statements only. Explicit `handle … in/do …` applications, inferred
arm modes, runtime ABI aliases and missing-signature fallbacks are deleted.
Old source examples remain historical evidence; they are not compatibility
requirements. The compiler rejects absent or unresolved operation metadata.

| Area | Observed state | Evidence |
| --- | --- | --- |
| Existing runtime | Value substitution, deep single-shot resume, generic operation identity, shared handler state and native fiber serialization exist. Native continuation storage uses pthreads. | `tests/effects/resume/`, `tests/effects/errors/`, `tests/regressions/effects/fiber_effects.test.osp` |
| Transport fixes | Operation arity is length-carrying; whole `Result` values and managed answers have regression coverage. | `resume_error_policies.test.{osp,ospml}`, `direct_recovery.test.{osp,ospml}`, `compiler/runtime/effects_runtime_tests.c`; issues #182, #183, #185 |
| Callable handlers | Both flavors run reusable handlers, captured factory values and rest-of-block installation. This is a working-tree prototype using ordinary closures. | `examples/handlers/handlers.*`, `crates/osprey-cli/tests/handler_values.rs` |
| Handler abstraction | The local higher-order fix passed 4 handler tests and 125 codegen tests before implementation paused. These are bounded checks, not a green branch-wide result. | `handler_values.rs`: generic/typed callbacks, independent Ada/Grace factories, both flavors and memory modes |
| Effects checker | Closed-program operation propagation and partial generic discharge exist. Independently quantified open rows do not. | `crates/osprey-types/src/effect_rows.rs`, `generic_effects_tests.rs` |
| Modes and continuations | Declared value/control modes replace arm-body classification. Arms run outside their activation; deep resume restores it. Owned escaping continuations, `many`, finalizers and masking remain unfinished. | `operation_modes.rs`: both flavors, default/GC/ARC; independent comparison below |
| Answer transformations | `return value => expression` transforms normal completion A to B outside its activation, preserving managed and callable values. Deep resume returns B; a control arm's answer bypasses the transform. Directly resolved static handler values and aliases specialize their computation before erasure. | `handler_returns.rs`: both flavors, default/GC/ARC; `returns.*` comparison |
| Staging | Explicit static selection accepts ordinary all-value effects. Source validation tracks builtin I/O through aliases/callbacks and rejects runtime dispatch hidden inside a locally handled helper. Capture/cell identity and dynamically used originals are preserved. | `static_selection.rs` and `staged_hygiene.rs` |
| Targets | Static discharge and dynamic value handlers have portable paths. Explicit dynamic resumption is unavailable in the current wasm backend and must be rejected before linking. Target limitations do not change language semantics. | [WebAssembly](../specs/0022-WebAssemblyTarget.md), target capability tests |

The existing curried-ML effect-loss report
[#184](https://github.com/Nimblesite/osprey/issues/184) remains an acceptance
item until its exact reproducer passes. Abandoning native continuation frames
also requires a resource audit: cancelling a pthread is not proof of source
finalization or release of every owned operand.

### Independent evidence

`python3 examples/handlers/run.py all --check` was executed with Osprey in both
flavors, Koka 3.2.3, OCaml 5.4.1, Eff 5.1 (commit
`503da71b9cb927af04fc62e28511e63cd7199151`) and Effekt 0.80.0. The common handler
examples produced `42`, `42`, then greetings for Ada, Grace and Ada.

The stronger `--demo semantics --check` comparison now passes in both Osprey
flavors and real Koka, OCaml, Eff and Effekt executables: all produce
`42/142/0/0`. Declared control operations fixed the former Osprey result
`42/142/1/0`. Returning without resuming abandons the computation; adding an
unreachable resume no longer changes its meaning. The independent golden was
preserved.

The `--demo returns --check` comparison also passes in every listed language:
`done=42`, `done=42!`, `stopped`. It distinguishes normal completion from code
after resumption and an answer that abandons the computation. Effekt places
the transform inside the handled computation; the other examples use explicit
normal-return clauses. This probe's transform is pure, so it does not establish
equivalent effect scope for those two encodings.

Two retained staging probes, written in the removed `in` syntax, falsified the
former implementation; current staging coverage uses the replacement grammar:

- `staging-scope.osp`: static interpretation captured the caller's shadowing
  binding (`static=2 dynamic=1`), instead of both producing 1.
- `staging-type.osp`: an `int` operation accepted a `string` arm and printed 10;
  it must fail before erasure, including when the arm is unused.

Historical staging measurements (114 versus 148 IR lines; a 27 KB wasm module)
are isolated observations, not performance promises. Previous claims that
staging eliminates the need for row inference, that dependency sets are exact
runtime reads, or that static-origin state is automatically replayable were
incorrect. The original higher-order staging gate passed; generic reactive
rebuild and nested parallel-matrix gates did not yet demonstrate the full
contract.

The new mode and hygiene suites passed 54 native executions across both
flavors and three memory backends, with zero live ARC objects. Scoped-runtime
checks passed 2,194 assertions. Control-without-resume target rejection also
passed. These focused checks establish the replacement behavior; they do not
claim full implementation of the remaining contract or a green legacy corpus.

## Delivery order and gates

Each step must have executable positive and negative cases in both flavors.
Keep the existing implementation where it satisfies the contract. Do not add a
separate handler-object hierarchy when an ordinary closure is sufficient.

1. **Freeze the contract and remove contradictions.** One effects spec, this
   plan, and a practical runnable guide. Resolve declaration modes, scoped rows,
   ownership, replay, finalization and target rules together. Agreement recorded with OspreyOpus1 on TMC on 2026-09-17 after correcting its five findings; local link and spec-ID checks complete.
2. **Make ordinary effect management trustworthy.** Implement value/control
   mode from the operation declaration, remove AST-search mode selection,
   replace contradictory semantics directly, and pass the `42/142/0/0`
   comparison. Pin no-`in`/`do` handler values, independent factories,
   higher-order transport, same-operation forwarding to an outer handler,
   result preservation and curried-ML calls. Add real/test logging and storage
   examples using the same unchanged business function.
3. **Validate before specializing.** Assemble imports, resolve bindings and
   generic instances, then validate every source arm and transitive requirement
   before static discharge. Preserve capture/cell identity, source locations,
   left-to-right once-only argument evaluation, unused-arm checking and
   original callable definitions. Recheck residual effects and target legality.
   Both staging probes and captured-state tests must pass; emitted code must
   contain no effect dispatch for discharged operations. Ordinary residual
   computation and closure allocation are permitted.
4. **Make effects part of reusable types.** Add generalized open scoped rows to
   function types and interfaces. Test upper bounds, duplicate labels, exact
   operation/instance discharge, occurs checks, shared-tail unification,
   callbacks returning callbacks and record fields, shadowing and separate
   compilation. Reuse old inference cases as regression evidence, not as the
   definition or sole oracle of the new solver. Diagnostics show the missing
   operation and why it escaped its handler.
5. **Complete scope and lifetime control.** Implement named instances, masking,
   answer transformations, owned continuation transfer and deterministic
   finalization. Test saved resumptions invoked after the arm returns, rejected
   borrowed escape and double consumption, deep nesting, cancellation,
   abandonment, HTTP callbacks and fiber ownership. Audit default, GC and ARC;
   output correctness alone cannot prove resource correctness.
6. **Provide reusable continuations.** Use a representation that can preserve
   the captured remainder independently for each branch; a consumed pthread
   cannot provide it. Prefer explicit continuation frames/CPS with a portable
   backend path; select the concrete runtime layout from the smallest working
   search and stored-resumption prototypes. Test pure search, independent
   branch state and no replay of work before capture. Reject replay of email,
   payment or an unproven captured resource, including operations sharing the
   handled effect's name. Retrying one operation must not replay later work.
7. **Prove target and tooling support.** Run accepted programs on native and
   wasm with the same answers; reject missing target capabilities before
   linking. Add logical continuation/handler-instance traces and LSP views of
   required operations, possible handlers and replay boundaries. No backend
   may silently weaken multiplicity or cleanup guarantees.

Steps 2–3 are the shortest useful prototype path. Full completion requires the
later steps; neither multi-shot nor wasm continuation parity is out of scope.
Safe cleanup integrates with [structured concurrency](0026-structured-concurrency.md).
This plan owns continuation lifetime; that plan owns scheduling and cancellation
integration.

## Comparison and distinguishing goals

The baseline comes from primary contracts and executed examples, not from
existing Osprey goldens. These are acceptance targets until their gates pass.

| Capability | Reference baseline | Osprey acceptance |
| --- | --- | --- |
| Replace an implementation without changing business code | Koka handler functions; OCaml `Effect.Deep`; Eff handlers; Effekt scoped handlers | Callable handlers in both flavors; real and test implementations |
| Control continuation explicitly | Koka `ctl`, OCaml continuations, Eff and Effekt continuation clauses | Declaration-based control, deep resume, no-resume abandonment |
| Reusable effectful abstractions | Koka open effect rows | Published open-row signatures, generic callbacks and scoped duplicates |
| Rich handler composition | Koka return clauses, masking and named handlers | Defined answer types, explicit scope and instance routing |
| Saved and repeated resumptions | Koka and Eff; OCaml continuations are one-shot | Owned affine and reusable continuations with capture/replay checks |
| Manage effects at compile time | Osprey design goal | The same typed-operation discipline drives checked static lowering and dynamic interpretation |
| Accessible experience | Osprey product goal | Default-first examples, ordinary calls, optional advanced notation and useful diagnostics |

References: [Koka book](https://koka-lang.github.io/koka/doc/book.html),
[OCaml Effect.Deep](https://ocaml.org/manual/5.3/api/Effect.Deep.html),
[Eff](https://www.eff-lang.org/handlers-tutorial.pdf),
[Effekt handlers](https://effekt-lang.org/docs/concepts/effect-handlers).
Osprey aims to combine this baseline with checked staging, replay safety and
both approachable and ML surfaces. Unmeasured speed, universal superiority and
unimplemented capabilities must not be presented as established facts.

## Downstream work and release

Reactive dependency reporting must use resolved operation identities and a
sound may-read set, report unresolved tails, and demonstrate selective rebuild.
Device effects (`Parallel`, `Tensor`, `Alloc`) depend on
[GPU delivery](0023-gpu-computation.md); a host fallback is not a device proof.
Keep static discharge a separate checked pass. MLIR remains a backend option
when measured device-lowering needs justify its dependency; adopting it cannot
change source semantics.

Release requires workspace tests, native goldens under all memory modes, wasm
capability/conformance checks, both-flavor syntax and editor tests, docs checks,
formatting, linting and repository-required CI. Keep ownership/IR checks where
observable output cannot detect a defect. Review changed goldens against the
new contract and independent evidence; never weaken a failure into a skip to
claim a completed milestone.
