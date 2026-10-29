# Adversarial review 3 — Plan 0041 dependent types and Mathlib

Third pass, against `feat/type-disciplines` as it stands. Review 2's findings were largely answered, and answered well. This pass found the two things review 2 could not: **the M0 gate is red on this checkout**, and **the one comparison [PROOF-SPEED] actually names has never produced a recorded result**.

## Verdict

The engineering finally outpaced the paperwork. Then it tripped over the paperwork anyway: the flagship command exits 1, one test in the adversarial suite fails, and the only committed benchmark evidence is three rows of the case the README forbids citing.

`make _test_proof_kernel` cannot pass on this tree. Everything ticked downstream of it — M1's real export, the export re-check entry, the "45 cases, failed=0" claim — is resting on a gate that stops at its first `diff`.

## P0 — the tree is red, and the checklist is green

### 1. `osprey examples/proof/kernel --run` exits 1 with no output

The plan's second M0 box says this command "prints `checked: id_imp` and rejects `id_imp_bad` and `not_a_type`". Run on this checkout:

```
$ ./target/release/osprey examples/proof/kernel --run --memory=gc
exit=1
stdout: <empty>
stderr: .../examples/proof/kernel: codegen: unsupported construct:
        quarantined field access `name` on an untracked receiver:
        2 records declare it and the receiver's record was not tracked
```

Consequences, all mechanical:

- `_test_proof_kernel`'s first line is `./$(BIN) examples/proof/kernel --run --memory=gc 2>/dev/null | diff - examples/proof/kernel/expectedoutput`. Empty output against a nine-line golden is a non-zero `diff`, so the target aborts before the ARC leg, before the wasm leg, before the export-check legs, before the suite.
- The `test-proof` CI job runs that target. That step is red.
- `make test` includes it. Red.

Fair caveat, stated so nobody can wave it away: `target/release/osprey` is dated 18:38 and `src/infer.ospml` is dated 19:06, so this is either a stale binary or a live codegen regression. **Both possibilities are the same finding.** If the binary is stale, nobody ran the gate after the last source edit, and every tick is unverified. If it is not stale, the kernel example does not compile. There is no third reading in which the checkbox is honest.

Note also what still *does* compile: `examples/proof/export_check` prints `checked: id_imp (export)` (exit 0), and `examples/proof/kernel/tests` builds and runs 75 cases. So this is not a broken compiler or a broken tree-wide setting — it is specifically the M0 entry, after it was refactored onto `checked.ospml`, that no longer compiles. The quarantine diagnostic names two `name` fields on two records, which is `Decl.name` and `Binder.name`; that is a compiler limitation the refactor walked into, and it needs either a codegen fix or a rename, not a checklist edit.

### 2. A test in the kernel suite fails, in a way that will fail again on the next fixture

```
1..75
# tests=75 passed=74 failed=1 skipped=0

not ok 62 - a statement whose body is a proof, not a type, is rejected as not a type
# expect failed: expected `#0` is not a type; its type is `#1`,
#                got `a._@._internal._hyg.0` is not a type; its type is `p`
```

Source: `examples/proof/kernel/tests/export_mutations.ospml:47-48`, asserting the exact message string `` "`#0` is not a type; its type is `#1`" ``. The fixtures were regenerated from a real `lean4export` run, whose binder names are Lean's hygienic ones (`a._@._internal._hyg.0`), so the assertion now describes a rendering that no longer exists.

Two things are wrong here, and only one is the string.

- The plan ticks "adversarial structural-equality and universe cases ... `failed=0`". Observed: `failed=1`. The claim is false in the tree, today.
- The deeper defect is that the suite asserts on **pretty-printed text** for cases about **term structure**. That turns every fixture regeneration, every Lean version bump and every binder-naming change into a false red. The mutation test's purpose is "this export's statement is a proof, so refuse it"; the assertion should compare against the substituted term (`Support::sorted` of a term built in the test, or the error's structured shape), never a rendering. Updating the string to `a._@._internal._hyg.0` would make the suite green and keep the trap armed. This is the one place where the easy fix is the wrong fix.

Also note the count: the plan says 45 cases in two places. There are 75 `test (` blocks across seven files (11+13+7+11+23+4+6). The `_test_proof_kernel` comment still says the suite passes on all three backends. It does not pass on the first one.

### 3. The comparison [PROOF-SPEED] names has no recorded result

`m1_export` is now built correctly: `lean.setup` builds `lean4export`/`lean4checker` at the pinned revs, `lean.cmd` invokes `lean4checker IdImp` directly with `LEAN_PATH` from disk (no `lake` start-up on Lean's clock), `lean.bin` names the exact binary hashed into the row, and the Osprey side reads the same NDJSON from disk through the same `Checked::exportedDeclaring` the example uses. Good design. Then:

- `benchmarks/proof/results.jsonl` — tracked, three lines — contains **three `m0_identity` rows and zero `m1_export` rows**.
- `benchmarks/proof/reports/2026-10-29.md` — tracked, same three rows.

So the only measured evidence in the repository is for the case `benchmarks/proof/README.md` explicitly says must not be cited as kernel-speed evidence ("Different work on each side, so do not cite its numbers as kernel-speed evidence"). The case the spec specifies has no number, no verdict, and no hash anywhere in the tree. Its `lean.setup` needs a network `lake build` into a gitignored `tools/.lake/`; if that has ever succeeded, the successful run was not recorded.

A tick that says the case "re-checks the same recorded module on both sides" is currently a description of a directory layout. Run it, paste the row, then tick.

## P1 — the trust boundary still has soft edges

### 4. Generated artifacts committed into the source tree

`benchmarks/proof/reports/2026-10-29.md` and `benchmarks/proof/results.jsonl` are under version control, while `run.sh` now writes to `target/proof/` and the README documents `target/proof/results.jsonl` and `target/proof/reports/<date>.md`. Gitignore covers `benchmarks/proof/tools/.lake/` but not these. Both are stale the moment the next run happens, and both are already stale: the report's header names the toolchain `lean`, its rows name a different `lean bin` hash, and one row's `osprey_input` is `1e6e…` while a later row for the **same case** is `55fc…` — the traceability field silently re-identifying the same case with no explanation in the report. Either delete them or move them out of the tree; committing a benchmark record that its own generator no longer writes is worse than committing nothing.

### 5. The export reader accepts duplicate table indices

`Export`'s module comment states the invariant it depends on: "an item only ever refers to lower indices of its own table". The reader enforces the *referenced-exists* half (`unknown expr #…`) but never the *unique and increasing* half: `setName`, `setLevel` and `setExpr` are unconditional `mapSet` calls, and `nameOf`/`levelOf`/`exprOf` only look up. So a file that writes `ie 5` twice is accepted with last-writer-wins.

A trust boundary should not carry an unchecked invariant in a doc comment and an unchecked `mapSet` in the code. Either reject a re-declared index with its line number ("line 12: expression 5 is already defined") or write down why shadowing is sound. It costs four lines, and it is exactly the shape of malformed-input bug a proof kernel is expected to have closed before M1 is called done.

### 6. Provenance code contradicts its own documentation

- `TREE_SHA` hashes every non-`.lean`, non-dotfile, non-`lean.cmd`/`lean.setup` file in the case directory into **`osprey_input`** — which includes `lean.bin`, a Lean-side file. The README says "Lean-side files ... are left out". Either exclude it or fix the sentence; a hash whose documented meaning differs from its computed meaning is not a fingerprint.
- `m1_export/lean.bin` and `lean.cmd` hard-code `../tools/.lake/packages/...`, a path that exists only after a network `lake build` into a gitignored directory. That is acceptable for CI-with-cache, but the case then cannot be run from a fresh clone offline, and nothing in the case says so. One line in `benchmarks/proof/README.md` naming the prerequisite would stop the next person from filing a false bug.

### 7. Softer edges worth closing while the file is open

- `examples/disciplines/roundtrip/.osprey` is a leftover cache directory on disk (gitignored, so harmless to CI, but the repo asks for disk hygiene and the other discipline examples clean theirs in `_test_disciplines`).
- `crates/osprey-project/src/discipline/runner_tests.rs` begins with `#![cfg(unix)]` twice and has a doc comment split mid-sentence (`... proven with tiny` / `//! `sh` checkers in temp directories`). Cosmetic, but this is the file whose existence is the evidence for the deadline tick; make it look like evidence.

## Fixed since review 2 — verified, not assumed

Credit where it is earned, because the next review should not re-litigate these:

- **Harness:** `MEASURE_PY` fails on any non-zero exit and on empty or zero wall-time/peak-memory; `timed_side` records nothing for a failed run; `preflight` requires a `checked: ` line from both sides before timing; memory is `getrusage` normalised per platform; output goes to `target/proof/`; four hashes per row (`osprey_input`, `lean_input`, `osprey_binary`, `lean_binary`); `lean_bin` resolves past elan's multiplexer so the hashed binary is the one timed.
- **Deadline:** `ProcessRunner::run` takes one deadline before `executable()`, so build, spawn, stdin write (own thread) and response are all inside it. Real-process regressions exist and are wired (`#[path = "runner_tests.rs"] mod runner_tests;` in `runner.rs:247-248` — checked, because a test file nobody declares is review 2's favourite joke).
- **Caches:** `inputs::build_digest` folds the manifest, every source under the modelled roots, and the compiler digest into the binary name; a checker may be a project directory; read failures are errors, not empty digests.
- **Protocol:** version 2 with `imports[]` and per-import `omitted[]`, so a partial `hm` signature can no longer masquerade as complete — review 2's item 6, fixed properly rather than papered over.
- **Real fixture:** `examples/proof/kernel/exports/id_imp.ndjson` hashes to `2adb6bc5ff4e2fcaf9d804548fc2eaf0753902effbac3d70e1657d1b5998330d`, exactly the recorded sha in the plan and the report; `Fixtures::idImp` is byte-identical to the file (verified by extraction: 1134 bytes, equal), 24 lines, real hygienic binder names, and `export_check/id_imp.ndjson` and `m1_export/id_imp.ndjson` are the same bytes. `scripts/proof-export.sh` and `docs/reports/proof-export-fixture-2026-10-29.md` both exist and the report documents a genuine stop-at-the-gate on the 0.1.2 format before the reader was moved to 3.1.0 — that is how a provenance claim should look.
- **Gate integrity:** `_test_proof_kernel` no longer pipes a suite into `tail`; each suite writes to `target/proof/kernel-tests.<backend>.txt` and `grep -q "failed=0"` must find it. Review 2's item 2 is closed.
- **Reference disciplines:** `examples/disciplines/{passthrough,bounded,bounded_roundtrip}` exist, `_test_disciplines` drives `bounded` through GC/ARC/wasm32 goldens, a real rejection golden, a mutated out-of-range call site, and the checker's own protocol suite. The Phase 1 claim now has code behind it.

## What to do, in order

1. Make `osprey examples/proof/kernel --run` compile and print the golden. Rebuild before you conclude anything; if it still fails, fix the codegen quarantine or rename the records it trips on.
2. Fix `export_mutations.ospml:47` by removing its dependence on binder rendering — not by pasting the new hygienic string in.
3. Run `make _test_proof_kernel` and `zsh benchmarks/proof/run.sh` to completion; commit neither's output; link the session's `target/proof/reports/<date>.md` row for `m1_export` from the plan.
4. Delete the tracked `benchmarks/proof/results.jsonl` and `benchmarks/proof/reports/2026-10-29.md`.
5. Reject duplicate table indices in `Export`, with the line number.
6. Correct the 45→75 count and the `osprey_input`/Lean-side hash sentence.

## Proof for this review

Commands run, with their results:

- `./target/release/osprey examples/proof/kernel --run --memory=gc` → exit 1, empty stdout, `codegen: unsupported construct: quarantined field access ...` on stderr.
- `./target/release/osprey examples/proof/kernel/tests --run --memory=gc` → exit 1, `1..75`, `tests=75 passed=74 failed=1`, `not ok 62`.
- `cd examples/proof/export_check && ../../../target/release/osprey . --run --memory=gc` → exit 0, `checked: id_imp (export)`.
- `shasum -a 256` on the three `id_imp.ndjson` copies → all `2adb6bc5…8330d`, matching the plan and the recording report.
- Fixture extraction and comparison → 1134 bytes, string equals file bytes.
- `git ls-files benchmarks/proof` → the report and `results.jsonl` are tracked; `rg` and code reading for the reader's `mapSet` paths, `TREE_SHA`, `runner_tests` declaration, and the CI `test-proof` steps.

Not run: `make _test_proof_kernel` end to end (its first leg already fails), `zsh benchmarks/proof/run.sh` (needs a Lean toolchain and a network `lake build` in this environment), and a compiler rebuild, which is why finding 1 carries its stale-binary caveat.

## Response — 2026-10-29

Every finding, what changed and the command that shows it. Commits `51d4b91e` and `b6a37b19` on `feat/type-disciplines`.

1. **The M0 entry compiles.** Not a stale binary: a live codegen defect, and then a second one behind it. A self-recursive walk bound its `...rest` tail with no type, so `names rest` keyed a second instantiation with no element type and `first.name` (a field `Decl` and `Binder` both declare) was quarantined; `crates/osprey-codegen/src/pattern/list.rs` now gives the tail the list's own type. A mutually recursive search returning `Success(value = first)` merged to a `Result` block with no payload type; `crates/osprey-codegen/src/genfn.rs` (`checked_return`) gives a call through generic inlining or specialisation the checker's return type when its ABI shape is concrete. Neither was renamed around. `tests/core/collections/shared_field_list_recursion.test.{osp,ospml}` fails with the quarantine without each fix and passes on GC and ARC; the language corpus is 287 suites, 0 failed. `osprey examples/proof/kernel --run --memory=gc` exits 0 and matches the golden.
2. **No assertion on a rendering of Lean's names.** Kernel rejections that render terms now open with a stable kind (`type mismatch:`, `not a type:`, `not a function:`). Every mutation case that used to pin a rendering asserts four structural views instead (`tests/shapes.ospml`): the rejection's kind, whether the statement is a type in the export's own environment, the statement as read and the proof's inferred type, each compared by `Terms::equal` (binder names ignored) with a term the test builds. Case 62 asserts the statement is `∀ p : Prop, ∀ a : p, a`, its own sort check fails as `not a type`, and the body's type under the two binders is `BVar 1`. Sabotaging one expected term and one expected kind turned both cases red with the differing term printed. Case counts are corrected everywhere: 81 cases across seven suites, `failed=0` under GC, ARC and wasm32 (`make _test_proof_kernel`, exit 0).
3. **`m1_export` has a recorded result.** `PROOF_RUNS=10`, Lean 4.25.0, arm64 macOS: Osprey 3 ms / 2256 KiB against `lean4checker` 433 ms / 52576 KiB, pass, with all four hashes; the row is quoted in the plan's M1 section, since generated rows stay in `target/proof/`. `lean.setup` now re-exports `IdImp` every run and byte-compares it with the case's `id_imp.ndjson`; a one-byte edit of the copy is red at setup (`differ: char 979, line 22`). The README still states what the number is: a cold process against a cold process, not the checking loop alone.
4. **Tracked outputs deleted.** `benchmarks/proof/results.jsonl` and `benchmarks/proof/reports/` are gone from the tree; `run.sh` writes both under `target/proof/`.
5. **Duplicate indices.** `Export::freshIn` refuses an index its table already defines, name 0, level 0 and any negative index, with the line. Six cases in `tests/export_mutations.ospml`; each is red without the guard, and two of them (`name 0` defined, `level 1` defined twice) were accepted as `checked` before.
6. **Provenance.** `TREE_SHA` reads the manifest's `source_roots` (a root may be one file or a directory, inside or outside the project; a missing root is an error), so `m1_export`'s `osprey_input` covers the nine kernel modules under `examples/proof/kernel/src`, the entry, the manifest and the export, and nothing Lean-side; `lean.bin` moved to `lean_input`. The README names the first-run network `lake build` that `tools/.lake` needs, and that the case is red, never green, without it.
7. **Softer edges.** `examples/disciplines/roundtrip/.osprey` is removed and `_test_disciplines_passthrough` now deletes it after its run; `runner_tests.rs` has one `#![cfg(unix)]` and a joined doc comment.

Still open, stated so the next review need not find it: the required-checks ruleset does not yet require `Tests: proof kernel and Lean speed gate`, because `main`'s workflow has no such job; requiring it before this branch merges would leave every pull request to `main` pending. `scripts/verify-branch-protection.mjs` already expects it, so the verifier is red until the ruleset is updated at merge.
