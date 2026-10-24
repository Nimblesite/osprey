---
layout: page.njk
title: "Automated Testing with Osprey Hunt: Find the Input That Crashes"
excerpt: "Give Osprey Hunt a program and an input bound. It explores branches with KLEE, records a failing input, and checks whether a native build reproduces the crash."
description: "Use symbolic execution for automated testing in Osprey: reproduce a crash, read HTML and JSON reports, and understand where Hunt fits beside static analysis and fuzzing."
date: 2026-10-25
tags: ["blog", "testing", "symbolic-execution", "fuzzing", "klee"]
author: "Christian Findlay"
readingTime: 5
---

One byte was enough. Osprey Hunt explored a program, found the input `"9"`, and reproduced its crash in a native executable:

```text
findings: 1
  abort      crash.osp:4
             abort failure
             stdin    "9"
             replay   reproduced (SIGABRT)
```

This is a deliberately planted failure in [`tests/hunt/crash.osp`](https://github.com/Nimblesite/osprey/blob/main/tests/hunt/crash.osp). The useful part is the workflow: Hunt produces an input you can run again, a source location, and a separate native reproduction verdict. You can turn that evidence into a regression test.

## Run the crash demo

From the Osprey repository root, install the engine and build the symbolic runtime:

```sh
brew install klee
make build
./target/release/osprey hunt tests/hunt/crash.osp --input-bytes 1 --max-time 10
```

The command is `osprey hunt`. The demo's exit status is **1**, because the failure reproduced. Its source is small enough to inspect completely:

```osprey
/// Deliberately planted input-triggered crash for Hunt.
extern fn abort() -> int
let line = input()
let result = if line == "9" { abort() } else { 0 }
print("result ${result}")
```

The C call makes this a crash demonstration, not an example of ordinary Osprey error handling. C interoperability crosses Osprey's memory-safety boundary. In application code, an expected failure belongs in a `Result` or a declared effect; callers and handlers decide what to do with it.

## What symbolic execution adds to automated testing

A unit test supplies a concrete input and checks the expected result. Hunt gives supported inputs symbolic values, then uses KLEE to explore the program's branches. When a path reaches a failure, the engine records concrete bytes that take that path.

Osprey compiles the program to LLVM IR linked with its symbolic runtime models. Those models cover stdin, file reads, entropy and clock values. Findings are mapped back to Osprey source locations, including files inside a project.

Hunt then builds an ordinary native executable and replays the recorded stdin and files. A `reproduced` verdict means that executable also terminated from a signal. Native replay does not currently restore symbolic entropy or clock values, so a finding that depends on either needs further investigation.

This complements tests with expected outputs. Finding a crashing input does not tell you the correct business result, and exploring without a finding does not prove the program correct.

## Open the HTML report

Every normal run saves `report.html` and `report.json` beneath `target/hunt/` and prints their paths. The HTML report shows the result, source excerpts, input and replay verdict. It works offline without JavaScript.

The **For AI** section includes source context, engine arguments and raw diagnostics. It gives a developer or coding assistant concrete evidence to investigate. Reports contain local paths and source text, so review the contents before sharing one.

For automation, add `--json --quiet` to emit the JSON report on stdout. Saved reports remain enabled. Use `--out` when you also want to retain the engine's raw artifacts in a chosen directory.

Read the verdict before deciding what to fix:

| Result | What it establishes |
| --- | --- |
| `reproduced` | The native executable also crashed. Preserve the input and add a regression test. |
| `klee-only` | The native replay did not reproduce the crash. Check the model and any input that replay does not restore. |
| `unmodelled` | The path reached an unsupported operation. Code beyond that boundary was not explored. |
| `incomplete` | A budget or engine limit stopped analysis. This is unfinished work, not a discovered program defect. |

With replay enabled, unsupported operations and incomplete paths can coexist with exit status **0**. A CI policy that requires complete exploration must inspect the JSON report as well as the exit code.

## How does this differ from fuzz testing?

Fuzz testing runs programs with generated or mutated concrete inputs. Symbolic execution uses constraints from program branches to derive inputs. Both can expose failures that a hand-written test suite misses; their costs and coverage limits differ.

Static analysis checks properties without running the program on concrete inputs. Osprey's type checker already rejects unhandled effect operations. Hunt answers a different question: which supported execution paths can reach a failure, and can the resulting input reproduce it?

There is no comparative speed or coverage claim here. Hunt uses KLEE and has specific runtime models; a fuzzing campaign, compiler checks and tests with expected results each contribute different evidence.

## Keep the input bound small

The clean control parses input and safely selects a list element:

```sh
./target/release/osprey hunt tests/hunt/clean.osp --input-bytes 2 --max-time 30
```

It expects no findings. More symbolic bytes can create more branches, so increase the bound deliberately and check whether paths finish within the budget.

The current workflow supports macOS and Linux x86_64. Fibers and dynamic `control` handlers reach operations the engine cannot model; a handler region can stop before the code inside it is explored. Floating-point execution is concretized, so numerical code still needs explicit edge cases and reference comparisons. Typed function harnesses and automatic regression-test generation remain planned.

Run the [crash demo](/docs/hunt/) first, inspect its report, then try a small input-processing program from your own project. Keep every reproduced input as a test before changing the code.
