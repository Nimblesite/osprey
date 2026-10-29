# Adversarial review 2 — Plan 0041 dependent types and Mathlib

Reviewed against the tree on the day of writing. Every claim below was checked against source, tests, make targets and CI. Fixes from review 1 are credited where they actually landed; everything else gets the treatment it earned.

## Verdict

The kernel equality hole is genuinely closed, universe instantiation is real, and the discipline seam now carries import signatures. Credit where due — those were the P0s and they are fixed with adversarial tests, not just edits.

But the headline claims are still ahead of the evidence. The "Osprey beats Lean" gate is still a rigged comparison that also fails to hash its own input correctly. The M1 box says "a Lean proof checks" while every fixture was typed out by hand, and the export reader rejects every real Lean file on sight. The make target that promises "a failing suite exits non-zero on GC and ARC" pipes the suite through `tail`, which throws the exit code in the bin. The plan is now honest in its checkboxes and dishonest in its titles.

## What actually got fixed (verified, not taken on faith)

- **P0.1 equality:** `Terms::equal` is structural over de Bruijn indices and every universe argument (`src/term.ospml:104-142`); `Reduce::defeq` no longer short-circuits through the pretty-printer. Repeated-binder-name and `Const "F" [Zero]` vs `Const "F" [Succ Zero]` rejections are pinned in `tests/terms.ospml` and `tests/infer.ospml`, including the theorem-level smuggling case. Fixed properly.
- **P0.2 universes:** `Infer::constant` instantiates the declaration type at the use-site levels, arity mismatches are rejected, `Environment::add` scoping rejects undeclared/duplicate parameters, definitions unfold instantiated (`src/env.ospml`, `src/infer.ospml:35-39`). `Levels::equal` is on normal forms with a correct soundness argument in the comment. Fixed properly.
- **P0.3 imports, half:** sources are visited in dependency order and requests now carry real signatures — the inferred ML signature of imported `hm` modules and checker-written signatures of disciplined ones (`discipline/imports.rs`, three seam tests pinning both directions). This is the spec's `check_module(module, imported_signatures)` contract finally existing. Half, because of finding 6 below.
- **P1.5/6:** golden regenerated with all nine lines; the assertion suite is wired into `_test_proof_kernel` on GC, ARC and wasm32, and the export reader runs through the kernel in `main.ospml`. Mostly — see finding 2.
- **Checker read failures:** `executable()` now propagates an unreadable checker source instead of `unwrap_or_default()` into the cache key. stdout/stderr are drained on threads so a chatty checker can't deadlock on a full pipe.

That is real work. The rest of this document is why the milestone column still can't be trusted.

## P0 — still blocking

### 1. The speed gate is still not measuring the thing the spec names, and it can pass on a crash

[PROOF-SPEED] says both kernels re-check the same input. `benchmarks/proof/run.sh` still times `lean --run id_imp.lean` — Lean process startup, elaborator, frontend and all — against a prebuilt native Osprey binary whose theorem was compiled in ahead of time. Different work, different process model, one number. The Osprey side additionally reads no export at all: its fixtures are hand-written strings (`src/fixtures.ospml`), a fact the plan itself concedes in the one honest unchecked M1 box.

Worse, the harness around it is broken on its own terms:

- `wall_ms` ignores the exit status of every timed invocation (`run.sh:34` — `"$@" >/dev/null 2>&1`). A run that dies instantly produces the minimum time. Best-of-10 selection over a field that includes crashes is a machine for manufacturing false passes.
- The recorded input hash is `$(sha "/src/main.ospml")` — a literal root-absolute path that does not exist. `shasum` fails, the field is empty, and every results line carries the same non-hash. The provenance column is decorative. It was also only ever `main.ospml`, never the kernel modules that decide the result.
- `peak_kb` uses `/usr/bin/time -l`, which is the BSD spelling; on the Linux CI runner it produces nothing, and nothing checks. Empty memory metrics flow into the JSONL unvalidated.
- Results are appended forever into `benchmarks/proof/results.jsonl` in the source tree — generated output accumulating in version control.

A gate this eager to green-light itself should not be cited anywhere as evidence that the kernel is fast.

### 2. The test target's GC/ARC suite leg swallows its own failure signal

`_test_proof_kernel` (`scripts/make/tests.mk:331-344`) comments that "a failing suite exits non-zero on GC and ARC", then runs:

```make
./$(BIN) examples/proof/kernel/tests --run --memory=gc 2>/dev/null | tail -1
```

Unless `pipefail` is in force, the recipe's status is `tail`'s — zero regardless of what the suite reported — and `tail -1` output is diffed against nothing. The wasm32 leg at least greps `failed=0`; the native legs are theatre. The one sentence in the repo that claims the adversarial suite gates GC and ARC is attached to the one pipeline that discards the result. Fix it with `set -o pipefail` (or drop the pipe and check the TAP line), and while there, have the wasm smoke output for the example diffed against the golden rather than trusted.

### 3. The checker deadline still does not cover the phase where a bad checker hangs

`ProcessRunner::run` (`discipline/runner.rs:74-80`): spawn, then `feed(&mut child, request)?`, then `wait(child, discipline.timeout)`. The clock starts in `wait`. `feed` is a blocking `write_all` with no timeout: a checker that doesn't read stdin fills its pipe buffer and blocks the compiler forever, exactly the case the plan's own P1.8 box describes — still open, at least. Startup and checker build are likewise outside the deadline, so "time limit" means "time limit for the part of the run that was never the problem".

### 4. Checker binaries and cache entries are keyed on one file

`executable()` names the binary `{discipline}-{digest of discipline.checker}` and `cache::entry_path` keys on request line + binary digest. The checker is compiled as an Osprey *project* — imports, packages and all — but none of that enters the hash. Edit a module the checker imports, upgrade the compiler, and the seam keeps serving the stale binary and its cached verdicts as fresh. For a component whose whole job is being a trust boundary, "rebuilt only when the entry file's bytes change" is a cache-poisoning feature, not an optimization. The P1.9 box is honestly unchecked; the code underneath it is honestly wrong.

### 5. M1's title is currently false by the plan's own admission

"M1 — a Lean proof checks" is ticked on the strength of fixtures that were typed in by hand. The plan's own unticked box says so. And it's worse than provenance: the reader hard-rejects `#IND`, `#CTOR`, `#REC`, `#QUOT` and friends (`src/export.ospml`), and essentially every real Lean export begins by declaring inductives — `Or`, `And`, `Eq`, `Quot` initialization. There is no non-trivial file Lean's exporter produces that this reader will fully accept. So the accurate status is: *a text in Lean's export format, describing a theorem that uses nothing but `Prop` and `→`, written by a human, checks.* That is a fine M0.5. It is not "a Lean proof checks". The hand-written fixtures were forced by this limitation, which is itself the evidence that M1 is structurally waiting on M3.

## P1 — overstated or unsafe

### 6. HM import signatures silently drop everything that isn't a function

`imports.rs::item` returns `None` for exported values and types, and the request gives the checker no way to know the signature is partial. A disciplined module importing an `hm` module that exports a union type receives a signature that looks complete and isn't. The unchecked plan item admits functions-only; the protocol does not. Mark partial signatures in the protocol or refuse to import such modules — silent incompleteness in a verification seam is the false-negative shape this project calls a showstopper.

### 7. Protocol version 1 was broken and not bumped

`Request` gained a required `imports` array — a wire-format change — while `protocol::VERSION` stayed `1`. Any checker built against the original v1 misparses today's requests with no signal. Bump it or pay for it later.

### 8. The M2 surface is a fixture-shaped lexer, not a parser

`Surface::tokens` splits on whitespace after padding only `(`, `)`, `,`. `p:=h`, `p→q`, `∀p`, or a `theorem` token inside an expression either mis-tokenize or mis-parse. Fine for the seven hardcoded demo lines; the box is ticked as "ML-layout proof parser", which will disappoint the first real input. One sentence in the module comment saying "whitespace-delimited demo input only" would make it honest; making it a real lexer can wait.

### 9. Ledger hygiene, again

- "The kernel cannot be written until [PROOF-STDLIB] lands" still sits in *Verified constraints* directly beneath a written kernel that uses only built-in maps and lists. Qualify it to the full kernel or delete it; a constraints section that contradicts the checklist above it is worse than no section.
- Reconcile the status lines in specs 0049/0050 with the fact that M0–M2 are now implemented and gated — a spec saying "not started" above a green make target fails the same audit in the opposite direction as review 1's findings.
- CI's Lean toolchain remains pinned indefinitely while [PROOF-SPEED] demands a current-release baseline; the pin is fine for reproducibility, the claim of a *baseline* is not.

## What would make the next review boring

1. Fix `run.sh`: fail on any nonzero timed run, hash the whole Osprey input (fix the `"/src/main.ospml"` literal), portable memory measurement, validate every recorded field, write results outside the source tree. Then have both sides consume the *same* export text.
2. `pipefail` the proof-kernel make target and assert the TAP summary on all three backends.
3. Put spawn, feed and build inside the deadline; add the never-reads-stdin regression.
4. Key binaries and cache entries on the full checker build input.
5. Regenerate one fixture with a pinned real `lean4export` run, or rename M1 to what it is.
6. Mark or forbid partial HM signatures; bump the protocol version.

The distance between this plan and its checkboxes is now measured in single-digit items. Close them and the next review has nothing to say — which is the only outcome this document wants.

## Response — 2026-10-29

Every item above, with what changed and where the evidence is. Nothing here is ticked on intent.

1. **Speed gate.** `benchmarks/proof/run.sh`: every timed invocation's exit status is checked (a non-zero exit is a red case with a message and no timing); the input hash is `osprey_input` over every `*.ospml` and `osprey.toml` of the project plus the case's data files, and `lean_input` over the module, `lean.cmd` and `lean.setup`; peak memory is `getrusage(RUSAGE_CHILDREN)` through Python, normalised to KiB per platform, and an empty or zero reading is an error; results and reports go to `target/proof/`. The comparison [PROOF-SPEED] names now exists: `m1_export` re-checks one recorded Lean module on both sides, the Osprey reader on the `lean4export` NDJSON and `lean4checker` replaying the same module, invoked directly so `lake env` start-up is not on Lean's clock; `lean_binary` is the hash of the `lean4checker` that ran (`lean.bin`), not of a `lean` that did not. The README states what the number still is not (a cold Lean process versus a cold Osprey process, not the checking loop alone). `m0_identity` stays as the regression gate and the README says not to cite it.
2. **Make target.** `_test_proof_kernel` writes each suite run to a file and greps the TAP summary for `failed=0` on GC, ARC and wasm32; the wasm example run is diffed against the golden.
3. **Deadline.** One deadline taken at the top of `ProcessRunner::run` covers build, spawn, the request write on its own thread and the response; `discipline/runner_tests.rs` kills a real checker that never reads stdin.
4. **Cache keys.** The checker binary is named by a digest of every source of the checker package, its manifest and the compiler; results are keyed on the request line and the built binary's bytes; an unreadable input is an error. The checker may now be a project directory (`checker = "src"`), so a discipline is not confined to the one file a single-file build resolves; its manifest and every module are inputs (`discipline/tests.rs`).
5. **M1.** The fixture is the recorded export of `benchmarks/proof/tools/IdImp.lean` by `lean4export` v4.25.0, NDJSON 3.1.0, byte for byte; the bad fixture is one documented edit of it; the reader reads that format and refuses every item kind it does not check by name. The M1 title says what it is: a `Prop`/`→` theorem, inductives waiting on M3.
6. **Partial signatures.** An inferred `hm` signature lists every exported type and value it leaves out in `omitted[]`; a checker that cannot stub an omitted item refuses the import (passthrough does).
7. **Protocol version** is 2.
8. **Surface.** The module comment names it a whitespace-delimited demo tokenizer.
9. **Ledger.** The [PROOF-STDLIB] constraint is qualified to the full kernel; both specs' status lines say what is implemented and gated; the README states the pin is bumped by hand and the number re-recorded.
