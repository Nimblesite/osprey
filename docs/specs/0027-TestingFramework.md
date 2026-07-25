# Testing Framework

A test file is an ordinary program whose evaluated `test(...)` calls run in
evaluation order (top level is the convention); testing adds no DSL, syntax, or
registration step.

Both flavors lower these calls to the same AST and runtime behavior. A case may
use soft assertions and return `Unit`; an ML case may instead return `Verdict`
(`Pass | Fail | Skip`).

## The built-ins

**`[TESTING-BUILTINS]`** The type environment, code generator, and C runtime
provide three functions. Testing adds no grammar or AST nodes.

### `test(name: string, body: fn() -> a) -> Unit` — `[TESTING-BUILTIN-TEST]`

Runs `body` as one named test case and prints exactly one TAP result line for
it. Test cases execute inline, in evaluation order, wherever the `test` call is
evaluated (top level is the convention). A test passes when no assertion
inside its body fails; assertions are soft — a failing `expect`/`check` marks
the case failed and execution continues, so one case can report several
mismatches.

The body's return type is polymorphic (`fn() -> a`):

- A `Unit` body reports through its `expect` and `check` calls.
- A `Verdict` body (`[TESTING-VERDICT]`) supplies one outcome for `test` to
  report.

The `body` argument must be a zero-parameter function: an inline lambda
(Default `fn() => …`, ML `\() => …`) or the name of a zero-parameter function.
Any other expression is a compile-time codegen error.

Test cases must not nest: a `test` call evaluated while another case is
running does not run its body — it prints a
`# nested test '<name>' skipped …` diagnostic and fails the enclosing case,
without advancing nested-case counters.

### `expect(actual: any, expected: any) -> Unit` — `[TESTING-BUILTIN-EXPECT]`

Compares the values with actual first (see
[Equality semantics](#equality-semantics)); on
mismatch, records a failure against the enclosing test case (or the whole run
when used outside `test`) and prints a `#` diagnostic line.

### `check(label: string, expected: any, actual: any) -> Unit` — `[TESTING-BUILTIN-CHECK]`

Takes a short label, then **expected before actual**. Its behavior otherwise
matches `expect`.

Both assertions are valid anywhere an expression is — inside `test` bodies,
in helper functions called from tests, or at the top level of a script.

**`[TESTING-SHADOWING]`** Unlike other runtime built-ins, `test`, `expect`,
and `check` do NOT reserve their names: a user-defined function or `extern`
declaration with the same name shadows the built-in in both the type
environment and codegen dispatch. Ordinary declarations may therefore use
these names.

## Equality semantics

**`[TESTING-EQUALITY]`** Assertion equality is canonical-string equality
over values with a canonical string rendering: ints, bools, floats, strings,
and `Result`s of those. Both sides render with the same `toString` lowering
used by string interpolation and compare with `strcmp`; the diagnostic shows
exactly the two rendered strings that were compared. A `Result` operand
renders a `Success` as its bare payload and an `Error` as `Error(<message>)`.
Lists, maps, and records are rejected as assertion operands at code generation.
Values of different types that render identically, such as `5` and `"5"`,
compare equal.

## ML Verdict model

**`[TESTING-VERDICT]`** An ML test case may return `Verdict`; `test` is its
reporting boundary.

`Verdict` is a user-declared union with three states:

```osprey-ml
type Verdict =
    Pass
    Fail
        reason : string
    Skip
        why : string
```

Verdict helpers use these contracts:

- `check (label, expected, actual) -> Verdict` — `Pass` on equality, else
  `Fail` carrying the labeled mismatch. Polymorphic over the compared type
  (int, string, bool) via the same canonical-string equality as the imperative
  built-in.
- `assume cond -> Verdict` — a false condition yields `Skip`; a true condition
  yields `Pass`.
- `andThen first rest -> Verdict` — combines two verdicts; `Pass` yields `rest`,
  while a first `Fail` or `Skip` is retained. Both arguments are evaluated
  before the call.

`test` recognizes the inferred `Verdict` return type and reports exactly one
outcome. `Pass` records no failure, `Fail` fails the case and prints its reason,
and `Skip` emits the TAP `# SKIP` directive.

## TAP output protocol

**`[TESTING-TAP]`** A compiled test binary writes
[TAP](https://testanything.org/)-style lines to stdout, interleaved with any
output the program itself prints:

```
ok 1 - addition works
# check 'difference' failed: expected 2, got 3
not ok 2 - subtraction works
ok 3 - overflow guard # SKIP precondition not met
1..3
# tests=3 passed=1 failed=1 skipped=1
```

- One `ok N - name` / `not ok N - name` line per executed test case, numbered
  from 1 in execution order, printed when the case's body finishes.
- A case whose `Verdict` is `Skip` still prints an `ok` line, suffixed with the
  TAP `# SKIP <why>` directive (`[TESTING-VERDICT]`); it counts as skipped,
  neither passed nor failed.
- Each failing assertion prints one `#` diagnostic line at the moment it
  fails: `# expect failed: expected E, got A`,
  `# check 'label' failed: expected E, got A`, or `# fail: <reason>` for a
  reported `Verdict` `Fail`. Diagnostics for a case therefore appear
  immediately *before* its result line.
- A failing assertion outside any test prints its diagnostic and marks the run
  failed without producing a result line. The summary's `failed` count remains
  the number of failed named cases; the out-of-case failure is reflected in the
  exit code.
- After the program's last statement, the runtime epilogue prints the plan
  `1..N` (N = cases executed) and a `# tests=N passed=P failed=F skipped=S`
  summary — including `1..0` when zero cases executed, so a filter that matched
  nothing stays visible. The epilogue is emitted only for programs that use a
  testing built-in; ordinary programs are unaffected.

## Exit code

**`[TESTING-EXIT]`** A test binary exits `0` when every executed test case
passed or skipped and no out-of-case assertion failed, else `1`. A `Skip`
verdict is not a failure and does not change the exit code. Compile errors keep
their existing CLI exit codes.

## Test filtering

**`[TESTING-FILTER]`** The environment variable `OSPREY_TEST_FILTER`, when
set and non-empty, selects exactly the test cases whose name equals its value
(exact string match). Non-matching cases are skipped silently: their bodies
do not run, they produce no TAP line, and they do not advance the numbering.
The filter is the single mechanism behind "run one test" in every front end
(CLI `--filter`, Test Explorer single-test runs).

## File naming convention

**`[TESTING-FILE-CONVENTION]`** Test files are named `*.test.osp` (Default)
or `*.test.ospml` (ML). The convention is what `osprey test` directory
discovery and the VS Code Test Explorer glob use. It is a convention, not a
gate — any Osprey program may call the testing built-ins.

## CLI

### `osprey test [path] [--filter <name>] [--quiet] [--coverage] [--coverage-json <path>]` — `[TESTING-CLI-RUN]`

Runs test files and aggregates results. `path` (default `.`) is either a
single file (run as-is, regardless of naming) or a directory searched
recursively for `[TESTING-FILE-CONVENTION]` files in sorted order. Hidden,
`target`, and `node_modules` directories are skipped; symlinks are not followed.
Each file runs like `osprey <file> --run`, with its TAP output under a
`# file: <path>` header. `--filter` sets `OSPREY_TEST_FILTER` for child
processes. The runner prints `# suites: X passed, Y failed` and exits `1` if a
suite fails to compile or run, otherwise `0`. An empty discovery set fails with
`no test files found`.

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
TAP; they are simply not listed statically.

## Line coverage

**`[TESTING-COVERAGE-CLI]`** `osprey test --coverage` builds each suite with
line-coverage instrumentation, runs it, and prints one
`# coverage: P% (covered/total lines) <file>` row per suite (suppressed by
`--quiet`) plus a final `# coverage total: P% (covered/total lines)` row.
Coverage never changes suite outcomes or the exit code.
`--coverage-json <path>` (implies `--coverage`) also writes the merged
machine-readable report **`[TESTING-COVERAGE-JSON]`**:
`{"files":{"<suite path>":{"lines":{"<line>":hits}}}}` — 1-based source
lines, every coverable line present (hit count `0` when unexecuted).

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
**`[TESTING-COVERAGE-DUMP]`**: a `# osprey-coverage v1` header followed by
one `<line> <hits>` row per registered line, ascending, zero-hit rows
included — a reader needs no other line universe.

Lines are the compiled suite's own 1-based lines. Each suite compiles standalone
and coverage measures that file (`[TESTING-CLI-RUN]`).

## VS Code Test Explorer

**`[TESTING-VSCODE]`** The extension ships a native Testing-API integration
(`vscode.tests.createTestController`) in `client/src/test-explorer.ts`,
registered from `activate()` and packaged in the VSIX:

- **Discovery**: a file-system watcher plus initial scan over
  `**/*.test.{osp,ospml}` creates one file-level item per test file; each
  file's children come from `osprey <file> --list-tests` (real parse — never
  regex), re-resolved on file change. The compiler binary is resolved with
  the same chain as the LSP (`osprey.server.compilerPath` setting → bundled
  `bin/<platform>/osprey` → `osprey` on PATH).
- **Run**: the run profile executes `osprey <file> --run` per requested file
  (cwd = the file's directory), with `OSPREY_TEST_FILTER=<name>` when a
  single case is requested, parses the TAP stream, and maps `ok`/`not ok`
  lines back to test items by name. `#` diagnostic lines preceding a
  `not ok` line become the failure message. A `# SKIP` directive on an `ok`
  line marks that case skipped in the Explorer (`[TESTING-VERDICT]`); the name
  matched against `--list-tests` is the text before the directive. Cases absent
  from the output are also marked skipped; a non-TAP failure (e.g. compile
  error) marks the file item errored with the compiler's stderr.
- **Coverage** (**`[TESTING-COVERAGE-VSCODE]`**): a second run profile
  (`TestRunProfileKind.Coverage`) executes
  `osprey test <file> --coverage-json <tmp> --quiet` per requested file
  (`--filter <name>` for a single case), maps the same TAP stream, then
  parses the `[TESTING-COVERAGE-JSON]` report into `FileCoverage` +
  per-line `StatementCoverage` — VS Code shows the percentage in the Test
  Coverage view and hit counts in the editor gutter.

## Runtime

**`[TESTING-RUNTIME]`** `compiler/runtime/test_runtime.c` holds the run
state (cases executed/failed/skipped, in-case failure and skip flags) and the
symbols emitted by codegen: `osp_test_begin(name)` (returns whether the case
runs, applying `[TESTING-FILTER]`), `osp_test_assert(label, ok, expected,
actual)` (label is NULL for `expect`), the `Verdict` report primitives
`osp_test_pass()` / `osp_test_fail(reason)` / `osp_test_skip(why)`
(`[TESTING-VERDICT]`), `osp_test_end(name)` (prints the TAP result line, with a
`# SKIP` directive when the case reported `Skip`), and `osp_test_finalize()`
(prints plan + summary including `skipped=S`, returns the exit code). The unit
is dependency-free C11, compiled into `libfiber_runtime.a` (and its
`_gc`/HTTP/wasm siblings), and assumes single-fiber test execution
(`[TESTING-RISK-FIBERS]`).

**`[TESTING-CODEGEN]`** Codegen lowers the built-ins in
`crates/osprey-codegen/src/testing.rs`: `test` evaluates its name, calls
`osp_test_begin`, branches around the inlined body, and calls `osp_test_end`;
when the body's inferred type is `Verdict` it pattern-matches the result and
reports it through `osp_test_pass`/`fail`/`skip` (`[TESTING-VERDICT]`), else the
body's inline `expect`/`check` already recorded via `osp_test_assert`.
`expect`/`check` unwrap + stringify both values, `strcmp`-compare, and call
`osp_test_assert`. Any use sets a per-module flag that makes `main` return
`osp_test_finalize()` instead of `0`.

## Risks

- **`[TESTING-RISK-FIBERS]`** The run state is plain (non-atomic) C globals;
  assertions performed inside spawned fibers may interleave TAP lines or
  miscount. Tests should assert on the main fiber (await fiber results, then
  assert).
- Test names are matched by exact string in filtering and in the Test
  Explorer TAP mapping; duplicate names within one file resolve to the last
  matching item.
