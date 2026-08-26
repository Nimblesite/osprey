# CPU Profiler

## [PROF-ACTIVATE-ENV] Activation

The profiler is compiled into every runtime archive and is off by default.
It activates only when the environment variable `OSPREY_PROFILE=<path>` is set
at process start; the raw profile is written to `<path>` at normal exit.
`OSPREY_PROFILE_HZ=<n>` overrides the sampling rate (clamped to 10..10000,
default 997). Native POSIX binaries can be profiled without recompilation.
Windows and Wasm builds carry inactive profiler stubs.

## [PROF-COLLECT-SAMPLER] Collection

A sampler thread wakes on a per-tick interval jittered uniformly by ±30% and
samples every registered thread. Each native fiber thread registers its fiber
id and label.

- **macOS**: `thread_suspend` → `thread_get_state(ARM_THREAD_STATE64)` →
  frame-pointer walk → `thread_resume`. The sampler performs no allocation
  while a thread is suspended.
- **Linux**: the sampler thread directs `SIGPROF` at running threads via
  `pthread_kill`; the async-signal-safe handler walks its own stack from
  `ucontext_t` into a preallocated per-thread SPSC ring drained by the sampler.
  A blocked thread gets one signal to capture its blocking stack; further
  waiting samples reuse that stack. Sleep uses an absolute deadline.

Samples record `(t_ns, thread, stack, state)`. State is on-CPU or waiting,
derived from per-thread CPU-time deltas using `THREAD_BASIC_INFO` on macOS and
the thread CPU clock on Linux.

## [PROF-COLLECT-UNWIND] Stack capture

Stack capture uses only a frame-pointer chain walk. Every
frame pointer is validated before dereference: 8-byte alignment, strict
monotonic growth, inside the thread's `[lo, hi)` stack bounds captured at
registration, bounded frame size, depth cap 128. Any failed check ends the
walk. Frame 0 is the precise PC. On arm64, `lr` is recorded and deduplicated
against the first chained return address, and PAC bits are stripped.

## [PROF-COLLECT-REGISTRY] Thread registry

`osp_prof_thread_register(fiber_id, label)` / `osp_prof_thread_unregister()`
are no-ops when the profiler is inactive. Call sites: the main thread (label
`main`, fiber 0), `fiber_thread_func` (label `fiber`), and effect continuation
threads (label `effect`, fiber −1). Registry slots carry a generation counter.
The sampler revalidates `active && generation` under the registry mutex before
sampling; unregister uses the same mutex before signaling completion. Lock
order is registry then allocator, and sampling allocates only after a suspended
thread resumes.

## [PROF-RAW-FORMAT] Raw profile file

JSON, written once at exit by the runtime (no symbolization in-process —
symbol names, files, and lines are resolved offline):

```json
{"version":1, "pid":0, "exe":"/path/bin", "rate_hz":997, "platform":"macos-arm64",
 "start_unix_ns":0, "end_unix_ns":0,
 "images":[{"path":"/path/bin","base":0,"slide":0}],
 "threads":[{"fiber":0,"label":"main"}],
 "stacks":[[4301231,4301100]],
 "samples":[[12345,0,0,0]],
 "dropped":0}
```

`stacks` are leaf-first raw return addresses. `samples` rows are
`[t_rel_ns, thread_index, stack_index, state]` with state `0` = on-CPU,
`1` = waiting. A single top-level `dropped` counter reports samples the runtime
could not retain (sampler-ring overflow or allocation/interning failure).

## [PROF-SYMBOLIZE-OFFLINE] Symbolization

The `osprey-profiler` crate maps each pc to its image, computes the unslid
address, and batch-symbolizes via `llvm-symbolizer` (file/line plus inline
expansion), falling back to `atos` on macOS and raw hex names when no
symbolizer is present. Return addresses (every frame except the leaf) are
adjusted by −1 so samples attribute to the call line, not the next line.
Osprey symbols are unmangled, so names map 1:1 to source functions.

## [PROF-BUILD-MODE] `--profile` builds

`--profile` compiles with debug metadata (DWARF line tables) at **full
optimization** — unlike `--debug`, which forces `-O0`. Driver flags:
`-g -fno-omit-frame-pointer` with the release `-O2`. On macOS the pipeline is
`.ll → .o → link → dsymutil`. Codegen emits `"frame-pointer"="all"` on every
generated function [PROF-CODEGEN-FP].

## [PROF-CLI-RUN] CLI pipeline

`osprey <file> --run --profile` (`--profile` implies `--run` when no other
mode is given): compile with [PROF-BUILD-MODE], execute with `OSPREY_PROFILE`
pointing at a scratch raw file, then post-process:

1. Write `<stem>.speedscope.json` — one sampled profile per fiber with samples,
   sharing an interned frame table and root-first stacks.
2. Write `<stem>.cpuprofile` — V8 format with microsecond deltas, a node call
   tree, and 0-based lines.
3. Write `<stem>.folded` — collapsed stacks with the fiber as a synthetic root
   frame (`fiber-1;main;fib`).
4. Write `<stem>.profile.json` — summary for editor integration: totals,
   per-fiber state split, hot functions (self/total), hot lines.
5. Print the terminal report [PROF-CLI-REPORT].

## [PROF-CLI-REPORT] Terminal report

Printed after the program exits (never in the harness path — only under
`--profile`): a header line (wall, CPU, samples, rate, fibers), a fiber-state
split (running / waiting), and a top-10 table with columns
`SELF% TOTAL% SELF TOTAL FUNCTION LOCATION`, Unicode eighth-block bars in the
self gutter, and color thresholds (≥5% red/bold, ≥0.5% yellow),
honoring `NO_COLOR` and non-TTY stdout. Sampling does not produce call counts,
so the report has no calls column. Below about 100 samples, the report flags low
confidence.

## [PROF-VSCODE-FLAME] Editor integration

The VS Code extension provides an `Osprey: Profile Current File` command that
runs the CLI pipeline and renders a canvas flame graph webview
(zoom, pan, hover tooltips, substring search with match dimming, click-to-
source), with Left Heavy and Time Order views, a per-fiber filter, and a
self/total hot-function table. It also adds after-line heat decorations
(`NN.N% · M samples`) with overview-ruler marks [PROF-VSCODE-HEAT], driven by
`<stem>.profile.json`.

Frame colors are a function of the profile alone, never of the engine. Frames
are ranked by their file path, then their name, then their original index, and
each rank picks one colour from a fixed ramp; a frame's colour therefore depends
only on the document, and the same profile looks the same on every run and every
machine. The index term carries the whole weight of that promise for frames that
share a file AND a name — the same function inlined at two sites, or two runtime
frames with no file at all. An ordering that answered "equal" for those would
make their ranks a property of the ARRAY, not of the frames: a host sort is
required to be stable, so the ranks would come out in whatever order the
document happened to list them, and a profile that named the same two frames in
the other order would colour them the other way round. The ordering is therefore
a TOTAL one — it reports two distinct frames as equal only when they are the
same frame — and that is a property of the comparator, which no observation of
a sort's output can confirm, so it is asserted on the comparator directly.

Ranking by file THEN name is ranking on the pair, not on a string that happens
to contain both: any separator used to splice them must be one no path and no
frame name can contain, or `("/w/a b.osp", "z")` and `("/w/a", "b.osp z")`
collapse onto one key and two distinct frames become indistinguishable.

## [PROF-TEST] Testing

- The C runtime suite verifies thread registration, sample capture, stack
  bounds, and raw JSON output.
- `osprey-profiler` tests verify parsing, aggregation, symbolization,
  exporters, and terminal formatting.
- The end-to-end profiler script runs an example under `--profile` and parses
  every export. The ordinary example harness runs with profiling disabled.
