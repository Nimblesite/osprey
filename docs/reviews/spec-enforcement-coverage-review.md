# Review closure audit: spec enforcement and coverage

Date: 2026-10-25. Verdict: **DO NOT CLOSE**.

An independent, read-only pi audit using DeepSeek V4.1 Flash checked all eight findings from the original review, the relevant code and assertions, plan claims and retained execution artifacts. The parent agent checked the conclusions against the source and reports. The review had been deleted in `84e6d135` after its five critical findings were addressed; it is restored because confirmation of every item is still outstanding. Concurrent Jinja suite expansion happened after that earlier verification.

The audit used source and retained execution evidence. It did not run full CI or inspect live GitHub rulesets. Package-manager paths below follow the concurrent move from `registry/` to `packages/manager/`; the reviewed source snapshot used the former paths. That move needs its own verification.

## Finding status

| Original finding | Status | Evidence and limit |
|---|---|---|
| 1. Executable interpolation bypasses capability scanning | Fixed | The scanner maintains nested string/interpolation frames. Surface tests passed 7/7; publication-to-lock tests passed 11/11, including missing and granted interpolation capabilities. |
| 2. Seven discovered package suites lack required artifacts | Jinja regression open | All seven have README, oracle constants and goldens. The expanded Jinja suite exposes silently ignored range steps, now quarantined; it cannot satisfy its desired 101-pass golden. See below. |
| 3. Python installation ignores pinned generated inputs | Fixed | Install and verify share checks of the actual generated project and lock before starting uv. Six retained tampered/clean cases pass; the subsequently regrouped suite was source-reviewed rather than re-executed in this audit. |
| 4. Bare package builds resolve paths under the derived manifest | Fixed | Derived paths are rebased; canonical compiler entry selection is covered. Eight actual Default/Python invocation forms passed. The registry end-to-end suite remains outside ordinary CI, as the plan states. |
| 5. GPU binary functions mishandle exceptional inputs | Fixed | Special-value branches are corrected; binary Cartesian-product assertions and the C-extracted shader suite pass. This is sampled library evidence, not universal hardware proof. |
| 6. Ambient terminal operations escape classification | Fixed | The compiler's declared ambient groups are checked against the classifier; terminal operations are classified and unknown `termX` names are rejected. The spec now states the actual fallback and inventory guard. |
| 7. GPU success text and bit-conversion assertions overstate proof | Fixed | Numeric bounds are stated accurately without changing tolerances. Actual emitted bit round trips pass both syntax variants across three native memory backends and WebAssembly, plus a forced Metal dispatch over 12 words. OpenCL hardware remains unverified. The shader header's correct-rounding overclaim is removed. |
| 8. Hunt accepts empty or incomplete clean reports | Completion proof outstanding | The checker rejects malformed, empty and incomplete reports; its 23 regression tests pass. Real planted-crash/replay tests pass 15/15, but the unchanged default-budget clean regression is still red. See below. |

## Outstanding: complete the existing Hunt gate

`scripts/hunt_expect.py` now rejects incomplete exploration and checks process status. CI installs KLEE and runs the deterministic expectations, `scripts/test-hunt-default-budget.py` and the planted-crash/replay gate. The previous false pass is fixed, but the existing clean program does not complete within its required 64-byte, 30-second configuration.

The latest failure is retained in `target/hunt-profiler-2026-10-25/current-red.log`: `test_clean_and_crash_finish_with_default_64_bytes_in_30_seconds` fails for `clean` with `ValueError: incomplete exploration`. The Hunt worker independently confirmed that failure; the crash case completes. Earlier retained clean reports also contain incomplete paths. The successful 15-case planted-crash proof in `target/logs/ci-prep-session/hunt-crashes-final.log` establishes detection and native replay, not completion of the default-budget clean case.

Close this item only after the existing deterministic and default-budget gates pass with complete exploration at their unchanged settings. Pi suggested reducing fixture input widths or separating the budget gate; those recommendations were rejected because they would narrow the existing gate. Do not increase the timeout, reduce the symbolic input width, skip the case or accept incomplete exploration as success. Plan 0034 must keep this completion proof open while the regression is red.

## Outstanding: quarantined Jinja range-step regression

At audit time, `packages/jinja2/tests/expected_output.txt` ended with `101 passed, 0 failed`, while the retained Default/GC/ARC transcripts under `target/logs/ci-prep-session/jinja-shared-chain-*`, `jinja-parser-shared-*` and `jinja-value-layout-*` ended with 72 passes. Those earlier runs establish the earlier suite, including zero live ARC objects; they cannot prove the added assertions.

After the audit, the worker expanding Jinja coverage confirmed a real failure: the CPython case `range(1, 6, 2)` produced a 100-pass, one-failure result because the port ignored the step. Commit `6041ac74` quarantines that silent failure in `packages/jinja2/src/expr.py` rather than returning incorrect values. The retained `target/logs/ci-prep-session/jinja-range-quarantine.stdout` contains `Jinja range step is not implemented; refusing incorrect output`; the desired 101-pass golden and regression remain. This is a confirmed implementation gap, not merely absent proof.

Keep the quarantine and failing oracle case explicit. Before closure, the complete current suite must pass under Default, GC and ARC, with byte-for-byte equality to the committed golden and a single zero-live-object ARC sentinel. Retain the exact source/test/oracle/golden inputs associated with those runs. A larger handwritten success transcript or a coverage percentage alone does not prove the suite passes.

## Accepted proof and limits

The seven formerly missing-artifact suites had these retained passing totals before subsequent coverage expansion: Einops 103, Jinja 72, NumPy 938, Re 6100, Safetensors 87, SciPy 2127 and Torch 354. Torch's complete current proof linked one common LLVM object against the three actual memory runtime archives, produced identical output on each and reported zero live ARC objects; its committed golden was captured from execution. The finite-difference oracle was regenerated with real pinned CPython Torch using an exactly representable step, without relaxing test tolerances.

The GPU C-extracted shader run passed 29,202,107 comparisons after vacuous self-comparisons were replaced with emitted-kernel tests. Existing strict exp/ln/tanh assertions remain; extended sampled tolerances remain 16/64 ulps. The new roundtrip fixture preserves signed zeros, infinities, NaN payloads, finite values and subnormals on the backends actually exercised.

The independent audit found no narrowing in the mandatory package matrix, golden comparisons, ARC sentinel checks, coverage thresholds or required Hunt wiring. This is not a claim that full branch CI passes. Delete this review only after the remaining items have retained passing evidence and a fresh closure audit accepts them.
