# Testing Framework

Osprey's built-in testing harness: three built-in functions (`test`, `expect`,
`check`), a TAP text protocol emitted by compiled test binaries, an
`osprey test` CLI runner, a `--list-tests` discovery mode, and a VS Code Test
Explorer integration shipped in the extension. The framework is deliberately
ultra-minimal: no test DSL, no new syntax, no registration step — a test file
is an ordinary Osprey program whose top-level `test(...)` calls run in source
order.

> **Flavor layer — shared core (AST and above).** The testing built-ins are
> shared core exactly like every other built-in ([0012](0012-Built-InFunctions.md)):
> the same functions exist in every flavor and lower to the same canonical
> `Expr::Call` nodes. Only the spelling is a flavor concern. The Default
> surface reads like Jest (`test("adds", fn() => expect(add(2, 3), 5))`); the
> ML surface reads like Alcotest (`test "adds" (\() => check "sum" 5 (add (2, 3)))`).
> See [Language Flavors](0023-LanguageFlavors.md).

- [Status](#status)
- [The built-ins](#the-built-ins)
- [Equality semantics](#equality-semantics)
- [TAP output protocol](#tap-output-protocol)
- [Exit code](#exit-code)
- [Test filtering](#test-filtering)
- [File naming convention](#file-naming-convention)
- [CLI](#cli)
- [VS Code Test Explorer](#vs-code-test-explorer)
- [Runtime](#runtime)
- [Decision record and assumptions](#decision-record-and-assumptions)
- [Risks](#risks)
- [Cross-references](#cross-references)

## Status

Implemented for both flavors (Default and ML) on the native target. The C
test-runtime unit is also compiled into the wasm archive so `--target=wasm32`
links, but the wasm runner path is not exercised by the harness.

## The built-ins

**`[TESTING-BUILTINS]`** Three built-in functions form the whole framework.
They are ordinary built-ins: declared in the type checker's base environment,
lowered by codegen, backed by the C runtime. No new grammar, keywords, or AST
nodes exist for testing.

### `test(name: string, body: fn() -> Unit) -> Unit` — `[TESTING-BUILTIN-TEST]`

Runs `body` as one named test case and prints exactly one TAP result line for
it. Test cases execute inline, in source order, wherever the `test` call is
evaluated (top level is the convention). A test passes when no assertion
inside its body fails; assertions are soft — a failing `expect`/`check` marks
the case failed and execution continues, so one case can report several
mismatches.

The `body` argument must be a zero-parameter function: an inline lambda
(Default `fn() => …`, ML `\() => …`) or the name of a zero-parameter function.
Any other expression is a compile-time codegen error.

```osprey
test("addition works", fn() => {
    expect(add(2, 3), 5)
    expect(add(0, 0), 0)
})
```

```osprey-ml
test "addition works" (\() =>
    check "sum" 5 (add (2, 3)))
```

### `expect(actual: any, expected: any) -> Unit` — `[TESTING-BUILTIN-EXPECT]`

The Default-flavor-familiar assertion (Jest argument order: actual first).
Compares the two values (see [Equality semantics](#equality-semantics)); on
mismatch, records a failure against the enclosing test case (or the whole run
when used outside `test`) and prints a `#` diagnostic line.

### `check(label: string, expected: any, actual: any) -> Unit` — `[TESTING-BUILTIN-CHECK]`

The ML-flavor-familiar assertion, modeled on Alcotest's
`check testable msg expected actual`: a short label, then **expected before
actual**. Behavior is otherwise identical to `expect`.

Both assertions are valid anywhere an expression is — inside `test` bodies,
in helper functions called from tests, or at the top level of a script.

**`[TESTING-SHADOWING]`** Unlike other runtime built-ins, `test`, `expect`,
and `check` do NOT reserve their names: a user-defined function with the same
name shadows the built-in in both the type environment and codegen dispatch.
This keeps pre-existing programs (e.g. a `fn check(t: Tree)` helper) compiling
unchanged.

## Equality semantics

**`[TESTING-EQUALITY]`** Assertion equality is *canonical-string equality*:
both sides are auto-unwrapped if they are `Result` values (mirroring the `==`
operator's checker rule), rendered with the same `toString` lowering used by
string interpolation, and compared with `strcmp`. This gives uniform,
structural, human-explainable equality across ints, bools, floats, strings,
`Result`s, records, and lists — the diagnostic shows exactly the two rendered
strings that were compared. Corollary: values of different types that render
identically (e.g. `5` and `"5"`) compare equal; assert on the value the test
actually computes.

## TAP output protocol

**`[TESTING-TAP]`** A compiled test binary writes
[TAP](https://testanything.org/)-style lines to stdout, interleaved with any
output the program itself prints:

```
ok 1 - addition works
# check 'difference' failed: expected 2, got 3
not ok 2 - subtraction works
1..2
# tests=2 passed=1 failed=1
```

- One `ok N - name` / `not ok N - name` line per executed test case, numbered
  from 1 in execution order, printed when the case's body finishes.
- Each failing assertion prints one `#` diagnostic line at the moment it
  fails: `# expect failed: expected E, got A` or
  `# check 'label' failed: expected E, got A`. Diagnostics for a case
  therefore appear immediately *before* its result line.
- A failing assertion outside any test prints its diagnostic and counts
  toward the run's failure total without producing a result line.
- After the program's last statement, the runtime epilogue prints the plan
  `1..N` (N = cases executed) and a `# tests=N passed=P failed=F` summary.
  The epilogue is emitted only for programs that use a testing built-in;
  ordinary programs are unaffected.

## Exit code

**`[TESTING-EXIT]`** A test binary exits `0` when every executed test case
passed and no out-of-case assertion failed, else `1`. Compile errors keep
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

### `osprey test [path] [--filter <name>] [--quiet]` — `[TESTING-CLI-RUN]`

Runs test files and aggregates results. `path` (default `.`) is either a
single file (run as-is, regardless of naming) or a directory searched
recursively for `[TESTING-FILE-CONVENTION]` files (sorted, deterministic
order, hidden/`target` dirs skipped). Each file is compiled and executed like
`osprey <file> --run`; its TAP output streams through unmodified under a
`# file: <path>` header line. `--filter` sets `OSPREY_TEST_FILTER` for the
child processes. After all files, the runner prints
`# suites: X passed, Y failed` and exits `1` if any suite failed (test
failures or compile errors), else `0`. An empty discovery set is a failure
(`no test files found`).

### `osprey <file> --list-tests` — `[TESTING-LIST]`

Static test discovery for editors. Parses the file (skipping the type gate,
like `--symbols`, so discovery works mid-edit) and prints a JSON array of the
statically visible test cases — `test(...)` calls whose first argument is a
string literal, found anywhere in the program's expression trees:

```json
[{"name":"addition works","line":3,"column":1}]
```

`line`/`column` are 1-based and point at the `test` call. Dynamically named
tests (non-literal first argument) still run and report via TAP; they are
simply not listed statically.

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
  `not ok` line become the failure message. Cases absent from the output are
  marked skipped; a non-TAP failure (e.g. compile error) marks the file item
  errored with the compiler's stderr.

## Runtime

**`[TESTING-RUNTIME]`** `compiler/runtime/test_runtime.c` holds the run
state (cases executed/failed, in-case failure count) and four symbols emitted
by codegen: `osp_test_begin(name)` (returns whether the case runs, applying
`[TESTING-FILTER]`), `osp_test_assert(label, ok, expected, actual)` (label is
NULL for `expect`), `osp_test_end(name)` (prints the TAP result line), and
`osp_test_finalize()` (prints plan + summary, returns the exit code). The
unit is dependency-free C11, compiled into `libfiber_runtime.a` (and its
`_gc`/HTTP/wasm siblings), and assumes single-fiber test execution
(`[TESTING-RISK-FIBERS]`).

**`[TESTING-CODEGEN]`** Codegen lowers the three built-ins in
`crates/osprey-codegen/src/testing.rs`: `test` evaluates its name, calls
`osp_test_begin`, branches around the inlined body, and calls `osp_test_end`;
`expect`/`check` unwrap + stringify both values, `strcmp`-compare, and call
`osp_test_assert`. Any use sets a per-module flag that makes `main` return
`osp_test_finalize()` instead of `0`.

## Decision record and assumptions

**2026-07-12 — built-ins over syntax.** A `test "name" = expr` declaration
form was rejected: it would touch the tree-sitter grammar, the ML parser, the
AST, and project assembly for zero expressive gain. Built-in calls need no
grammar change, work identically in both flavors, and keep discovery trivial
(`Expr::Call` walk).

**2026-07-12 — canonical-string equality.** Codegen's `==` is shallow
(handles compare as `i64` for lists/records). Rather than build deep
structural equality into the backend for v1, assertions compare `toString`
renderings — the same canonical form the golden-example harness asserts on.
Revisit if/when deep `==` lands in codegen; the TAP surface won't change.

**2026-07-12 — soft assertions.** A failing assertion does not abort the
case (Osprey has no exceptions; an abort would need effect machinery for a
feature the golden harness doesn't need). Alcotest aborts on first failure;
this framework reports all mismatches in a case. Divergence noted in the
docs.

**2026-07-12 — filtering by exact name via environment.** An env var needs
no argv plumbing through the compiled binary and works identically for
`osprey test --filter`, bare `--run` invocations, and the Test Explorer.
Exact match keeps "run this one test" unambiguous.

## Risks

- **`[TESTING-RISK-FIBERS]`** The run state is plain (non-atomic) C globals;
  assertions performed inside spawned fibers may interleave TAP lines or
  miscount. Tests should assert on the main fiber (await fiber results, then
  assert).
- Canonical-string equality conflates values with identical renderings
  across types (`5` vs `"5"`). Accepted for v1; documented in
  `[TESTING-EQUALITY]`.
- Test names are matched by exact string in filtering and in the Test
  Explorer TAP mapping; duplicate names within one file resolve to the last
  matching item.

## Cross-references

- [0012 Built-In Functions](0012-Built-InFunctions.md) — the built-in
  reference (testing built-ins listed there, normative rules here).
- [0023 Language Flavors](0023-LanguageFlavors.md) — flavor selection and
  the shared-core boundary the built-ins sit above.
- [0024 ML Flavor Syntax](0024-MLFlavorSyntax.md) — the ML surface used in
  ML-flavor test files.
- `crates/diff_examples.sh` — the golden harness that runs
  `examples/tested/testing/` (the framework's own executable examples).
