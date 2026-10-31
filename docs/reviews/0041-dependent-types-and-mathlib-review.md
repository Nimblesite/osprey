# Adversarial review — Plan 0041 dependent types and Mathlib

Historical review; findings and responses retain their recorded scope. Current development follows the [Lean oracle and discipline-correction contract](../messaging.md#proof-development-and-the-lean-oracle); this record does not define a competitive objective or establish consistency.

## Verdict

**Do not call M0 a trustworthy kernel, M1 a Lean-proof check, or Phase 1 a working pluggable discipline.** The tree contains useful prototypes, but the evidence is being mistaken for the contract. The current M0 equality implementation can identify distinct terms; the export reader does not make the kernel check a Lean export; and the discipline protocol deliberately sends an empty import list. Those are architectural blockers, not polish.

The plan's own status is also unreliable: it says no implementation has started while implementation exists, leaves the CI Lean item unchecked although CI installs Lean and runs the benchmark, and checks infrastructure items whose required acceptance evidence is absent.

## P0 — soundness and correctness blockers

### 1. Definitional equality delegates to a lossy pretty-printer

`examples/proof/kernel/src/term.ospml:63-79` renders terms with binder names, omits constant universe arguments, and then defines:

```ospml
export equal (left, right) = show (left, []) == show (right, [])
```

`examples/proof/kernel/src/reduce.ospml:28-31` calls that function before real comparison and accepts `true` immediately.

This is not structural equality. Repeated binder names make different de Bruijn indices print identically. For example, these distinct terms have the same rendering when every binder is named `x`:

```ospml
Lam "x" (Sort Zero) (Lam "x" (Sort Zero) (BVar 0))
Lam "x" (Sort Zero) (Lam "x" (Sort Zero) (BVar 1))
```

The same defect reaches theorem checking. With all binders named `x`, a proof of `∀ p q, q → q` can be compared against `∀ p q, q → p` after their distinct de Bruijn bodies have collapsed to the same string. The shipped `id_imp_bad` control uses distinct names, so it walks straight around the bug.

Constants are worse: `Terms::show` prints only `name` for `Const name levels`. Consequently `Const "F" [Zero]` and `Const "F" [Succ Zero]` compare equal before `constEq` can inspect their levels.

**Required before any further milestone claim:** replace printed equality with structural, alpha-correct comparison that includes every universe argument. Add rejection regressions for repeated binder names at both term and theorem level and for distinct universe instantiations. Run those regressions through the same GC, ARC and wasm32 gate as the identity example.

### 2. Universe-polymorphic constants are parsed but not instantiated

`examples/proof/kernel/src/infer.ospml:37-41` looks up a constant and returns `decl.ty`; the supplied `levels` are discarded. `examples/proof/kernel/src/reduce.ospml:18-24` unfolds a definition body without substituting universe arguments either. The export reader carefully populates `Decl.params` and `Const.levels`, but the kernel does not implement their semantics.

That invalidates the claimed M1 direction. Reading Lean's data structures is not checking Lean's theory. A one-theorem, universe-free fixture cannot conceal this once real exports contain universe-polymorphic declarations.

**Required:** validate universe arity and parameter scope, instantiate declaration types and bodies, and test unequal and equal instantiations. Until then, mark export support as parser-only.

### 3. The discipline seam supplies no imported signatures

`crates/osprey-project/src/discipline/protocol.rs:102-116` states the problem in its own comment and then constructs every request with:

```rust
imports: Vec::new(),
```

The central contract in spec 0050 is `check_module(module, imported_signatures)`, with imports supplied in dependency order. A checker cannot accept every HM import, reject an unsupported import, or check a disciplined module against its dependencies when it receives none. This is not a completed seam; it is a single-file preprocessing hook with the cross-module semantics removed.

The unit test at `crates/osprey-project/src/discipline/tests.rs:275-287` explicitly pins the empty list instead of testing the required behavior.

**Required:** move dispatch into dependency-ordered assembly, populate actual export signatures, and add HM→discipline and discipline→HM integration programs. Do not tick the assembly-seam item until those programs pass.

## P1 — the evidence does not establish the checked claims

### 4. The export work does not check an exported Lean proof

The current `examples/proof/kernel/export.ospml` is a reader for hand stored `.export` fixtures. `examples/proof/kernel/src/main.ospml` does not import it or check its declarations. The test manifest `examples/proof/kernel/tests/osprey.toml` omits `../export.ospml`, while `tests/export.ospml` imports `proof::Export`.

Even if that project wiring is fixed, the tests only compare rendered declarations. They do not feed the declarations through `Infer::check`, do not demonstrate output from Lean's actual exporter, and do not reject a wrong Lean proof through the kernel.

**Required M1 gate:** generate the fixture with a recorded Lean/`lean4export` version, parse it, add every declaration through the kernel, accept `id_imp`, and reject `id_imp_bad` for the kernel's reason. Parsing and rendering belong in a parser milestone, not as evidence that “a Lean proof checks.”

### 5. The current executable output and golden disagree

At review time `examples/proof/kernel/src/main.ospml:49-56` emits the original three reports plus four surface reports. `examples/proof/kernel/expectedoutput` still contains only the original three lines. Therefore the documented `make _test_proof_kernel` diff is stale against the current source.

This is exactly why unchecked roadmap entries cannot be treated as harmless: unlisted M2 work has already disturbed the M0 acceptance target without updating its oracle.

**Required:** keep the golden, main program and milestone checklist atomic. Run the actual make target before claiming any of M0–M2 green.

### 6. The kernel's assertion suites are outside the advertised gate

`scripts/make/tests.mk:328-334` runs only the stdout example under GC, ARC and wasm32. It does not run `examples/proof/kernel/tests`. The richer tests therefore do not protect the milestone gate, and the export suite is not even included in its test project's source roots.

A three-line golden with one happy path and two carefully chosen controls is not a kernel test strategy.

**Required:** wire every kernel suite into `_test_proof_kernel`, run them on all promised targets, and make the adversarial equality cases mandatory.

### 7. No reference discipline demonstrates the seam end to end

The checklist ticks marker/registry/protocol/cache infrastructure, but `examples/disciplines/passthrough` and `examples/disciplines/bounded` are absent. The Rust tests use scripted responses and spawn no real checker. They do not prove that an Osprey checker builds, communicates, exports a signature, imports another module, erases correctly, or runs through codegen.

The two reference disciplines are not optional decorations; they are the plan's own proof that the abstraction works without a prover. Until both exist, Phase 1 has scaffolding rather than a demonstrated feature.

### 8. The checker timeout does not cover request delivery or checker construction

`crates/osprey-project/src/discipline/runner.rs:55-69` calls synchronous `feed` before entering timed `wait`. A checker that does not read stdin can block the compiler on a full pipe without the advertised timeout applying. Checker compilation also occurs outside the per-module deadline.

**Required:** include process startup and request I/O in the deadline, write stdin asynchronously, and add a real process regression for a checker that never reads. Keep the existing crash and timeout cases; scripted errors alone cannot prove process isolation.

### 9. Checker and result caches can go stale after dependency changes

`crates/osprey-project/src/discipline/runner.rs:42-52` names the checker executable using only the entry source's hash. `crates/osprey-project/src/discipline/cache.rs:17-28` likewise keys results using the request and that one source file. Imported checker sources, package contents and the actual checker binary are absent, contrary to spec 0050's “request and checker binary” contract.

The same cache code converts a checker read failure into an empty byte string with `unwrap_or_default()`. That silently changes cache identity instead of reporting the failure.

**Required:** hash the complete checker build inputs and verified binary, propagate read failures, and test that changing an imported checker module invalidates both build and result caches.

## P1 — the benchmark is not measuring the specified comparison

### 10. Lean elaboration/startup is compared with a prebuilt hardcoded AST checker

Spec 0049 requires both sides to re-check the same `lean4export` input. `benchmarks/proof/run.sh` instead times `lean --run id_imp.lean`, including Lean startup and elaboration, against an already compiled Osprey executable whose theorem and proof are hardcoded in `main.ospml`.

That comparison can produce a number, but not the claimed kernel-speed evidence. It measures different work.

Additional defects:

- `run.sh:28-36` discards timed-command failures; a failed timed invocation can still contribute a winning time.
- The input hash covers only `main.ospml`, not the kernel modules that determine the result.
- No Osprey binary hash is recorded despite the spec requiring both binary hashes.
- `/usr/bin/time -l` is macOS-specific while the CI job runs on Ubuntu; empty measurements are not rejected.
- CI pins Lean 4.25.0 “forever” while the specification requires a current-release baseline.
- Results are appended into `benchmarks/proof/results.jsonl` in the source tree rather than an explicit generated-artifact directory.

**Required:** benchmark equivalent export re-checking, require every timed invocation to succeed, hash complete inputs and binaries, use portable memory measurement, fail on missing metrics, and record a dated report. Until then, do not use this script to support a speed claim.

## P2 — plan and contract hygiene

### 11. The status fields and checkboxes are stale in both directions

- Specs 0049 and 0050 still say **“no implementation started”** despite kernel and seam code.
- The plan leaves Lean CI unchecked although `.github/workflows/ci.yml:539-592` installs Lean and runs the speed script.
- The plan says the road test needs no compiler change, then ticks a compiler resolver change as part of M0.
- “The kernel cannot be written until [PROOF-STDLIB] lands” contradicts the existing M0 kernel; qualify this as the full kernel or remove it.
- New surface and export code exists while M1/M2 remain unchecked and their actual acceptance gates are incomplete.

A checklist that is simultaneously overclaimed and underclaimed is not a delivery ledger. Update it only from executed, linked evidence.

### 12. “Giant leaps” is producing exactly the wrong review granularity

Kernel equality, export parsing, surface elaboration, process orchestration, caching and CI benchmarking are all trust-bearing components. Landing them as partially wired extras around a supposedly checked M0 makes failures invisible between checklist rows.

Split acceptance by semantic capability, not by file existence:

1. structural equality adversarial suite;
2. universe instantiation suite;
3. real exported-proof kernel check;
4. dependency-aware discipline round trips;
5. real checker process failure tests;
6. equivalent-work benchmark with a published report.

## Minimum bar to restore credibility

Before advancing the roadmap:

- Fix and regression-test the P0 equality defect.
- Implement universe instantiation or explicitly exclude it from every accepted input.
- Wire and run all kernel tests; repair the golden.
- Stop describing export parsing as proof checking.
- Populate imported signatures and prove both cross-discipline import directions with real packages.
- Replace the benchmark with same-input, same-work measurement.

## Validation performed for this review

This was a source and contract audit of the plan, specs 0049/0050, kernel, export/surface additions, discipline implementation, tests, benchmark and CI wiring. An attempted isolated execution of the shadowing counterexample and kernel test suite did not complete within 120 seconds, so this review does **not** claim an observed runtime result for that counterexample. The equality defect follows directly from the quoted implementation and still requires an executable rejection regression.

No full `make ci`, successful proof benchmark, real Lean export run, or end-to-end discipline package run was completed during this review.

## Response, 2026-10-29

Each item, what changed, and where the evidence is. Nothing below is ticked in plan 0041 without a test in `examples/proof/kernel/tests/` or `crates/`.

1. **Fixed.** `Terms::equal` (`src/term.ospml`) is structural: indices compared as integers, binder names ignored, every universe argument compared with `Levels::equal`, `Lam`/`Pi`/`Let` kept apart. `Terms::show` now prints `name.{u, v}` for constants. Regressions: `tests/terms.ospml` ("repeated binder names do not merge distinct indices", "every universe argument of a constant", "lambda from pi"), `tests/infer.ospml` ("repeated binder names cannot smuggle a proof of q → p past q → q": the all-`x` proof checks against `∀ p q, q → q` and is rejected against `∀ p q, q → p` with the kernel's mismatch). All run under GC, ARC and wasm32 in `make _test_proof_kernel`.
2. **Fixed.** `Levels::instantiate`, `Levels::declared`, `Terms::instantiateLevels`, `Terms::declared`; `Environment::instantiated` checks arity; `Environment::add` rejects an undeclared or repeated parameter; `Infer::constant` and `Reduce::whnfConst` instantiate at the use. `Levels::equal` is structural on a normal form that simplifies `max l l`, `max 0 l`, `imax l 0`, `imax l (succ r)`; incompleteness rejects, never accepts. Regressions: `tests/infer.ospml` ("instantiated at the universe it is used at", "arity mismatch", "distinct instantiations are not equal", "a definition unfolds at the universe it is used at", "undeclared universe parameter"), `tests/levels.ospml`.
3. **Fixed in the seam, open in the corpus.** `discipline::apply` now visits sources in dependency order (`imports::order`), infers the export signature of each `hm` module a disciplined module imports (assembly of the files before it, `infer_program`, rendered as `signature Name` with `name<t0> : (t0) -> t0` lines), and carries the signature a checker wrote for a disciplined import. Tests: `a_checker_receives_the_inferred_signature_of_each_hm_module_its_source_imports` (the rendered signature is also parsed by the ML frontend), `a_checker_receives_the_signature_another_checker_wrote_for_an_imported_module`, `an_hm_module_nobody_disciplined_imports_is_never_inferred_on_its_own`. Residue: an inferred `hm` signature lists exported functions only, not exported types or values; the real-package round trips wait on the Osprey-written reference disciplines (item 7).
4. **Fixed, with one residue.** `main.ospml` reads the export text, sends every declaration through `Infer::sort` and `Infer::check`, then `Environment::add`; `checked: id_imp (export)` and the rejection of the bad export with the kernel's mismatch are the last two golden lines. The reader's four cases are green. Residue: the fixture texts are hand-written to the `lean4export` 0.1.2 format, not produced by a recorded exporter run; that item stays open in the plan.
5. **Fixed.** Golden regenerated (seven lines); the make target passes on the kernel example.
6. **Fixed.** `_test_proof_kernel` runs the 45-case suite under GC, ARC and wasm32, with `pipefail` so a red suite is a red target. `tests/osprey.toml` lists `../export.ospml`.
7. **Open.** The `passthrough` a delegate wrote was a Python script placed in the build cache by hand; rejected and deleted. Both reference disciplines are to be Osprey packages built by the runner.
8, 9. **Open.** Listed in the plan.
10. **Open.** Listed in the plan; `elan-init.sh` URL corrected in CI.
11. **Fixed.** Status lines in the plan and specs 0049/0050; Lean CI ticked; M2 ticked; the recursive-signature item ticked for the `Result` case with the retained refusal named.
12. Acknowledged: the plan's checklist is now split by capability (items 1 to 6 of the review) rather than by file.
