# Proof speed gate

`run.sh` is the executable form of [PROOF-SPEED] ([spec 0049](../../docs/specs/language/0049-DependentTypesAndProofs.md)): for every case it times Osprey's kernel and Lean's on the same machine in the same run, and it is red unless Osprey's minimum wall time is below Lean's. It runs in CI as the `test-proof` job, which invokes it as `zsh benchmarks/proof/run.sh`.

```sh
PATH=$HOME/.elan/bin:$PATH PROOF_RUNS=3 zsh benchmarks/proof/run.sh   # all cases
PATH=$HOME/.elan/bin:$PATH zsh benchmarks/proof/run.sh m0_identity   # cases whose name matches
```

Exit codes: `0` every case passed, `1` a case failed (slower than Lean, or a timed run failed), `2` a prerequisite is missing (no `lean`, no `python3`, no `target/release/osprey`).

## What is measured, and what is not

Each run records both sides' minimum wall time over `PROOF_RUNS` runs (default 10) and peak resident set, plus the sha256 of both inputs and both binaries, so a number can be traced to the exact sources and executables that produced it.

Honesty about M0: the current case times a compiled Osprey program whose identity theorem is written into `examples/proof/kernel/src/main.ospml`, against `lean --run id_imp.lean`, which includes Lean's startup and elaboration as well as its kernel. That is the milestone's regression gate, not the comparison [PROOF-SPEED] specifies: the specification compares both kernels re-checking the *same* `lean4export` file, and that case (M1) lands with `benchmarks/proof/tools/`. Until it does, do not cite these numbers as kernel-speed evidence; the citable record is the dated trust report of [PROOF-TRUST-REPORT].

## A case is one directory

`benchmarks/proof/<case>/` holds:

| path | meaning |
| --- | --- |
| `osprey.toml`, or `kernel` symlink to a project that has one | the Osprey program whose run checks the theorem |
| the case's single `*.lean` (named after the case by convention) | the Lean module `lean --run` elaborates |
| `lean.cmd` (optional) | a shell command line, run from the case directory, that replaces `lean --run <module>` as the Lean side; a case with `lean.cmd` needs no `*.lean` |
| `args` (optional) | extra arguments appended to the Osprey binary's command line, split on whitespace |

A directory with no Osprey project is not a case: `reports/` holds the generated reports, and `tools/` is a Lake project whose `lakefile.toml` marks it as tooling, so both are skipped. A directory with a Lean side but no Osprey project is a broken case and is named as one.

Before anything is timed, both sides are run once and must exit zero and print a line starting `checked: `. A case whose kernel never checks anything is red, not fast.

## Failures are red, never a number

Every timed invocation's exit status is checked. A run that exits non-zero prints why, fails its case, and records no timing at all, so a crashing or failing run can never contribute a winning number. A missing measurement is treated the same way: wall time is ceiled to whole milliseconds and an empty or zero wall-time or peak-memory reading is an error.

Peak memory is measured portably by an inline Python 3 helper (`MEASURE_PY` in `run.sh`) that spawns the command with `subprocess` and reads `resource.getrusage(RUSAGE_CHILDREN).ru_maxrss`. That field is bytes on macOS and kilobytes on Linux, so the helper normalises it to KiB by `sys.platform`; `/usr/bin/time -l` is macOS-only and is gone.

## Hashes

Every results line carries:

- `osprey_input`: sha256 over every `*.ospml` under the case's Osprey project, files sorted and concatenated as `path\0bytes` with paths relative to the project root — the kernel modules that determine the result, not just the entry file;
- `lean_input`: sha256 of the case's Lean module, or of `lean.cmd` when that replaces it;
- `osprey_binary`: sha256 of the timed binary (`.bench-osprey`, compiled into the case directory and removed afterwards);
- `lean_binary`: sha256 of the executable actually timed. `~/.elan/bin/lean` is elan's multiplexer, so the script resolves the toolchain's own binary through `elan which lean` (falling back to `lean --print-prefix`) and hashes that.

Hashing is done in Python rather than with `shasum`/`sha256sum`, whose names differ between macOS and Ubuntu.

## Outputs

- `target/proof/results.jsonl` — one JSON line per case per run, including `at` (UTC ISO timestamp) and the Lean version.
- `target/proof/reports/<YYYY-MM-DD>.md` — the human-readable report for that UTC day: a header naming the Lean and Osprey binaries with their hashes and the run count, then one row per case (case, osprey ms, lean ms, peak KiB each, verdict, all four hashes). A later run on the same day appends rows to the existing file.

Both are generated evidence written under `target/`, outside the source tree. They are never committed: the numbers a claim cites belong in the dated trust report.

## Platforms and toolchain

macOS and Ubuntu both work, and CI runs the Ubuntu path. The script is `zsh` — CI calls it as `zsh benchmarks/proof/run.sh`, and `ubuntu-latest` runners ship `zsh` preinstalled — and it needs `python3` (present on macOS and on the Ubuntu runners) and Lean from [elan](https://github.com/leanprover/elan).

CI pins Lean to `leanprover/lean4:v4.25.0` so the baseline does not float mid-comparison. That pin is bumped **by hand on each Lean release**, and the benchmark is re-run and re-recorded at the same time; a moved baseline makes the comparison noise. Nothing in the script hides the version: it prints the measured Lean version in the report header and in every results line, alongside the hash of the Lean binary that produced the timing.
