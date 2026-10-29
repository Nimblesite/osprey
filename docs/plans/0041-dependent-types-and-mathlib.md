# Plan 0041 — Dependent types as a pluggable discipline, and Mathlib on Osprey

Giant leaps - not baby steps
Violent refactoring

Contracts: [spec 0050](../specs/language/0050-TypeDisciplines.md) for the pluggable discipline seam and [spec 0049](../specs/language/0049-DependentTypesAndProofs.md) for the `lean4` discipline and the Mathlib port. Status: **M0, M1 (a `Prop`/`→` theorem from a recorded Lean export) and M2 implemented and gated by `make _test_proof_kernel` and the speed gate; M3 not started; phase 1 implemented except the two open boxes below.** The road test (M0 to M3) is the first work; it needed one resolver change (positional constructors across files) and one codegen change (recursive `Result` returns). The adversarial reviews [1](../reviews/0041-dependent-types-and-mathlib-review.md), [2](../reviews/0041-dependent-types-and-mathlib-review-2.md) and [3](../reviews/0041-dependent-types-and-mathlib-review-3.md) are each answered item by item at their foot. Stabilization mode holds for the phases after it: none begins without the owner's go.

## Summary

Osprey gets a proof assistant whose checked library is Mathlib, built beside the language rather than into it. The Hindley-Milner type system, effect rows, monomorphisation and codegen of today's compiler are not touched; the compiler gains one dispatch seam ([TYPES-DISCIPLINE-SEAM], [spec 0050](../specs/language/0050-TypeDisciplines.md)) that hands a module marked `// osprey: types=<name>` to the package registered under that name. The first registered discipline is `lean4`: a kernel for Lean 4's type theory, an elaborator, tactics and the `@osprey/mathlib` package, all written in Osprey and compiled through the native backend. Other provers and other type systems register the same way ([TYPES-DISCIPLINE-REGISTRY]); the seam is a feature on its own, delivered and proved by two reference disciplines before any kernel is written, and nothing in this plan is the only way to put a solver on Osprey.

```mermaid
flowchart LR
    S[module source] -->|types=hm| HM[osprey-types HM checker]
    S -->|types=lean4| D[lean4 discipline package]
    S -->|types=other| X[another registered discipline]
    D --> K[kernel: pure Result function]
    D -->|erasure| A[canonical AST]
    HM --> A
    X --> A
    A --> C[codegen, unchanged]
    L[lean4export of Mathlib] --> K
```

## Checklist

Ticked as the work lands on `feat/type-disciplines`; a box is ticked only when its test or output exists in the repo.

### M0 — a proof checks
- [x] `examples/proof/kernel/`: levels, terms (de Bruijn), environment, β/δ/ζ/η reduction, inference and checking as ML modules
- [x] `osprey examples/proof/kernel --run` prints `checked: id_imp` and rejects `id_imp_bad` and `not_a_type` with the kernel's reason
- [x] Compiler: a positional union constructor applied from another file folds to its construction (`constructor_resolve.rs`, `resolve.rs`)
- [x] Resolver test: imported positional variant accepted, imported record still needs named fields
- [x] `examples/proof/kernel/expectedoutput` and `make _test_proof_kernel` (in `make test`) running the kernel under GC, ARC and wasm32
- [x] `benchmarks/proof/run.sh` and `m0_identity`: a process-against-process regression gate (a compiled Osprey kernel with the theorem built in against `lean --run`, elaboration included), red when Osprey is slower; not kernel-speed evidence, which is `m1_export`
- [x] Lean (elan) installed in CI (`test-proof` job, toolchain pinned) and the proof benchmark run on every kernel change
- [x] `docs/reports/proof-road-test-2026-10-29.md`: lines, hours, language gaps
- [x] Compiler: a recursive function returning `Result<T, _>` no longer needs a written signature (`abi_shape_concrete` at the return slot); an open success payload still does
- [x] Review P0.1: `Terms::equal` is structural (indices, every universe argument), never printed; repeated-binder and universe-argument rejections in `tests/terms.ospml` and `tests/infer.ospml`
- [x] Review P0.2: universe parameters instantiated at every constant use and unfold; arity and scope checked; `Levels::equal` structural on normal forms
- [x] Review P1.5/6: `examples/proof/kernel/tests` (81 cases across seven suites) in `make _test_proof_kernel` under GC, ARC and wasm32, `failed=0` required of each; golden regenerated with the M2 and export lines
- [x] Review 3 P0.1: the entry compiles again; two codegen record-tracking fixes rather than a rename (a `...rest` tail keeps its list type, and a call through generic inlining keeps the checker's concrete return type), each with a twin regression in `tests/core/collections/shared_field_list_recursion` that fails without it

### M1 — a recorded Lean export checks (a `Prop`/`→` theorem; inductives wait on M3)
- [x] `lean4export` reader for the NDJSON format 3.1.0 the pinned exporter writes: names, levels, the M0 expression forms, `thm`/`def`/`axiom` (`src/export.ospml`; eleven cases in `tests/export.ospml`); inductives, quotients and the expression forms M3 adds are refused by kind, never skipped
- [x] Review 3 P1.5: every table index is defined once: a re-declared, reserved (name 0, level 0) or negative index is refused with its line, never rebound (six cases in `tests/export_mutations.ospml`, each red without the guard)
- [x] Mutated exports (`tests/export_mutations.ospml`, 29 cases): every single-edit corruption of the recorded export is rejected, asserted on the rejection's kind, the statement as read and the proof's inferred type compared by `Terms::equal` with hand-built terms (`tests/shapes.ospml`), never on a rendering of Lean's hygienic names
- [x] Every declaration of an export goes through `Infer::sort`/`Infer::check` before `Environment::add` (`Checked::exported` in `src/checked.ospml`, shared by the example and the benchmark entry)
- [x] `id_imp` from a Lean export checks; the wrong proof is rejected with the kernel's mismatch (last two lines of `expectedoutput`)
- [x] Fixture regenerated by a recorded Lean and `lean4export` version: `scripts/proof-export.sh` builds `benchmarks/proof/tools` (Lean 4.25.0, `lean4export` and `lean4checker` at their `v4.25.0` tags) and records `examples/proof/kernel/exports/id_imp.ndjson` (sha256 `2adb6bc5…8330d`); `Fixtures::idImp` is that text byte for byte and `idImpBad` is one documented edit of it ([report](../reports/proof-export-fixture-2026-10-29.md))
- [x] `m1_export` benchmark input: `benchmarks/proof/m1_export` reads the recorded export from disk on the Osprey side and replays the same module through `lean4checker` on the Lean side; `lean.setup` re-exports the module every run and byte-compares it with the case's copy
- [x] `m1_export` recorded, 2026-10-29, `PROOF_RUNS=10`, Lean 4.25.0 on arm64 macOS: Osprey 3 ms / 2256 KiB against `lean4checker` 433 ms / 52576 KiB, pass (`osprey_input` `2ab95a53…e053`, `lean_input` `637f6d7d…246d`, `osprey_binary` `748e523c…f7adf0`, `lean_binary` `be06b289…f9201`). Generated rows live in `target/proof/` and are not committed; this line is the record

### M2 — a proof is typed
- [x] ML-layout proof parser for `theorem name (binders) : type := term` (`src/surface.ospml`), over a whitespace-delimited demo tokenizer: `p:=h`, `p→q` and `∀p` without spaces are not read
- [x] Direct elaborator to M0 terms; `id_imp_typed`, `imp_trans`, `forall_typed` check and `wrong_typed` is rejected

### M3 — inductives
- [ ] Inductive declarations, constructors, recursors, ι-reduction
- [ ] `And`, `Iff`, `Eq` as Lean declares them
- [ ] `and_swap` checks from a Lean export and from the M2 surface
- [ ] Road-test report and the decision point

### Phase 1 — pluggable disciplines
- [x] `types=` marker and `[project].types`; `[discipline]` registry; `discipline-error`
- [x] `check_module` seam in assembly, checker process, JSON protocol, time limit, result cache
- [x] Review P0.3: sources visited in dependency order; a request carries the inferred ML signature of every `hm` module it imports and the checker-written signature of every disciplined one (`discipline/imports.rs`; three seam tests)
- [x] Review P0.3, rest / review 2 item 6: an inferred `hm` signature names every exported type and value it leaves out (`omitted[]`, protocol version 2), so no checker trusts a partial signature; an `hm` import that does not type-check is a `discipline-error` and no checker runs after it
- [ ] Exported types and values rendered in an inferred `hm` signature (today named as omitted)
- [x] Review P1.8: one deadline covers checker build, spawn, request write (own thread) and response; real-process regressions for a checker that never reads stdin (`discipline/runner_tests.rs`)
- [x] Review P1.9: checker binary named by the digest of every package source, the manifest and the compiler (`discipline/inputs.rs`); results keyed on the request and the built binary; read failures are errors
- [x] A checker may be a project directory (`[discipline] checker = "<dir>"` with its own `osprey.toml`), built like any project, so a discipline is not confined to one file; the directory's manifest and every module are build inputs
- [x] Review P1.10, harness: a failed timed run is a red case, sha256 of every kernel source and both binaries, `getrusage` peak memory, reports under `target/`, `lean.cmd`/`args` per case
- [x] Review P1.10, comparison: `m1_export` re-checks the same recorded module on both sides (Osprey reader of the export vs `lean4checker` replay, invoked directly with no `lake` start-up on Lean's clock); `osprey_input` hashes every kernel source and the export, `lean_input` the module, `lean.cmd`, `lean.setup` and `lean.bin`, `lean_binary` the `lean4checker` that ran; `osprey_input` follows the manifest's `source_roots`, so the nine kernel modules outside the entry's project are in it
- [x] Erased modules parsed by the ordinary frontend and checked against the written signature
- [x] `osprey <file> --signature` prints the ML export signature the seam renders (`crates/osprey-cli/src/signature.rs`); `options.compiler` in every request
- [x] `examples/disciplines/passthrough` with the round trip under GC, ARC and wasm32 (`make _test_disciplines`; the round trip is a project of its own rather than a `tests/` corpus twin, so the differential harness does not run it — the target diffs it byte-for-byte against its golden on all three)
- [x] `examples/disciplines/bounded`: `// @bound n in lo..hi` checked at every call site by interval arithmetic, erased to `int`, diagnostics through the protocol; the checker is a project of twelve modules under `src/`, its twelve-case protocol suite and the `bounded_roundtrip` goldens under GC, ARC and wasm32 in `make _test_disciplines`
- [x] Failure tests of [TYPES-DISCIPLINE-VERIFICATION] (`discipline/tests.rs`: unknown, duplicate, crash, timeout, malformed, unparsable erased, signature failure, version)
- [x] Purity scan over `crates/` (`tests/discipline_purity.rs`: every registered name in a string literal, the theory words anywhere; green with both reference packages registered)
- [ ] Byte-identical output and diagnostics for every markerless program; no `hm` benchmark change

### Phase 2 — kernel and trust
- [ ] Kernel specification with every judgement as inference rules
- [ ] Kernel under 5,000 lines in a separate process; `osprey prove import`
- [ ] `tests/proof/adversarial/` seeded; collision and overflow tests
- [ ] Cross-check against `lean4lean` and `nanoda`; core, `Std`, Mathlib gates with speed tests

### Phases 3 to 7
- [ ] Phase 3: proof AST, ML frontend, unifier, classes, `osprey prove check`, LSP goal view
- [ ] Phase 4: tactics, `hammer`, `benchmarks/proof/` with miniF2F and PutnamBench
- [ ] Phase 5: `osprey prove port`, sync job, re-proving, `@osprey/mathlib`, upstream bridge
- [ ] Phase 6: erasure, mixed-discipline corpus program, refinement discipline
- [ ] Phase 7: the workbench, tracked in [plan 0042](0042-proof-workbench.md)

## Verified constraints

Checked against the tree on 2026-10-29.

- `osprey_types::check_program(&Program) -> Vec<TypeError>` (`crates/osprey-types/src/check/driver.rs`) is the single checker entry; the seam goes in front of it. `Type` (`crates/osprey-types/src/ty.rs`) has `Var`, `Con`, `Fun`, `Record`, `Union` and nothing value-indexed, which is why the dependent theory stays outside it.
- [FLAVOR-BOUNDARY] already forbids branching on flavor below the canonical AST; the discipline seam extends the same rule to typing.
- `int` is a 64-bit `i64` and `Map<K, V>` is immutable; there is no arbitrary-precision integer, no mutable array and no structural-key hash map in the specs. The M0 to M2 kernel is written on built-in maps and lists; the full kernel of phase 2 (Mathlib-sized environments, structural-key term store) waits on [PROOF-STDLIB], pulled in by measured need.
- The 2026 Lean kernel bugs ([PROOF-TRUST-LESSONS]): nested-inductive projection check plus a hash collision gave an axiom-free `0 = 1` (issue #14576, CVE-2026-72844, fixed in the 4.32 line); three more were found by AI adversarial testing before 4.34.0, one a reference-count overflow turned into use-after-free. `nanoda` accepted the Collatz "disproof" because an old version was run. The public proof of concept exists and becomes a pinned rejection test.
- Independent checkers of Lean's theory exist (`lean4lean`, `nanoda`) and `lean4export` produces a text export; the import path is known work.

## Road test first: the first proof

The plan is ordered so that a proof checks on an Osprey-written kernel before any compiler, standard-library or syntax work starts. Each milestone answers one question about how hard this is going to be, and records the answer in `docs/reports/proof-road-test-<date>.md`: lines of Osprey written, hours taken, and every language or library gap that hurt.

### M0 — a proof checks (days, not weeks)

A plain Osprey program, ML flavor, under `examples/proof/kernel/`, run with `osprey run`. No seam, no new syntax, no runtime change. It implements the Pi fragment of [PROOF-KERNEL-THEORY] only: `Sort u` with level expressions, dependent function types, lambda, application, `let`, de Bruijn indices, β/δ/ζ/η, and an environment of constants. Terms are Osprey union values; `int` and `Map<string, V>` are enough because nothing is a literal yet.

- The proof: the term `fun (p : Prop) (h : p) => h` against the type `∀ (p : Prop), p → p`. `osprey run examples/proof/kernel/main.ospml` prints `checked: id_imp`.
- The control: `fun (p q : Prop) (h : p) => h` against `∀ (p q : Prop), p → q` is rejected with the two terms that failed to be definitionally equal, as a typed `Result`, never a panic.
- Both are corpus programs with `.expectedoutput`, so they run under GC, ARC and wasm32 from day one.
- The first benchmark: `benchmarks/proof/m0_identity` times the Osprey kernel against Lean's kernel on the same identity theorem and fails if Osprey is slower ([PROOF-SPEED]). The harness, the report format and the gate exist from this milestone; every later milestone adds its input to the same harness.

Answers: can a type checker with substitution, whnf and definitional equality be written comfortably in Osprey as it is today, and how fast does it run.

### M1 — a recorded Lean export checks (the first real signal)

The same kernel reads the `lean4export` text of a one-theorem Lean file (`theorem id_imp : ∀ p : Prop, p → p := fun p h => h`): the export parser for constants, levels and the expression forms of M0, nothing more. The program checks Lean's term and prints the same line. A second Lean file with a wrong proof is a rejection test.

Answers: does the kernel agree with Lean on real terms, and how much of the export format is painful to parse in Osprey. The benchmark gains `m1_export`: the export is checked by both kernels and Osprey must be faster.

### M2 — a proof is typed, not pasted

A minimal ML-layout proof parser in the same package: `theorem id_imp (p : Prop) (h : p) : p := h` in a `.ospml` file elaborates, by a direct elaborator with no metavariables, to the M0 term and checks. This is the first line of [PROOF-SURFACE-FLAVORS] and the moment a person can write a proof on Osprey.

Answers: how far a direct elaborator gets before unification is forced, and what the surface will feel like.

### M3 — inductives, and the first Mathlib-shaped theorem

Inductive declarations, constructors, recursors and ι-reduction. The target is `And`, `Iff` and `Eq` as Lean declares them, then `theorem and_swap (p q : Prop) : p ∧ q → q ∧ p`, checked both from a Lean export and from the M2 surface. Nested and mutual encodings, quotients and literals are still out.

Answers: the size and shape of the real kernel, and whether the structural-key map and term store of [PROOF-STDLIB] are now blocking. From here the standard-library work is pulled in by a measured need, not scheduled ahead of it.

### Decision point

After M3 the report says whether the full kernel proceeds in Osprey as planned or what in the language and library must change first. Phases below start only after that report and the owner's go.

## Phases

### Phase 0 — standard-library prerequisites [PROOF-STDLIB]

Scheduled by M3's findings, in the order the kernel needs them:

1. `bigint` with the accelerated `Nat` operation set, total, `Result`-returning division; both memory backends, native and wasm32; differential corpus programs. Needed at the first `Nat` literal.
2. `Map<K, V>` over structural keys, and a content-addressed term store that resolves hash collisions structurally (a planted-collision test). Needed when M3's environment outgrows string keys.
3. `Array<T>` and `Cell<T>` under [MODULES-STATE]. Needed by the unifier of phase 3, not by the kernel.
4. Benchmarks in `benchmarks/` for bigint arithmetic and term-store lookups against Rust baselines.

Exit: each addition specified in spec 0012, corpus-tested under GC and ARC and on wasm32, with recorded benchmark numbers.

### Phase 1 — pluggable disciplines [spec 0050](../specs/language/0050-TypeDisciplines.md)

A deliverable in its own right: after this phase any type system can be brought to Osprey as a package, with or without the prover that follows.

1. The `types=` marker and `[project].types` ([TYPES-DISCIPLINE-SELECT]), the `[discipline]` manifest table ([TYPES-DISCIPLINE-REGISTRY]), `discipline-error` for an unknown or duplicate name.
2. The seam: `check_module` in project assembly, in front of `check_program`, with the checker process, the JSON protocol, the time limit and the result cache ([TYPES-DISCIPLINE-SEAM], [TYPES-DISCIPLINE-PROTOCOL]). The erased module is parsed by the ordinary frontend and checked by `check_program` against the signature the checker wrote.
3. The `passthrough` reference discipline under `examples/disciplines/`, proving the round trip both ways in the differential corpus under GC, ARC and wasm32.
4. The `bounded` reference discipline: a real theory in 767 lines over twelve modules, erasing to `int`, with diagnostics through the protocol; the template every later discipline copies.
5. The failure tests of [TYPES-DISCIPLINE-VERIFICATION] and the grep test over `crates/`.

Exit: both reference packages green; byte-identical output and diagnostics for every markerless program over the whole corpus; no benchmark change for `hm`-only projects.

### Phase 2 — kernel and trust [PROOF-KERNEL], [PROOF-TRUST]

1. The kernel specification document with every judgement as inference rules.
2. The kernel in Osprey, under 5,000 lines, every function naming its rule, pure `Result` interface, separate process.
3. `osprey prove import` for `lean4export`.
4. `tests/proof/adversarial/` seeded with the CVE-2026-72844 proof of concept and the other 2026 cases; the collision test; the overflow test.
5. Cross-check against `lean4lean` and `nanoda` at current release on the same export.

Gates, in order: Lean's core library checks; `Std` checks; the full pinned Mathlib export checks, cross-checked, with the first `docs/reports/proof-trust-<date>.md`. Each gate has its speed test: Osprey's check of the same export is faster than Lean's on the same machine, or the gate is red ([PROOF-SPEED]).

### Phase 3 — surface language and elaborator [PROOF-SURFACE], [PROOF-ELAB]

1. The canonical proof AST and its ML frontend ([PROOF-SURFACE-FLAVORS]); ML first because Lean 4's syntax is already ML-shaped and the port's readers know it. Default and Python frontends are a later step of this phase, gated on cross-flavor equivalence of the elaborated kernel terms.
2. Unifier, type-class resolution, coercions, literals, attributes, notation and macros.
3. `osprey prove check` and the LSP goal view.

Exit: `Mathlib/Algebra/Group/Basic` translated by hand elaborates to terms the kernel finds definitionally equal to the imported ones ([PROOF-MATHLIB-DRIFT]).

### Phase 4 — tactics and automation [PROOF-TACTICS]

1. The metaprogramming API and the core tactic list, each with a Mathlib-derived test corpus.
2. `hammer` over Z3/cvc5 and E/Vampire with kernel reconstruction; unreconstructed answers are hints.
3. `benchmarks/proof/` with miniF2F and PutnamBench against the current Lean release; results recorded with hashes.

Exit: every tactic on the list passes its corpus; the first benchmark report is published. No score is quoted before it is recorded.

### Phase 5 — the Mathlib port [PROOF-MATHLIB]

1. `osprey prove port`: statements, definitions, structures, instances and notations to readable `.ospml` mirroring Mathlib's tree, proofs `by imported`.
2. The sync job ([PROOF-EXPORT-SYNC]) on a fixed cadence, opening the update pull request, red on any failure.
3. Re-proving: translated tactic scripts first, automation second; per-module fraction in the trust report.
4. Licence notices and the `@osprey/mathlib` package through [plan 0020](0020-package-manager.md).
5. The two-way bridge ([PROOF-MATHLIB-UPSTREAM]): every Mathlib defect the port or the adversarial suite finds goes upstream with Osprey's evidence; local patches are pending-only and listed in the trust report with their upstream link; kernel findings about Lean are disclosed to its maintainers before the report is published.

Exit: the pinned Mathlib commit is fully available by Lean name (layer 1), its statements translated with drift checked (layer 2), and the re-proved fraction reported honestly (layer 3).

### Phase 6 — programs and proofs together [PROOF-BOUNDARY], [PROOF-ERASE]

1. Erasure to the canonical AST with `@[erase int]` obligations checked by the kernel.
2. A corpus program in which an HM module calls a verified `lean4` function and a `lean4` module calls HM code, under both backends and on wasm32.
3. A refinement-type discipline over the existing KLEE/Z3 tooling of [plan 0034](0034-symbolic-execution.md), grown from the `bounded` template of phase 1, as the first discipline with a theory that is not Lean's.

Exit: the boundary rules hold in the differential corpus and a second discipline exists outside `crates/`.

### Phase 7 — the proof workbench [PROOF-WORKBENCH]

Delivered by [plan 0042](0042-proof-workbench.md) against [spec 0051](../specs/applications/0051-ProofWorkbench.md); its first three phases run in parallel with M0 to M3 here, and its phase 4 waits on M1 and M3. The steps below are the proof system's side of that plan.

1. Mathematics as the only input: Unicode notation through the Lean abbreviation table and a palette, statements and proof steps written as a mathematician phrases them, translated to the ML proof surface and shown beside the mathematics. The user writes no code; the generated `.ospml` is the saved artifact.
2. The goal pane over the checker protocol's goal-state capability, re-elaborating the declaration under the cursor on every edit without blocking typing; failing steps underlined with the terms that did not unify.
3. Code actions for `cases`, `induction`, `constructor` and `exact?` that edit the proof in place; refining goals from the pane.
4. Statement-shape search over the pinned Mathlib, hover with statement and docstring, jump to Mathlib source and to the Lean original.
5. The rendered document view, which is also the export.
6. The scratch board: freehand strokes rendered as chalk on slate, a chalk theme for typed mathematics, and handwriting recognition feeding the same statement and step input.
7. Hosts: native macOS and iOS, VS Code through `@osprey/vscode`, and the browser through wasm32 with a web host; one application, three hosts, proven by the same fixture-driven `update` tests.

Exit: a mathematician new to Osprey writes and checks `and_swap` in under ten minutes without documentation, recorded in the road-test report; every service used is reachable by any editor through spec 0020 or the checker protocol.

## Pi delegation

Bulk, mechanical work goes to pi under the AGENTS.md rules: the `lean4export` parser, the tactic test corpora, the statement translator's per-module runs and the benchmark harness. Kernel code, the seam and the trust report are written and reviewed here.

## Results first

Every public claim about trust, speed or automation is a link to a dated report in `docs/reports/` ([PROOF-TRUST], [PROOF-TACTICS-BENCH]). The claim that lands is the one the Lean community cannot answer: their own exploit rejected by the adversarial suite, their library checked and cross-checked at current releases, the benchmark against their current release with hashes. The bridge stays two-way while that happens ([PROOF-MATHLIB-UPSTREAM]).

## What this plan never does

- Change `Type`, unification, effect rows or monomorphisation in `crates/osprey-types`.
- Put a discipline name, a theory or a tactic in `crates/`.
- Trust a tactic, a solver, a model or a hash.
- Quote a benchmark, a wall time or a "most trusted" claim without a dated report in `docs/reports/`.
- Merge a kernel, elaborator or tactic change that makes any `benchmarks/proof/` gate slower than Lean on the same input.
