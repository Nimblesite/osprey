---
layout: page
title: "Hunt for bugs: automated bug finding with symbolic execution"
description: "osprey hunt is a built-in automated bug finder. It uses KLEE symbolic execution, a smarter alternative to fuzz testing, to find the exact input that crashes your program, then reproduces the crash natively."
permalink: /docs/hunt/
tags: [testing, hunt, symbolic-execution, fuzzing, klee]
---

## What is Osprey Hunt?

`osprey hunt` is an **automated bug finder** built into the Osprey compiler. You point it at a program and it finds the **exact input that makes it crash** — no test cases to write.

It works by **symbolic execution** with [KLEE](https://klee-se.org/). A **fuzzer** throws random input at a program and hopes to hit a bug. Hunt instead treats the input as unknown and solves for values that steer execution down as many branches as its time budget allows, including the edge case a random fuzzer rarely reaches. Each finding is then **replayed on a normal native build**, so you know the crash is real.

## Why use it?

- **Find crashes your unit tests miss.** Tests check the inputs you thought of. Hunt searches for the ones you didn't.
- **Get a reproducible crash, not a guess.** Every finding comes with the source line, the concrete input bytes and a native replay verdict. Paste the input into a regression test and fix it.
- **Harden input handling.** Parsers, CLI tools, file readers and anything reading stdin or untrusted data are where crashes and security bugs hide.
- **Zero setup beyond the engine.** No harness, no fuzzing corpus, no annotations. One command and an HTML report.

Hunt complements testing and static analysis; it does not replace them. A reproduced finding is a real crash, but an empty report is not a proof of correctness.

The command is `osprey hunt`, with no leading dash. It accepts a Default, ML or Python-flavor source file, a project directory or an `osprey.toml` manifest. Python-flavor files carry `# osprey: flavor=python`.

## Run your first hunt

On macOS or Linux x86_64, install KLEE through Homebrew, then build Osprey from the repository root so the symbolic runtime is available:

```sh
brew install klee
make build
./target/release/osprey hunt tests/hunt/crash.osp --input-bytes 1 --max-time 10
```

This example deliberately calls C `abort()` only when its input is `"9"`. Hunt discovers that input, reports `abort crash.osp:4`, and confirms `reproduced (SIGABRT)` in a normal run. Exit status 1 is expected. This demonstrates detection of a planted failure. `process.osp` separately tests an unsupported engine operation; it is not the crash demo.

For a clean control, run:

```sh
./target/release/osprey hunt tests/hunt/clean.osp --input-bytes 2 --max-time 30
```

The clean control parses stdin as an integer and safely selects a list element, with fallbacks for invalid input and an out-of-range index. It expects `findings: none`; a run can still exhaust its budget. An `incomplete` section means paths stopped at an engine limit or budget, not that a bug was found. The former crash example, `tests/hunt/record_update_list.osp`, is fixed and also expects no findings.

For your own project:

```sh
osprey hunt ./my-project --max-time 60 --input-bytes 4
```

Start with a small input bound for parsers. Each extra symbolic byte can introduce more branches; increase it when you need to reach longer inputs. Hunting complements tests with expected results: it does not prove that your program implements its intended behavior, and an empty findings list is not a proof of correctness.

## Open the report

Hunt saves **HTML and JSON automatically** and prints their paths. Open `report.html` in your browser for a plain-English result, source excerpts, inputs and next steps. The report distinguishes a reproduced crash from an unsupported operation or unfinished analysis. No findings is not proof that the program is correct.

Expand **For AI** to copy the full investigation context: findings, replay details, exact engine arguments, raw diagnostics, source text and analysis limits. The report works offline without JavaScript. It contains local paths and source contents; review it before sharing.

Reports normally live under `target/hunt/<program-and-run-id>/`. Use `--out ./hunt-results` for a predictable directory that also keeps raw KLEE artifacts.

## Understand the path counts

Paths are different possible executions, not repeated test attempts. The crash demo explores four paths: three finish normally and one reaches the abort that Hunt detects and reproduces. Zero are unfinished. **That is a successful crash detection, not a crash missed one time in four.**

| Count | Meaning |
| --- | --- |
| Paths explored | Different executions reached by the symbolic engine, including paths that fail or stop early. |
| Normal exits | Paths that reached ordinary program completion. A detected crash is excluded from this count. |
| Crashes reproduced | Deduplicated findings that also caused a signal in a normal native run. |
| Unfinished | Paths stopped by an analysis limit, solver failure or engine interruption. Their behavior beyond that point was not checked. |

Findings are deduplicated by failure class and source location, so their count need not equal the number of failing paths. An unsupported operation is an engine limitation, not a demonstrated bug. Never calculate a detection success rate by dividing normal exits by explored paths.

## Read a finding

Each finding includes its class, Osprey source location when available, runtime frame, concrete stdin and file bytes, and a replay verdict.

| Report | What to do |
| --- | --- |
| `reproduced` | The native build also died from a signal. Keep the input and add a regression test before fixing the defect. |
| `klee-only` | The native build ran without that crash. Investigate the symbolic runtime model and the input; native replay currently restores stdin and files, not entropy or clock values. |
| `unmodelled` | The program reached an operation the symbolic runtime cannot model. This is a coverage boundary; native replay skips it. |
| `skipped` or `not run` | Read the reason, or rerun with native replay enabled. There is no native reproduction verdict yet. |
| `incomplete` | Some paths hit time, memory or solver limits. Adjust the budget or reduce the input bound. |

A memory-safety failure outside deliberate or unsafe C-boundary misuse points to a compiler, runtime or symbolic-model defect. Keep the reported source location and input when reporting it.

## Options and automation

```sh
osprey hunt ./my-project --json --quiet --max-time 60 > hunt.json
osprey hunt ./my-project --out ./hunt-results --input-bytes 4
```

| Option | Default and behavior |
| --- | --- |
| `--max-time <seconds>` | 60 seconds; positive whole number. |
| `--max-memory <mb>` | 2000 MB; minimum 256 MB. |
| `--input-bytes <n>` | 64 bytes per symbolic stream; accepts 0–4096. |
| `--out <dir>` | Save both reports and keep KLEE artifacts, including concrete `.ktest` inputs. Otherwise raw temporary output is deleted after the report context is captured. An unrelated non-empty directory is refused. |
| `--json` | Also write JSON to stdout for automation; saved reports remain enabled. |
| `--no-html` | Disable the saved HTML report. |
| `--no-json` | Disable the saved JSON report; an explicit `--json` still writes JSON to stdout. |
| `--quiet` | Suppress progress on stderr. |
| `--no-replay` | Skip the native build and replay; any finding then makes the command fail. |

Exit status is `0` when there is no failing finding, `1` for a native reproduction, any finding with replay disabled, or a compilation failure, and `2` for usage or toolchain errors. Skipped findings and incomplete paths can coexist with status `0`; automation that needs full exploration must inspect the report as well as the exit code.

From the repository, `make hunt FILE=tests/hunt/crash.osp HUNT_ARGS="--input-bytes 1 --max-time 10"` builds the prerequisites and wraps a single run. `make hunt-corpus` sweeps the test and example corpus; it is not currently a required CI check.

## Qualify a quick corpus

From a built checkout with KLEE installed:

```sh
python3 scripts/hunt_sample.py --out target/my-hunt-sample
```

This inventories runnable programs across tests, examples, book examples, package apps, registry apps and benchmarks. Projects are counted once; their individual modules and libraries without an entry point are excluded. Default/ML files, standalone Python-flavor files and Python-entry projects are included. Known engine limitations are sampled rather than silently skipped.

Each invocation gets a hard **10-second wall-clock deadline**, including compilation and replay. The sampler kills the whole process group on timeout. Its default engine budget is eight seconds with two symbolic bytes per stream. The ordinary Hunt `--max-time` flag limits the engine; it is not this whole-command deadline.

Open `index.html` for each program's run status and issues. Logs and path counts are under Details. `safe-runs.json` lists complete, finding-free runs with their recorded input and time limits; `quick-corpus.txt` contains the same program paths.

To gate the JSON list in CI, use a fresh output directory:

```sh
python3 scripts/hunt_sample.py --manifest target/my-hunt-sample/safe-runs.json --out target/hunt-ci
```

This reruns every listed program under the recorded limits and returns a nonzero exit if any run times out, reports a finding, remains incomplete, or fails. Passing applies to those limits; it does not prove safety for every input. The required CI job runs the pinned 215-program `tests/hunt/safe-runs.json` corpus at those same limits and uploads its reports.

## Verify deliberate crash detection

```sh
make hunt-crashes
```

The gate covers fifteen planted input cases: stdin, parsing, checked arithmetic, lists, maps, closures, recursion, unions, record invariants, JSON, file input, invalid C free, C double free, ML flavor and Python flavor. Each runs independently within ten seconds. Every expected class, source location, concrete input, file payload and native signal must match. Each case must also have a normal symbolic path, and benign stdin `Z` must finish normally with the exact expected output. Missing findings, unfinished analysis, tool errors, skipped replay and KLEE-only findings fail the gate.

These tests deliberately violate application invariants or the unsafe C boundary. They demonstrate detection of known failures; they are not new compiler defects. CI runs the real KLEE/native-replay gate in the existing required macOS coverage job and the mock/runtime tooling tests in the required C-runtime job. Reports are uploaded as `hunt-crash-reports`. A missing engine fails the crash gate rather than skipping it.

## Use it in VSCode

Open an `.osp` or `.ospml` file and select **Osprey: Hunt for Bugs** from the command palette or editor context menu. The extension saves the file, runs the compiler selected by the language server, and shows findings in the **Osprey Hunt** output channel and editor diagnostics. Set `osprey.hunt.maxTime` to change its time budget.

A reproduced finding is an error, an unreplayed or KLEE-only finding is a warning, and an unmodelled operation is information.

## Current boundaries

KLEE runs natively on macOS and Linux x86_64. Linux arm64 and Windows are not supported by the current workflow. If the engine or runtime bitcode is missing, Hunt exits with a setup error. `OSPREY_KLEE` selects the engine binary and `OSPREY_KLEE_RUNTIME` selects the runtime bitcode.

Hunt starts at the program entry point; typed function harnesses and automatic regression-test generation are planned. Fibers and resumable control effects use threads that this symbolic model cannot follow. A dynamic `control` handler can stop exploration at the region entry (`__osprey_coro_new`), before the input-processing code runs. Use static handlers in a hunt driver where applicable; an unmodelled region has not been checked. Native replay has a 30-second timeout and skips runs that do not exit. Floating-point execution is concretized by KLEE, so numerical code still needs explicit edge cases and oracle comparisons.

See the [symbolic execution contract](/spec/symbolicexecution/) for the runtime models, replay details and delivery status. Next, run a short hunt against a small input-processing project and turn any reproduced finding into a test.
