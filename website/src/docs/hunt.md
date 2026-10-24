---
layout: page
title: Hunt for bugs
description: Explore Osprey programs with symbolic inputs, inspect KLEE findings and replay them on a native build.
permalink: /docs/hunt/
tags: [testing, hunt, symbolic-execution]
---

`osprey hunt` explores your program with symbolic inputs and reports inputs that reach a failure. It uses KLEE to follow branches through stdin, file reads, entropy and clock values, then replays findings on a native build. You get source locations and concrete inputs to investigate.

The command is `osprey hunt`, with no leading dash. It accepts a Default or ML source file, a project directory or an `osprey.toml` manifest. A project can include Python-flavor sources through its manifest.

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

A memory-safety failure in generated Osprey code points to a compiler, runtime or symbolic-model defect. Keep the reported source location and input when reporting it.

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

This inventories runnable programs across tests, examples, book examples, package apps, registry apps and benchmarks. Projects are counted once; their individual modules and libraries without an entry point are excluded. Default and ML files and Python-entry projects are included. Known engine limitations are sampled rather than silently skipped.

Each invocation gets a hard **10-second wall-clock deadline**, including compilation and replay. The sampler kills the whole process group on timeout. Its default engine budget is eight seconds with two symbolic bytes per stream. The ordinary Hunt `--max-time` flag limits the engine; it is not this whole-command deadline.

Open the generated `index.html`. `quick-corpus.txt` lists only programs that exited successfully within ten seconds, explored at least one path, finished every explored path normally, and had no findings or unfinished paths. Logs, exact commands, source context and binary fingerprints accompany the results. Treat this list as candidate tests under those bounds, not a proof of general safety or of identical timing on CI. Use a fresh output directory for each sweep.

## Verify deliberate crash detection

```sh
make hunt-crashes
```

The gate covers fifteen planted input cases: stdin, parsing, checked arithmetic, lists, maps, closures, recursion, unions, record invariants, JSON, file input, invalid C free, C double free, ML flavor and Python flavor. Each runs independently within ten seconds. Every expected class, source location, concrete input, file payload and native signal must match. Missing findings, unfinished analysis, tool errors, skipped replay and KLEE-only findings fail the gate.

These tests deliberately violate application invariants or the unsafe C boundary. They demonstrate detection of known failures; they are not new compiler defects. CI runs the real KLEE/native-replay gate in the existing required macOS coverage job and the mock/runtime tooling tests in the required C-runtime job. Reports are uploaded as `hunt-crash-reports`. A missing engine fails the crash gate rather than skipping it.

## Use it in VSCode

Open an `.osp` or `.ospml` file and select **Osprey: Hunt for Bugs** from the command palette or editor context menu. The extension saves the file, runs the compiler selected by the language server, and shows findings in the **Osprey Hunt** output channel and editor diagnostics. Set `osprey.hunt.maxTime` to change its time budget.

A reproduced finding is an error, an unreplayed or KLEE-only finding is a warning, and an unmodelled operation is information.

## Current boundaries

KLEE runs natively on macOS and Linux x86_64. Linux arm64 and Windows are not supported by the current workflow. If the engine or runtime bitcode is missing, Hunt exits with a setup error. `OSPREY_KLEE` selects the engine binary and `OSPREY_KLEE_RUNTIME` selects the runtime bitcode.

Hunt starts at the program entry point; typed function harnesses and automatic regression-test generation are planned. Fibers and resumable control effects use threads that this symbolic model cannot follow. A dynamic `control` handler can stop exploration at the region entry (`__osprey_coro_new`), before the input-processing code runs. Use static handlers in a hunt driver where applicable; an unmodelled region has not been checked. Native replay has a 30-second timeout and skips runs that do not exit. Floating-point execution is concretized by KLEE, so numerical code still needs explicit edge cases and oracle comparisons.

See the [symbolic execution contract](/spec/symbolicexecution/) for the runtime models, replay details and delivery status. Next, run a short hunt against a small input-processing project and turn any reproduced finding into a test.
