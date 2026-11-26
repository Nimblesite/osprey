# Plan 0062 — Compile-time metaprogramming

Contract: [spec 0056](../specs/language/0056-Metaprogramming.md) (language) · [expansion engine](../specs/compiler/ExpansionEngine.md) · [macro tooling](../specs/tooling/MacroTooling.md). Sources: [research basis](../research/metaprogramming/README.md).

**Status: not started.** The repository is in stabilization mode, which bans new features. No implementation step below begins until the user lifts stabilization for this plan.

## Goal

Top-tier compile-time metaprogramming, macros and templates in Osprey, delivered as several mechanisms above one kernel, so no single approach is the only one. Proved by programs that use each mechanism under GC, ARC, ownership and wasm32, by diagnostics tested in the language server and the CLI, and by measured editor latency.

Effect substitution chosen at compile time and implicit effects ([META-EFFECTS-PERF], [META-EFFECTS-LOCAL]) are delivered by [the algebraic-effects plan](0016-algebraic-effects-and-handlers.md#local-and-implicit-effects), not here.

## Design rules

The normative rules and their sources are [META-PRINCIPLES](../specs/language/0056-Metaprogramming.md#design-principles-from-the-literature-meta-principles). Every step meets one of them or names the one it leaves open. Each step is test-first, lands with its tests and expands existing corpus programs where it can.

## Decisions

Each is settled by a short, measured spike before the stage that depends on it, and recorded here with its evidence.

- [ ] **Engine arrangement** ([EXPAND-CHOICE](../specs/compiler/ExpansionEngine.md#one-engine-or-two-expand-choice)): shared protocol only, a shared core crate, or Osprey as a host of a generalised dmx. Spike: run one dmx Mustache macro against an Osprey record through the protocol, and list which dmx modules were reused unchanged. Decide by [EXPAND-CHOICE-RULE].
- [ ] **Evaluator** ([EXPAND-EVALUATOR](../specs/compiler/ExpansionEngine.md#expander-tiers-expand-tiers)): an interpreter over the typed canonical AST, or wasm32 with an embedded runtime. Spike: evaluate the same twenty corpus functions both ways; compare start-up, per-call time, added dependencies and exactness of the step limit. Build one.
- [ ] **Staging escape check** ([META-STAGE-EXTRUSION](../specs/language/0056-Metaprogramming.md#typed-staging-meta-stage)): how much is rejected statically and how much is detected during expansion. Spike: the escape examples of [Kiselyov 2014] written in Osprey.
- [ ] **Typed versus untyped analysis** ([META-STAGE-ANALYSE](../specs/language/0056-Metaprogramming.md#typed-staging-meta-stage)): whether typed matching on `Code<T>` ships in the first staging stage or after it.

## Surface syntax

- [ ] Fix the spelling of the four invocation shapes, the level-1 import, `static`, quote and splice in each flavor (Default, ML, Python, TypeScript), reusing `static` from the effect system. Each flavor's choice must parse with its fixed tree-sitter grammar and no expander ([META-KERNEL-PARSE](../specs/language/0056-Metaprogramming.md#kernel-meta-kernel)). Acceptance: grammar tests for every shape in every flavor, a token-tree test with unbalanced and layout-sensitive bodies, and `failscompilation/` programs for each malformed shape.
- [ ] One definition of "known at compile time" cited by both [META-STAGE-LEVEL] and `[STAGE]`, replacing any second wording.

## Stage A — Kernel and compile-time evaluation

- [ ] Factor the process runner, deadline, protocol framing, binary cache and dependency digests out of `osprey-project`'s discipline code into one module used by disciplines and the engine ([EXPAND-SHARED](../specs/compiler/ExpansionEngine.md#shared-infrastructure-expand-shared)). Acceptance: `make _test_disciplines` unchanged and green; deslop reports no duplicate.
- [ ] Syntax objects: scope sets and origin records on canonical AST nodes; resolution by largest subset with ambiguity an error ([META-KERNEL-HYGIENE], [META-KERNEL-ORIGIN]).
- [ ] The evaluator chosen above, with step and memory limits and compile-time handlers for `Diagnose` and `Fresh` ([META-EFFECTS-EXPANDER]).
- [ ] `static` expressions ([META-EVAL]). Acceptance: the differential test of [META-EVAL-AGREE] over every pure corpus function callable with literals; rejected twins for a run-time effect at level 1, a value that cannot cross, a step-limit overrun and an `Error` at the boundary.
- [ ] No-harm gate ([META-NO-HARM]): byte-identical output and diagnostics for the whole corpus with the kernel present, in CI.

## Stage B — Typed staging and templates

- [ ] `Code<T>`, quote, splice and the level rule, with effect rows on code ([META-STAGE]).
- [ ] Templates with `static` parameters and shared specialisations ([META-TEMPLATE]). Acceptance: an IR comparison against the hand-written equivalent for [META-TEMPLATE-GONE], and one existing duplicated family in `src/osprey/packages/` replaced by a template with identical output.
- [ ] `Reflect` for records, unions, effects and argument types, as recorded questions ([META-REFLECT], [META-ORDER-TYPED], [META-ORDER-CYCLE]).

## Stage C — Syntax and declaration macros

- [ ] Pattern macros with syntax classes ([META-SYNTAX-CLASS]); then procedural macros as level-1 functions.
- [ ] Delimited literals with a declared result type ([META-SYNTAX-LITERAL]).
- [ ] Declaration macros with roles and declared names ([META-DECL]); the outline computed without expansion ([META-ORDER-OUTLINE]).
- [ ] The hygiene suite of [META-VERIFICATION] at every shape, and the order-independence test of [META-ORDER].
- [ ] First real uses, replacing hand-written code and deleting it: JSON decoding into typed records for `@osprey/json`'s boundary modules, and equality and display for records.

## Stage D — Providers and the shared engine

- [ ] The protocol model in typeDiagram with generated code on both sides, tdbin framing and the JSON rendering ([EXPAND-PROTOCOL]).
- [ ] Worker tier with sandbox, text expanders and binder checking ([EXPAND-TEXT], [EXPAND-SANDBOX]).
- [ ] A typeDiagram provider: `.td` models become Osprey types in the compilation, replacing generated-and-committed model code where it exists ([META-PROVIDER]).
- [ ] The engine arrangement decided above, carried out; a dmx expander and an Osprey expander pass one conformance suite ([EXPAND-VERIFY]).
- [ ] [Math to code](../specs/tooling/MathToCode.md) moved onto the provider path with its proof contract and oracle unchanged.

## Stage E — Tooling

Tooling lands with the stage that needs it, not after; this list is the full set.

- [ ] One diagnostic renderer for CLI and editor, with blame by origin ([LSP-MACRO-DIAG](../specs/tooling/MacroTooling.md#diagnostics-lsp-macro-diag)). Lands with Stage A.
- [ ] The faulty-program corpus with exact diagnostics, checked through both ([LSP-MACRO-VERIFICATION]).
- [ ] Navigation, rename, completion and highlighting inside arguments ([LSP-MACRO-NAVIGATE]). Lands with Stage C.
- [ ] Expansion view, stepper, `osprey expand` and `osprey expand --json` ([LSP-MACRO-EXPAND]); add the commands to `make help`.
- [ ] Editor services for embedded languages ([LSP-MACRO-DSL]).
- [ ] Debug information, stack traces, coverage attribution and profiler grouping ([LSP-MACRO-DEBUG]).
- [ ] Documentation of generated members ([LSP-MACRO-DOCS]).
- [ ] Latency tests against [EXPAND-BUDGETS](../specs/compiler/ExpansionEngine.md#budgets-expand-budgets), with the measurement published before any figure is quoted.

## Packages and security

- [ ] Manifest and lock entries for packages that export expanders, and the project switch that refuses them ([META-SECURITY]).
- [ ] Reword the "build-time code execution" row of [PACKAGE-THREATS](../specs/ecosystem/0029-PackageManagement.md#threat-model-and-invariants-package-threats) to state the sandboxed compile-time exception, keeping invariant 4 as written.
- [ ] An attack suite first: expanders that try to read an undeclared file, reach the network, read the clock, exhaust memory, loop, and emit code that escapes hygiene. Each must be refused with its diagnostic.

## Proof obligations

Stated and checked with Osprey's proof kernel, with the executable definitions derived from the shipped source ([formula and proof contract](../specs/tooling/MathToCode.md)).

- [ ] Scope-set resolution returns the unique binding whose scopes are the largest subset, or reports ambiguity; it is deterministic and independent of the order bindings were added [Flatt 2016].
- [ ] Pattern-macro expansion preserves alpha-equivalence [Herman and Wand 2008; Adams 2015].
- [ ] The level checker is sound for the staging fragment: a program it accepts has no level-1 variable in generated code [Xie et al. 2023].
- [ ] Origin maps compose: the origin of a node after nested expansion is the composition of each step's map.
- [ ] The cache key is sufficient: two invocations with equal keys have equal expansions, given a deterministic expander.
- [ ] Hygiene for procedural macros in general has no published proof; this is recorded as tested, not proved.

## CI

- [ ] Phase 1 builds every program that uses a mechanism with no red; phase 2 runs the suites above once, with coverage; phase 3 gates coverage. Nothing here adds a phase or a required check beyond the root `CI` job.
- [ ] The determinism double-run and the no-harm gate are part of phase 2.

## References

Every address below was tested on 2026-11-27: a PDF address returns the same bytes as the downloaded copy, and a DOI resolves to the work's record, whose title Crossref confirms. The [research basis](../research/metaprogramming/README.md) says what each work establishes and what Osprey takes from it.

- [Adams 2015, *Towards the essence of hygiene*, POPL](https://michaeldadams.org/papers/hygiene/hygiene-2015-popl-authors-copy.pdf)
- [Flatt 2016, *Binding as sets of scopes*, POPL](https://www.cs.utah.edu/plt/publications/popl16-f.pdf)
- [Herman and Wand 2008, *A theory of hygienic macros*, ESOP](https://doi.org/10.1007/978-3-540-78739-6_4)
- [Kiselyov 2014, *The design and implementation of BER MetaOCaml*, FLOPS](https://okmij.org/ftp/meta-programming/ber-design.pdf)
- [Xie, White, Nicole and Yallop 2023, *MacoCaml: staging composable and compilable macros*, ICFP](https://www.cl.cam.ac.uk/~jdy22/papers/macocaml-staging-composable-and-compilable-macros.pdf)
