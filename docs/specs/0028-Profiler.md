# 0028 — CPU Profiler

Osprey ships a state-of-the-art sampling CPU profiler built into the runtime.
It is wall-clock based, fiber-aware, bias-hardened, and needs no elevated
permissions, no `perf`, no `dtrace`, and no attach step: the sampler lives in
the same process as the program.

Research grounding: random-offset sampling and yield-point-bias avoidance
(Mytkowicz et al., PLDI 2010; McCanne & Torek, USENIX 1993), full-stack
attribution instead of gprof arc propagation (Graham/Kessler/McKusick 1982's
documented flaws), frame-pointer unwinding at fleet scale (Gregg), Go's
per-thread timer + profBuf architecture, and samply's Mach suspend sampler.

## [PROF-ACTIVATE-ENV] Activation

The profiler is compiled into every runtime archive and is **off by default**.
It activates only when the environment variable `OSPREY_PROFILE=<path>` is set
at process start; the raw profile is written to `<path>` at normal exit.
`OSPREY_PROFILE_HZ=<n>` overrides the sampling rate (clamped to 10..10000,
default 997 — a non-round rate so sampling never phase-locks with 100/1000 Hz
periodic activity). Because activation is environment-only, any Osprey binary —
including production `--compile` output — can be profiled after the fact.
The differential golden harness never sets the variable, so tested example
output is untouched.

## [PROF-COLLECT-SAMPLER] Collection

A dedicated sampler thread wakes on a jittered interval (uniform ±30% around
the nominal period, re-drawn every tick, per McCanne-Torek) and samples every
registered thread. Osprey fibers are 1:1 pthreads, so per-fiber attribution is
exact: each thread registers once at start with its fiber id and label.

- **macOS**: `thread_suspend` → `thread_get_state(ARM_THREAD_STATE64)` →
  frame-pointer walk → `thread_resume` (the samply/Instruments model). No
  signals are delivered, so blocking syscalls are never EINTR-perturbed. While
  any thread is suspended the sampler performs no allocation; recording (which
  allocates) happens only after the resume.
- **Linux**: the sampler thread directs `SIGPROF` at running threads via
  `pthread_kill`; the async-signal-safe handler walks its own stack from
  `ucontext_t` into a preallocated per-thread SPSC ring drained by the sampler.
  A blocked thread gets one signal to capture its blocking stack; further
  waiting samples reuse it without re-perturbing the syscall (and `sleep`
  waits on an absolute deadline, so an interrupted sleep never shortens).
  Never `setitimer(ITIMER_PROF)` — process-wide delivery is throttled and
  under-reports beyond ~2.5 cores (golang/go#35057).

Samples record `(t_ns, thread, stack, state)` where state classifies the
thread as on-CPU or waiting from per-thread CPU-time deltas
(`thread_info(THREAD_BASIC_INFO)` on Mach — `CLOCK_THREAD_CPUTIME_ID`
under-reports on Apple Silicon; the thread CPU clock on Linux). One profile
therefore carries both the CPU picture and the off-CPU/blocked picture.

## [PROF-COLLECT-UNWIND] Stack capture

Frame-pointer chain walk only — never DWARF/libunwind in async context. Every
frame pointer is validated before dereference: 8-byte alignment, strict
monotonic growth, inside the thread's `[lo, hi)` stack bounds captured at
registration, bounded frame size, depth cap 128. Any failed check ends the
walk. Frame 0 is the precise pc; on arm64 the leaf's caller may live only in
`lr`, which is recorded and deduplicated against the first chained return
address. PAC bits are stripped from arm64 code addresses.

## [PROF-COLLECT-REGISTRY] Thread registry

`osp_prof_thread_register(fiber_id, label)` / `osp_prof_thread_unregister()`
are no-ops when the profiler is inactive. Call sites: the main thread (label
`main`, fiber 0), `fiber_thread_func` (label `fiber`), and effect continuation
threads (label `effect`, fiber −1). Slots carry a generation counter: the
sampler snapshots `(slot, gen)` pairs, then performs each per-slot sample
under the registry mutex after revalidating `active && gen` — unregister takes
the same mutex and always precedes the completion signal that gates
`pthread_join`, so a validated slot's thread is live for the whole sample (a
joined `pthread_t` or recycled mach port is never touched). Lock order is
registry → malloc everywhere, and nothing allocates while a thread is
suspended.

## [PROF-RAW-FORMAT] Raw profile file

JSON, written once at exit by the runtime (no symbolization in-process —
symbol names, files, and lines are resolved offline):

```json
{"version":1, "pid":0, "exe":"/path/bin", "rate_hz":997, "platform":"macos-arm64",
 "start_unix_ns":0, "end_unix_ns":0,
 "images":[{"path":"/path/bin","base":0,"slide":0}],
 "threads":[{"fiber":0,"label":"main"}],
 "stacks":[[4301231,4301100]],
 "samples":[[12345,0,0,0]]}
```

`stacks` are leaf-first raw return addresses. `samples` rows are
`[t_rel_ns, thread_index, stack_index, state]` with state `0` = on-CPU,
`1` = waiting. A single top-level `dropped` counter reports the samples the
runtime dropped (ring overflow).

## [PROF-SYMBOLIZE-OFFLINE] Symbolization

The `osprey-profiler` crate maps each pc to its image, computes the unslid
address, and batch-symbolizes via `llvm-symbolizer` (file/line/column plus
inline expansion), falling back to `atos` on macOS and raw hex names when no
symbolizer is present. Return addresses (every frame except the leaf) are
adjusted by −1 so samples attribute to the call line, not the next line.
Osprey symbols are unmangled, so names map 1:1 to source functions.

## [PROF-BUILD-MODE] `--profile` builds

`--profile` compiles with debug metadata (DWARF line tables) at **full
optimization** — unlike `--debug`, which forces `-O0`. Driver flags:
`-g -fno-omit-frame-pointer` with the release `-O2`. On macOS the pipeline is
`.ll → .o → link → dsymutil` (single-step `clang foo.ll -o foo` deletes the
temp object that holds the DWARF, making line info unrecoverable).
Additionally, codegen emits `"frame-pointer"="all"` on **every** generated
function unconditionally [PROF-CODEGEN-FP]: Darwin's default keeps frame
pointers only on non-leaf functions, which corrupts leaf-frame walks.

## [PROF-CLI-RUN] CLI pipeline

`osprey <file> --run --profile` (`--profile` implies `--run` when no other
mode is given): compile with [PROF-BUILD-MODE], execute with `OSPREY_PROFILE`
pointing at a scratch raw file, then post-process:

1. Write `<stem>.speedscope.json` — primary export, one sampled profile per
   fiber sharing an interned frame table (root-first sample stacks).
2. Write `<stem>.cpuprofile` — V8 format (µs timeDeltas, node call tree,
   0-based lines); opens natively in VS Code's built-in profile viewer.
3. Write `<stem>.folded` — Brendan Gregg collapsed stacks with the fiber as a
   synthetic root frame (`fiber-1;main;fib`), feeding inferno/flamelens/diff.
4. Write `<stem>.profile.json` — summary for editor integration: totals,
   per-fiber state split, hot functions (self/total), hot lines.
5. Print the terminal report [PROF-CLI-REPORT].

## [PROF-CLI-REPORT] Terminal report

Printed after the program exits (never in the harness path — only under
`--profile`): a header line (wall, CPU, samples, rate, fibers), a fiber-state
split (running / waiting), and a top-10 table with columns
`SELF% TOTAL% SELF TOTAL FUNCTION LOCATION`, Unicode eighth-block bars in the
self gutter, and perf-style color thresholds (≥5% red/bold, ≥0.5% yellow),
honoring `NO_COLOR` and non-TTY stdout. **No calls column** — a sampling
profiler cannot honestly report call counts. Sample counts are shown so users
can apply the √n error rule; below ~100 samples the report flags low
confidence.

## [PROF-VSCODE-FLAME] Editor integration

The VS Code extension gains a `Osprey: Profile Current File` command that runs
the CLI pipeline and renders an interactive **canvas flame graph** webview
(zoom, pan, hover tooltips, substring search with match dimming, click-to-
source), with Left Heavy and Time Order views, a per-fiber filter, and a
self/total hot-function table — plus after-line heat decorations
(`NN.N% · M samples`) with overview-ruler marks [PROF-VSCODE-HEAT], driven by
`<stem>.profile.json`. The `.cpuprofile` export doubles as a fallback viewer.

## [PROF-TEST] Testing

- C unit test (`profiler_runtime_tests.c`): registers threads, runs a busy
  loop under a high-rate sampler, asserts samples were captured, stacks are
  non-empty and bounds-valid, and the raw JSON parses.
- Rust: `osprey-profiler` unit tests cover raw parsing, aggregation
  (self/total, hot lines, fiber split), every exporter's schema shape, and the
  report formatter; the symbolizer is a trait so transforms are tested pure.
- `make test` end-to-end: run one tested example under `--profile` and assert
  the exports exist and parse; the differential harness runs with the profiler
  off and must stay byte-identical.
