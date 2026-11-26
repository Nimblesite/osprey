# Plan 0064 — Local and implicit effects

Contract: [META-EFFECTS](../specs/language/0056-Metaprogramming.md#compile-time-effects-meta-effects) (performance and locality) · [EFFECTS-IMPLICIT](../specs/language/0017-AlgebraicEffects.md#implicit-effects-effects-implicit) · [EFFECTS-HOST](../specs/language/0017-AlgebraicEffects.md#host-operations-effects-host) · [LSP-EFFECTS-IMPLICIT](../specs/tooling/0020-LanguageServerAndEditors.md#implied-effect-interfaces-lsp-effects-implicit).

**Status: not started.** The repository is in stabilization mode, which bans new features. Only the specifications are done. No step below begins until the user lifts stabilization for this plan.

## Goal

Write a program directly, performing each effect where the work happens, and run it against test doubles without editing it. Effects need no declaration when their use implies one, host operations can be substituted like any other effect, and a substitution can be aimed at one function. A build that substitutes nothing compiles to the same IR as today.

## Design rules

- Test first. Each step lands with programs in `src/osprey/tests/` that pass under GC, ARC, ownership and wasm32, and rejected twins in `src/osprey/examples/failscompilation/`. Existing corpus programs are expanded before new ones are added.
- [META-NO-HARM](../specs/language/0056-Metaprogramming.md#the-existing-language-is-untouched-meta-no-harm) holds at every step: the corpus output, diagnostics and IR are byte-identical for programs that use none of this.
- Performance is gated, not claimed: [META-EFFECTS-PERF](../specs/language/0056-Metaprogramming.md#performance-meta-effects-perf) is an IR comparison in CI.

## Done

- [x] Specifications: [META-EFFECTS] rewritten around performance and locality, [EFFECTS-IMPLICIT] and [EFFECTS-HOST] in the effects contract, [LSP-EFFECTS-IMPLICIT] in the editor contract.

## Surface syntax

- [ ] Fix the spelling of a directed substitution (provisionally `handle static E within f { arms }`) in Default, ML and Python. It must parse with each flavor's fixed grammar. Acceptance: grammar tests in every flavor and `failscompilation/` programs for a missing target, a target that is not a function and a `within` on a non-static handler where the plan decides that is refused.
- [ ] Name the builtin effects and their operations ([EFFECTS-HOST-OPS]), one table in the effects spec, cited by the prelude.

## Stage A — Implicit effects `[EFFECTS-IMPLICIT]`

- [ ] Resolve an undeclared effect name to an implied effect owned by the performing module ([EFFECTS-IMPLICIT-SCOPE]). Acceptance: the `Audit` example of the spec runs; rejected twins for a handler-only module, the same name implied in two modules and handled unqualified, and a module path that names no implied effect.
- [ ] Infer operation types by unification over performs and arms, generalised like top-level functions ([EFFECTS-IMPLICIT-OPS]). Acceptance: one corpus program implies an effect with three operations of different types used from two modules; rejected twins for an unconstrained operation type, an arm the interface lacks (with the suggestion in the expected diagnostic) and a conflicting perform type; a warning test for an operation no perform uses.
- [ ] Mode limits ([EFFECTS-IMPLICIT-MODE]). Acceptance: `handle static` on an implied effect discharges with zero residue; rejected twins for `resume` in an implied operation's arm and for a generic use.
- [ ] Equivalence gate ([EFFECTS-IMPLICIT-SAME]): CI inserts the implied declaration into every corpus program with an implicit effect and compares IR, output and diagnostics byte for byte.
- [ ] Editor ([LSP-EFFECTS-IMPLICIT]): hover, go to definition, **Declare effect** and the arm quick fix, each with a language-server test that applies the action and asserts unchanged diagnostics.

## Stage B — Host operations as effects `[EFFECTS-HOST]`

- [ ] Model each host operation as an operation of its builtin effect with a default interpretation at the entry ([EFFECTS-HOST-DEFAULT]). Acceptance: the whole corpus compiles to identical IR before and after ([META-EFFECTS-PERF-HOST]).
- [ ] Substitute a host operation in a test. Acceptance: corpus tests that replace console output, the clock and a file read with static handlers and assert what was recorded, under every memory mode and wasm32.
- [ ] Target capability ([EFFECTS-HOST-CAPABILITY]). Acceptance: on wasm32, a file read covered by a substitute on every path builds and runs; an uncovered one is rejected at the perform.

## Stage C — Directed substitution `[META-EFFECTS-LOCAL]`

- [ ] Region and function targets ([META-EFFECTS-LOCAL-TARGET]). Acceptance: the `report` example of the spec; a program where the same operation is substituted within one function and left real elsewhere, asserted from both paths; overlapping region and function substitutions resolved innermost first; rejected twins for an unknown and an ambiguous target.
- [ ] Selection ([META-EFFECTS-LOCAL-SELECT]): substitutions stated by a test or entry, and a target profile in `osprey.toml` naming a substitution module. Acceptance: the selection changes the cache key, and `osprey expand` prints the substitutions in force.
- [ ] Unchanged performing code ([META-EFFECTS-LOCAL-SUBSTITUTE]). Acceptance: the performing module's inferred rows and diagnostics are compared with and without the substitution and are equal.

## Stage D — Performance gates `[META-EFFECTS-PERF]`

- [ ] [META-EFFECTS-PERF-ZERO]: for each corpus static substitution, the emitted IR equals, apart from names, the hand-written direct call of the arm's code.
- [ ] [META-EFFECTS-PERF-DYNAMIC]: a dynamic substitute's dispatch appears only in builds that select it, checked by IR inspection of a build without it.
- [ ] A benchmark in `src/toolchain/benchmarks/` comparing a local-effect program under a static substitute with the hand-written version; the result is recorded here with the machine named.

## Proof obligations

Each is stated in the proof kernel where the rule is within its reach, and must fail on a relevant mutation of the implementation:

- [ ] [EFFECTS-IMPLICIT-SAME]: inserting the implied declaration preserves the checked program.
- [ ] [META-EFFECTS-LOCAL-TARGET]: a function-targeted substitution equals installing the same handler around every call of that function in the region.
- [ ] [EFFECTS-HOST-DEFAULT]: the default interpretation of each builtin effect is the runtime call the builtin made before this plan.

## CI

Every acceptance above runs in the existing phases: corpus and `failscompilation/` programs in the test phase, IR and equivalence gates in the test phase with their results as artifacts, and no new required job.
