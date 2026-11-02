# Test runner and editor integration

The corresponding TestingFramework contract is [here](../language/0027-TestingFramework.md).

## CLI

### `osprey test [path] [--filter <name>] [--quiet] [--coverage] [--coverage-json <path>] [--memory=gc|arc|none]` — `[TESTING-CLI-RUN]`

Runs test files and aggregates results. `path` (default `.`) is a single file
(run as-is, regardless of naming), a project directory (see
`[TESTING-PROJECT-SUITE]`), or a directory searched recursively for
`[TESTING-FILE-CONVENTION]` files in sorted order. Hidden, `target`, and
`node_modules` directories are skipped; symlinks are not followed. Each file
runs like `osprey <file> --run --quiet --no-warnings`, with its TAP output under a
`# file: <path>` header. A suite is executed, not reviewed: the warning pass
re-infers the whole program once per annotation site, which on a large
project costs minutes per suite, and its output never reaches a suite's
oracle. `osprey <path> --check` is where warnings are read. `--filter` sets `OSPREY_TEST_FILTER` for child
processes. The runner prints `# suites: X passed, Y failed` and exits `1` if a
suite fails to compile, fails to run, or skips a case without naming a reason
(`[TESTING-SKIP-REASON]`), otherwise `0`. An empty discovery set fails with
`no test files found`.

Standalone file suites run in separate temporary working directories, removed after each suite finishes, including failures. Source paths and the selected cache directory are resolved before changing directories. Relative output files therefore stay out of the caller's directory. Project suites retain the caller's working directory.

**`[TESTING-CLI-RUN-CONTEXT]`** Each suite process receives `OSPREY_TEST_COMPILER`, the executable running the suite, and `OSPREY_TEST_MEMORY`, its selected backend. The runner replaces inherited values for both variables; without a backend flag, the context reports the native GC default. Suites that launch another compiler can read these values and pass the backend through `--memory` to keep nested builds aligned.

**`[TESTING-PROJECT-SUITE]`** A `path` that is a directory holding an
`osprey.toml` is one project suite. It runs like `osprey <dir> --run`: the
manifest's `entry` and `source_roots` are assembled into one program, so a
suite's tests can live in one source root and the code they exercise in
others. The directory is never walked, and its own `*.test.*` files are sources
of that suite, not suites of their own. Only the `path` given is checked; a
project directory found while walking another directory is walked like any
other.

One invocation runs every discovered suite under exactly one memory backend.
`--memory=gc` selects tracing garbage collection, `--memory=arc` selects
Perceus reference counting, and `--memory=none` explicitly selects the
non-reclaiming backend. Omitting `--memory` uses the compiler default, `gc`.
The old `--memory=default` spelling is rejected.

**`[TESTING-PARALLEL]`** Independent suites compile and run concurrently by
default. Their captured stdout and stderr are replayed in sorted suite order,
so parallel scheduling does not scramble TAP output. The positive-integer
environment setting `OSPREY_TEST_JOBS` limits worker concurrency;
`OSPREY_TEST_JOBS=1` is the serial escape hatch for constrained or diagnostic
runs. An unset value uses the host's available parallelism, capped at four
workers and with at least two when the corpus contains multiple suites. Invalid or zero values fail
argument validation with exit code `2`.

The Android corpus harness defaults to one worker because all suites share the attached device's memory budget. An explicit `OSPREY_TEST_JOBS` overrides that default; host CPU count does not select device concurrency.

**`[TESTING-NATIVE-CACHE]`** Native test runs cache LLVM IR and object code for an assembled program and build configuration. Memory backends can share those artifacts, while each memory mode links its own executable. Reuse depends on the assembled sources and project settings, Osprey and C compiler identities, build kind and optimization setting. Executable keys additionally include the memory mode and both native runtime archives.

`OSPREY_TEST_CACHE_DIR` selects the cache directory; the test runner defaults to `osprey-test-cache-v1` under the system temporary directory. Use a new private directory for a fresh build. Custom `@linkdir` search paths bypass caching because their external inputs cannot be validated from source alone. Named system-library `@link` directives remain part of the source key. Builds that preserve debug artifacts or override debug optimization also bypass caching.

Every invocation executes the suite again and produces fresh results; `--coverage` runs collect fresh counters. Cache misses build to process-unique staging paths and publish atomically, so concurrent runners never execute a partial artifact.

### `osprey <file> --list-tests` — `[TESTING-LIST]`

Static test discovery for editors. Parses the file (skipping the type gate,
like `--symbols`, so discovery works mid-edit) and prints a JSON array of the
statically visible test cases — `test(...)` calls whose first argument is a
string literal, found wherever a call stands as a statement value (top level,
block statements, lambda/handler/match bodies, namespaces, modules):

```json
[{"name":"addition works","line":3,"column":1}]
```

`line`/`column` are 1-based and point at the test call's nearest enclosing
statement — the call's own line in the conventional top-level layout; for a
test that is a function or lambda body, the enclosing declaration's line.
Dynamically named tests (non-literal first argument) still run and report via
TAP; they are not listed statically. A file that shadows the `test` built-in
(`[TESTING-SHADOWING]`) lists nothing at all.

The listed names are also the tie-breaker for an ambiguous TAP line
(`[TESTING-TAP-AMBIGUITY]`): a run reporter compares a result's description
against them before believing a `# SKIP` directive.

### Documented test cases — `[TESTING-DOC]`

A test case is an expression statement, not a declaration, so the grammar
accepts a leading documentation block on any expression statement and the AST
carries it on `Stmt::Expr`. Both surface forms lower to the one `DocComment`
model of spec 0026: `///` in the Default flavor, `(** … *)` in ML.

```osprey
/// Addition is commutative.
///
/// # Since
/// 0.3
test("addition commutes", fn() => expect(add(1, 2), add(2, 1)))
```

A documented case gains two OPTIONAL keys in the `--list-tests` array — absent
entirely when the case carries no documentation, so an undocumented suite's
wire form is unchanged:

```json
[{"name":"addition commutes","line":6,"column":1,
  "summary":"Addition is commutative.",
  "doc":"Addition is commutative.\n\n**Since**\n\n0.3"}]
```

- `summary` is the doc's first paragraph;
- `doc` is the whole comment rendered to Markdown by `[DOC-EXPORT]` — every
  populated `# Parameters` / `# Returns` / `# Raises` / `# Examples` /
  `# See also` / `# Since` / `# Deprecated` section.

Attachment is exact: only the case's OWN statement documents it. A `///` block
on the enclosing `fn` or `let` documents that declaration, and a block above a
`{ … }` documents the block — neither is inherited by a nested case. The
reported `line` stays on the `test(` call, never on the first `///` line, so an
editor's gutter marker does not drift up into the comment.

**`[TESTING-DOC-HOVER]`** The language server answers a hover over the `test`
callee of a documented case with that case's documentation (`**Test:** <name>`
followed by the rendered block) rather than the built-in's generic signature.
An undocumented case still hovers as `**Test:** <name>`; the word `test` used
anywhere else falls through to the ordinary lookup chain, so a user binding
named `test` keeps hovering as itself.

## Line coverage

**`[TESTING-COVERAGE-CLI]`** `osprey test --coverage` builds each suite with
line-coverage instrumentation, runs it, and prints one
`# coverage: P% (covered/total lines) <file>` row per source file a suite
measured (suppressed by `--quiet`) plus a final
`# coverage total: P% (covered/total lines)` row. A file suite measures only
itself; a project suite measures every source of its program.
Coverage never changes a suite's OUTCOME — a passing suite passes whatever its
dump says, and a low percentage is never a failure. Evidence that cannot be
read is a different thing entirely: when a suite passed but its dump is absent,
unreadable or rejected, the command reports it and exits non-zero, because a run
that printed no percentage for a suite and exited green is indistinguishable
from one that measured it.
`--coverage-json <path>` (implies `--coverage`) also writes the merged
machine-readable report **`[TESTING-COVERAGE-JSON]`**:
`{"files":{"<source path>":{"lines":{"<line>":hits}}}}` — 1-based lines of
that file, every coverable line present (hit count `0` when unexecuted). A
file suite's entry is keyed by the suite path; a project suite has one entry
per source, keyed by its path under the project directory as given. Two
suites that measured the same file add their hits.

**`[TESTING-COVERAGE-CODEGEN]`** With coverage on
(`osprey_codegen::compile_program_coverage`,
`crates/osprey-codegen/src/coverage.rs`), the coverable-line universe is
seeded from the AST up front — function definition lines and positioned
statements, recursing through blocks, lambdas, match/handler arms, and
module/namespace bodies — so a never-executed (even never-lowered) line
still counts against the total. Lowering bumps a per-line `i64` counter
global inline where control flow reaches it: at each function body entry
(including inline-specialised generic bodies at their call sites) and before
each positioned statement. A generated `__osp_cov_init`, called at the top
of `main` before user code, registers every counter with the runtime.
Coverage builds keep release optimization and emit no DWARF.

**`[TESTING-COVERAGE-RUNTIME]`** `compiler/runtime/coverage_runtime.c`
(dependency-free C11, in every runtime archive) exposes
`osp_cov_register_line(line, &counter)`. Inert unless
**`[TESTING-COVERAGE-ENV]`** `OSPREY_COVERAGE=<path>` names the dump file at
process start; then an exit-time hook writes
**`[TESTING-COVERAGE-DUMP]`**: a `# osprey-coverage v2` header, one
`<line> <hits>` row per registered line, ascending, zero-hit rows included, and
a `# rows <n>` completion footer naming how many rows precede it. Every record,
the footer included, is newline-terminated.

The dump is published by renaming a sibling `<path>.partial`, so a reader opens
either nothing or a whole file. The footer is what makes a file that lost its
tail detectable: without it, a dump truncated after a valid prefix parses
cleanly and reports a HIGHER percentage over a smaller universe — partial
evidence presented as a complete result. A run whose line table could not be
grown writes no dump at all, because an under-reporting dump is indistinguishable
from honestly-uncovered code.

The reader rejects the whole file on any departure: a missing or unknown header,
a row that is not exactly two non-negative integers separated by one space, a
line number of `0` (source lines are 1-based, so `0` names nothing and would add
a phantom entry to the denominator), a repeated line number, a
missing footer, a footer whose count disagrees with the rows read, anything
after the footer, or a final record left unterminated. Skipping a row it cannot
parse is what let one good row and one corrupt row report `100.0% (1/1)`. The
terminator rule covers the one corpse the others cannot see: a dump is a strict
prefix of what its writer meant to emit, so a footer whose digits were cut short
always declares fewer rows than survived it and the count catches it — but a
file that lost nothing except its final newline agrees with itself in every
other respect, and only the missing terminator says the writer never finished.

Lines are the compiled program's 1-based lines. A file suite compiles
standalone, so they are that file's own lines (`[TESTING-CLI-RUN]`). A project
suite's program is its sources flattened into one line space, so the runner
maps each row back to the source that wrote it, at that file's own line, using
the same source map that places diagnostics (`[TESTING-PROJECT-SUITE]`). A row
no source claims stays under the suite's path.

## VS Code Test Explorer

**`[TESTING-VSCODE]`** The extension's Testing API integration:

- finds `**/*.test.{osp,ospml}` outside `target/` and `node_modules/` and
  obtains each file's child cases from `osprey <file> --list-tests`, several
  files at once, each file at most once at a time with a change landing
  mid-resolve queueing one more pass;
- keeps the tree current without a manual step: a test file created or
  changed re-resolves, a deleted one leaves the tree, any other `.osp`/`.ospml`
  change re-resolves every test file of the same project (the nearest
  `osprey.toml`, else the file's directory), and a rebuilt compiler binary
  rescans the workspace;
- answers the Testing view's Refresh with that same rescan, which also drops
  items whose file is gone;
- resolves the compiler from `osprey.server.compilerPath`, then the bundled
  platform binary, then `PATH`;
- runs requested files with `osprey <file> --run`, using
  `OSPREY_TEST_FILTER=<name>` for a single case;
- maps TAP results, diagnostics, skips, and non-TAP process failures to test
  items; and
- publishes a `test-skipped` Warning for every case a run skipped or ignored
  (`[TESTING-SKIP-WARNING-RUN]`), into its own `DiagnosticCollection` so the
  warnings reach the Problems panel and the editor gutter without disturbing
  the language server's own diagnostics.

**`[TESTING-COVERAGE-VSCODE]`** Its coverage profile runs
`osprey test <file> --coverage-json <tmp> --quiet`, adding `--filter <name>` for
a single case. It maps `[TESTING-COVERAGE-JSON]` lines to VS Code
`FileCoverage` and per-line `StatementCoverage` entries. A report's lone entry
belongs to the file that ran; a report of several files maps each entry to the
file at its own path, resolved against the directory the compiler ran in.

**`[TESTING-DOC-VSCODE]`** Documentation `[TESTING-DOC]` reaches three
surfaces, because VS Code's `TestItem` carries no tooltip of its own:

- the case's whole `doc`, collapsed to a single line, becomes its
  `TestItem.description` — the greyed text the Testing tree renders beside the
  case name. The row truncates it to the panel width, but `description` is also
  the only text the row's hover can show, so the hover must carry the entire
  block: a `description` of just the `summary` showed one line of a
  four-paragraph doc and dropped the rest. `summary` is the fallback when a
  case reports one without a rendered `doc`;
- hovering the `test(...)` call in the editor renders the whole block
  (`[TESTING-DOC-HOVER]`);
- **Osprey: Show Test Documentation** (`osprey.showTestDocumentation`, also on
  the Testing view's item context menu) opens the rendered block, its heading,
  and a `Declared at <file>:<line>` footer as Markdown. It resolves its target
  from an explicit test item first, then from the active editor's cursor —
  the nearest case declared at or above it.

A failing case's peek message leads with its documentation, then the failure,
then the `Context For AI` block, which gains a `- Documentation:` field.
Documentation is refreshed wholesale per file on every resolve, so deleting a
`///` block clears the description on the next discovery.

**`[TESTING-PROFILE]`** A third, non-default run profile — **Profile** — runs
the same discovery, filtering, and TAP mapping as Run, but executes each suite
as `osprey <file> --run --profile` (`[PROF-CLI-RUN]`) inside a per-suite
directory beneath one per-request artifact root, so several suites in one
request cannot overwrite each other's exports. Verdicts are reported exactly as
a plain run reports them; when the suite produced artifacts, the run
additionally opens the flame-graph webview (`[PROF-VSCODE-FLAME]`) and applies
the inline heat decorations (`[PROF-VSCODE-HEAT]`), and appends the suite's
profile summary plus its artifact directory to the run output. A suite that
fails to compile writes no artifacts; the run still reports the compile
failure. The sampling profiler is POSIX-only, matching `[PROF-CLI-RUN]`.

**`[TESTING-DEBUG-VSCODE]`** A fourth run profile — **Debug**, the default one
for the Testing view's debug action and its **Debug Test** context-menu entry —
runs the same discovery, filtering, and TAP mapping as Run, but launches each
suite under the Osprey debug adapter (`[DEBUGGER-EDITOR-LAUNCH]`,
docs/specs/tooling/0021-Debugger.md) instead of spawning a compiler child process, so a
breakpoint inside a `test(...)` body is live and the case's bindings are
readable where it stops.

The debugged program is the suite: `test(...)` lowers inline
(`[TESTING-CODEGEN]`) and the runtime applies `OSPREY_TEST_FILTER` when the case
begins (`[TESTING-FILTER]`), so `osprey <file> --debug --compile` produces a
binary whose stdout is the very TAP stream `osprey <file> --run` writes. Rules:

- the launch configuration names the suite as its `program`, the suite's own
  directory as `cwd`, and carries `OSPREY_TEST_FILTER` in `env` — the case name
  for a single-case run, and EMPTY for a whole-file run, never absent, so a
  value inherited from the editor's environment cannot silently skip cases;
- verdicts come from the session's own output: the adapter's `stdout` output
  events are the TAP stream and its `exited` event the exit code, mapped onto
  test items by the same rules a plain run uses, including the compile-failure
  and out-of-case-failure paths (`[TESTING-EXIT]`). `console` output is the
  adapter describing itself and is not part of the stream;
- a request VS Code refuses to launch — a failed debug build, or no adapter —
  errors the suite rather than leaving it reported as neither run nor skipped;
  cancelling the run stops the session it was waiting on.
