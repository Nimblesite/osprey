# CPU Profiler

## [PROF-ACTIVATE-ENV] Activation

The profiler is compiled into every runtime archive and is off by default.
It activates only when the environment variable `OSPREY_PROFILE=<path>` is set
at process start; the raw profile is written to `<path>` at normal exit.
`OSPREY_PROFILE_HZ=<n>` overrides the sampling rate (clamped to 10..10000,
default 997). Native POSIX binaries can be profiled without recompilation.
Windows and Wasm builds carry inactive profiler stubs. A running program can
also be profiled without either variable, through the live-attach endpoint
[PROF-ATTACH-ENDPOINT]; at most one session, of either kind, runs at a time.

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

The walk only works through frames that keep a frame pointer, so every unit
of the C runtime is compiled with `-fno-omit-frame-pointer`
(`scripts/make/config.mk`, `FRAME_POINTERS`). Osprey code already pins
`"frame-pointer"="all"` in its IR. Without the runtime half, a sample taken
inside the collector or a string allocation has no Osprey ancestor: on x86-64
Linux, where the C compiler drops the pointer at `-O2`, such a stack keeps only
runtime frames, is reported as native-only, and the Osprey caller's share
shrinks by exactly the time it spent in the runtime.

## [PROF-COLLECT-REGISTRY] Thread registry

`osp_prof_thread_register(fiber_id, label)` / `osp_prof_thread_unregister()`
are no-ops until a session or the live-attach endpoint enables registration;
from then on every thread registers for the rest of the process, so a later
session never samples a thread that already exited. Call sites: the main thread (label
`main`, fiber 0), `fiber_thread_func` (label `fiber`), and effect continuation
threads (label `effect`, fiber −1). A session numbers the registered threads
when it starts, and each thread that registers during it, as its `threads`
rows; between sessions no rows are kept. Registry slots carry a generation counter.
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
 "images":[{"path":"/path/bin","base":0,"slide":0,"text":0,"text_size":0,"arch":""}],
 "threads":[{"fiber":0,"label":"main"}],
 "stacks":[[4301231,4301100]],
 "samples":[[12345,0,0,0]],
 "dropped":0}
```

`stacks` are leaf-first raw return addresses. `samples` rows are
`[t_rel_ns, thread_index, stack_index, state]` with state `0` = on-CPU,
`1` = waiting. A single top-level `dropped` counter reports samples the runtime
could not retain (sampler-ring overflow or allocation/interning failure).

An `images` entry records both where the image was *mapped* and where its
*code* lives: `base` is the mach-header (ELF: load-base) address and `slide`
the relocation applied to it, while `text`/`text_size` are the start and byte
length of the image's executable range — `__TEXT` on Mach-O, the span of the
`PT_LOAD` segments carrying `PF_X` on ELF. The two are not interchangeable.
Inside the dyld shared cache every system dylib's mach header sits in one
region and its `__TEXT` in another, so header order is not text order, and a
`text_size` of `0` means the producer could not determine the range.

An entry also records `arch`: the mach-o slice the image was *mapped* as
(`arm64e`, `arm64`, `x86_64`), empty when the producer does not report one. It
is not cosmetic. A universal system dylib lays `__TEXT` out differently in each
slice, and on Apple Silicon the kernel maps the `arm64e` image out of the shared
cache even into a plain-`arm64` process, so a symbolizer left on its host
default reads the wrong layout and names every address after whichever function
occupies that offset in the other slice. Only the process that ran the image can
read its header, so only the producer can record this.

The loader itself is an image. `_dyld_image_count()` reports what dyld loaded,
not dyld, so `/usr/lib/dyld` is recorded separately; without it every frame of
process startup belongs to no image and can only be printed as a raw address.

## [PROF-ATTACH-ENDPOINT] Live-attach endpoint

Every native Osprey program on macOS and Linux opens a diagnostics endpoint when `main` starts, unless `OSPREY_DIAGNOSTICS=0`; iOS, Android, Windows and Wasm builds have none. The endpoint is a Unix-domain socket at `$TMPDIR/osprey-diagnostics-<uid>/<pid>.sock` (`/tmp` when `TMPDIR` is unset), so a language server can find every attachable Osprey process by listing one directory, the way a .NET tool finds `dotnet-diagnostic-<pid>` endpoints. The directory is created with mode `0700`, and an existing directory that is not a real directory owned by the user with no group or other permission bits disables the endpoint rather than being repaired. A process removes its socket at normal exit and, when no other socket remains, the directory with it, so a program leaves a scratch `TMPDIR` as it found it; a process whose freshly created directory was removed by a neighbour exiting just then recreates it once before binding. Each start sweeps sockets in the directory whose process no longer exists, so a crash or a `SIGKILL` leaves nothing behind for long. A forked child has no endpoint.

One listener thread, created with every signal blocked, serves up to eight connections; a ninth gets a `busy` error and is closed. The endpoint never allocates through the Osprey allocator and never touches the managed heap, so it changes no memory backend's behaviour. With the endpoint open, threads register with the profiler as [PROF-COLLECT-REGISTRY] describes, but no sampler runs and no samples or rows are kept until a client starts a session.

### [PROF-ATTACH-PROTOCOL] Wire protocol

A request is one text line (a trailing `\r` is ignored, and a line is at most 255 bytes). Every reply is one JSON object per line with a `kind` field; strings escape every control character, so a value can never break the framing.

| Request | Reply |
|---|---|
| `hello` | `{"kind":"hello","protocol":1,"pid":…,"platform":"macos-arm64","cpu_session":false,"runtime_version":"…","exe":"…"}` |
| `cpu_start <hz>` | `{"kind":"cpu_started","rate_hz":…,"start_unix_ns":…,"images":[…]}`, then `cpu_batch` lines |
| `cpu_stop` | the last `cpu_batch`, then `{"kind":"cpu_stopped","reason":"requested","dropped":…,"end_unix_ns":…}` |

`<hz>` is clamped to 10..10000 like `OSPREY_PROFILE_HZ`. A `cpu_batch` is `{"kind":"cpu_batch","threads":[…],"stacks":[…],"samples":[…]}` with the records of [PROF-RAW-FORMAT]: `threads` and `stacks` are appended to the session's tables in order, so the indices of one batch continue from the last, and `samples` rows reference those tables. A stream row carries a fifth column, `[t_rel_ns, thread_index, stack_index, state, tick]`: the number of the sampler tick that recorded it, never decreasing. The rows of one tick are one sampling instant across every thread, which is the unit a profiler's per-tick shares are counted in. On Linux a running thread's stack is captured by the signal the previous tick sent, so its rows land one tick late. A batch carries an `images` array as well whenever the program's image list changed since `cpu_started`. Batches are sent at most every 100 ms by the sampler thread itself, which owns the sample store; each batch's samples then leave the store, so a long session does not grow without bound.

One CPU session runs per process. `cpu_start` while one runs, including an `OSPREY_PROFILE` session, is refused with `already_profiling`. Closing the connection that started a session ends it. Program exit during a session sends the final batch and `cpu_stopped` with reason `exit`. A connection that cannot take a line within one second is closed, which ends any session it owned instead of stalling the sampler. Errors are `{"kind":"error","code":…,"message":…}` with code `unknown_command`, `bad_argument`, `already_profiling`, `not_profiling`, `attach_failed`, `line_too_long` (the connection is then closed) or `busy`.

## [PROF-LSP-RUNTIME] Language-server runtime

`crates/lsp-profiler-osprey` is the Osprey runtime for lspkit's shared profiler, `lsp-profiler`, which comes from the `lspkit` submodule because it is not published; its build script generates the wire model with the typeDiagram CLI, so a build needs Node.js and `typediagram` on `PATH`. The runtime supplies only what is Osprey's: recognising a process, sampling it, and naming its frames. Sessions, hotspots, exports, the JSON-RPC surface and the editor UI are lspkit's, the same for every language.

A process is an Osprey program when its endpoint answers `hello` [PROF-ATTACH-ENDPOINT], and the reply's runtime version is what the process listing shows. A socket nobody serves, which is a stale file a crash left for a pid the OS has since reused, is not an Osprey program, while an endpoint that never answers marks the program unattachable. A CPU session is one live-attach session [PROF-ATTACH-PROTOCOL]. Each stack is symbolized once, as it arrives ([PROF-SYMBOLIZE-OFFLINE]); a changed image list restarts that cache. The rows of one sampler tick become one sampling tick. Without native frames a stack keeps only its Osprey source frames, so time in the C runtime or the system belongs to the Osprey function that called it; a stack with no source frame at all, from a program built without debug info, keeps every frame instead of vanishing. Osprey source lives only in the program's own image, so without native frames another image's addresses are symbolized only for such a sourceless stack; every other stack would drop them, and naming system libraries through `atos` on every batch let a live session fall seconds behind on a busy machine and lose its tail at stop. Source lines need a debug-info build: `--profile` together with `--compile` builds one without running it. Thread trampolines and the loader's entry point are scaffolding. A pid that names no process is `PROCESS_NOT_FOUND`, even while a socket a `SIGKILL` left behind still names it, and a live process without an endpoint is `UNSUPPORTED_RUNTIME`. Windows has no endpoint, so there the runtime recognises nothing and has no CPU backend.

### [PROF-LSP-HOST] Language-server host

`osprey lsp` mounts the profiler service with the Osprey runtime. `initialize` advertises it twice: `experimental.lspProfiler` names every mounted runtime with its capabilities, and `executeCommandProvider.commands` lists every profiler operation as a `workspace/executeCommand` named `osprey.lspProfiler.<area>.<action>`, the convention lspkit's editor UI drives. A command's `arguments` must be exactly one params object (or absent); anything else is refused with `INVALID_PARAMS` (-32602). Each profiler command is answered from a task of its own, so a session that takes seconds to stop never holds up editing, and a `$/cancelRequest` for one still in flight answers it with `RequestCancelled` (-32800). The service's notifications, such as CPU progress, go to the client as they happen; every session is stopped when the server exits. Exports land under `$TMPDIR/osprey-profiler`.

## [PROF-SYMBOLIZE-OFFLINE] Symbolization

The `osprey-profiler` crate maps each pc to the image whose executable range
`[text, text + text_size)` contains it, falling back to the greatest `base`
at or below the pc only for images that reported no range. It computes the
unslid address, and batch-symbolizes via `llvm-symbolizer` (file/line plus
inline expansion), falling back to `atos` on macOS and raw hex names when no
symbolizer is present. An address is only ever resolved against an object that
describes its own image: the CLI's binary substitutes for the main image under
a moved path and for nothing else, because a dyld-shared-cache dylib has no
file on disk and resolving its addresses against the profiled program's symbol
table yields a confidently wrong name. `atos` reads those images from the live
cache, so the recorded path is handed to it unchanged, and it is pinned to the
image's recorded `arch` rather than the host default; where no object can
describe the image, the raw hex address stands. Return addresses (every frame
except the leaf) are adjusted by −1 so samples attribute to the call line, not
the next line. Osprey symbols are unmangled, so names map 1:1 to source
functions.

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

### [PROF-VSCODE-SHARED] Shared profiler UI

The extension adapts native captures to `@nimblesite/lsp-profiler-ui`, shared with SharpLsp. Osprey keeps its compiler invocation, optimized builds, native sampler, offline symbolization, cancellation, test-explorer profile action, summary parsing and inline sample annotations. The shared package owns the canvas model, interactions, result tables, trace actions, palette and debugger selection bridge. No native export format changes.

`osprey.profiler.palette` overrides the olive/brown-green defaults. Source/runtime frame classification stays in Osprey; frame identities determine deterministic color ranks. The shared view retains per-fiber running/waiting counts and dropped samples. A variable clicked in the Osprey Debug tree, or selected through Watch/Variables' profiler action, can be inspected and added back to Watch. A resumed or terminated session invalidates that selection.

The extension also mounts the library's live profiler, served by `osprey lsp` [PROF-LSP-HOST]. A Processes view in the Osprey Debug container lists every running Osprey program; Profile CPU attaches a live session to one, and Run & Profile CPU (Current File) builds the current file, runs it under the Osprey debugger and profiles it from the PID lldb-dap reports. Live results land in the shared heat map and results panel. Osprey has no memory backend, so the extension contributes no memory command, menu, launch link or setting, and no row offers or suggests memory tracking [PROF-LSP-RUNTIME]. `vscode-extension/scripts/sync-profiler-contributions.mjs` writes the library's generated contributions into the manifest, and compiling fails when they drift. npm installs the library as a packed copy of the submodule's build (`install-links`), so the VSIX ships it and the extension shares the library's one copy of its signals.

Native heap-retention capture would need an Osprey memory backend, which does not exist. The common VSIX capture and webview contracts run against the real Osprey extension and compiler, the capture contract both on an offline `--profile` capture and on a live session, alongside SharpLsp's live profiler suites ported to real Osprey programs [PROF-TEST].

## [PROF-TEST] Testing

- The C runtime suite verifies thread registration, sample capture, stack
  bounds, and raw JSON output. Its endpoint suite drives the real socket inside
  a private `TMPDIR`: the opt-out, the directory checks and stale-socket sweep,
  every request and error, batch indices against the tables earlier batches
  built, one session per process, a reader that stopped reading, and an exit
  mid-session.
- `crates/osprey-cli/tests/live_profiler.rs` compiles a CPU-bound fixture with
  debug info, runs it as its own process, and drives lspkit's profiler service
  with the Osprey runtime exactly as the language server does: the listing,
  the hot function and its lines, native frames only on request, the error
  codes, a busy program, a duration, a mid-session snapshot, and a program
  killed mid-session [PROF-LSP-RUNTIME].
- `osprey-profiler` tests verify parsing, aggregation, symbolization,
  exporters, and terminal formatting.
- The end-to-end profiler script runs an example under `--profile` and parses
  every export. The ordinary example harness runs with profiling disabled.
