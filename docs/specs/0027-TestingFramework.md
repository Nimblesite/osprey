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
> surface reads like Jest (`test("adds", fn() => expect(add(2, 3), 5))`) and
> stays imperative — a `fn() -> Unit` case firing soft assertions. The ML
> surface keeps the Alcotest-style spelling but adopts a *pure, value-based*
> model: a case is a pure function returning a three-state `Verdict`
> (`Pass | Fail | Skip`), and `test` reports it. See
> [the Verdict model](#the-pure-ml-flavor-verdict-model) and
> [Language Flavors](0023-LanguageFlavors.md).

- [Status](#status)
- [The built-ins](#the-built-ins)
- [Equality semantics](#equality-semantics)
- [The pure ML-flavor Verdict model](#the-pure-ml-flavor-verdict-model)
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

### `test(name: string, body: fn() -> a) -> Unit` — `[TESTING-BUILTIN-TEST]`

Runs `body` as one named test case and prints exactly one TAP result line for
it. Test cases execute inline, in source order, wherever the `test` call is
evaluated (top level is the convention). A test passes when no assertion
inside its body fails; assertions are soft — a failing `expect`/`check` marks
the case failed and execution continues, so one case can report several
mismatches.

The body's return type is polymorphic (`fn() -> a`), which is what lets one
`test` built-in drive both surfaces:

- **Imperative (Unit) body** — the Default-flavor style. The body runs
  `expect`/`check` for their side effect and returns `Unit`; `test` records
  nothing extra, the inline assertions having already reported.
- **Verdict body** — the pure ML-flavor style (`[TESTING-VERDICT]`). The body
  returns a `Verdict` value; `test` pattern-matches it and reports the single
  outcome. No exceptions, no side-effecting assertions.

The `body` argument must be a zero-parameter function: an inline lambda
(Default `fn() => …`, ML `\() => …`) or the name of a zero-parameter function.
Any other expression is a compile-time codegen error.

Test cases must not nest: a `test` call evaluated while another case is
running does not run its body — it prints a
`# nested test '<name>' skipped …` diagnostic and fails the enclosing case,
so the mistake is loud rather than silently reshuffling the run's counters.

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
and `check` do NOT reserve their names: a user-defined function or `extern`
declaration with the same name shadows the built-in in both the type
environment and codegen dispatch. This keeps pre-existing programs (e.g. a
`fn check(t: Tree)` helper or an `extern fn check(...)`) compiling unchanged.

## Equality semantics

**`[TESTING-EQUALITY]`** Assertion equality is *canonical-string equality*
over values with a canonical string rendering: ints, bools, floats, strings,
and `Result`s of those. Both sides render with the same `toString` lowering
used by string interpolation and compare with `strcmp`; the diagnostic shows
exactly the two rendered strings that were compared. A `Result` operand
renders discriminant-aware: a `Success` as its bare payload (so
`expect(intDiv(4, 2), 2)` passes), an `Error` as `Error(<message>)` (so
asserting against a failed computation is a visible mismatch, never a blind
payload read). Lists, maps, and records have no canonical rendering yet, so
an assertion operand of those types is a compile-time codegen error rather
than a silent pointer comparison. Corollary: values of different types that
render identically (e.g. `5` and `"5"`) compare equal; assert on the value
the test actually computes.

## The pure ML-flavor Verdict model

**`[TESTING-VERDICT]`** The Default flavor keeps the imperative, soft-assertion
style familiar from Jest/Alcotest: a case is a `fn() -> Unit` that fires
`expect`/`check` for their side effect. The ML flavor gets a *pure, value-based*
surface instead, in the spirit of QuickCheck's three-state result — a test case
is a pure function returning a **`Verdict`** value, and `test` is the sole
effect boundary that reports it.

`Verdict` is an ordinary user-declared union with three states — no compiler
magic beyond the report primitives below:

```osprey-ml
type Verdict =
    Pass
    Fail
        reason : string
    Skip
        why : string
```

The surface is built from ordinary pure functions over that type. Because a
curried, generic, union-returning function currently defeats monomorphization,
the equality assertions take a **tuple** argument (which the shared-core lowering
accepts in both flavors):

- `check (label, expected, actual) -> Verdict` — `Pass` on equality, else
  `Fail` carrying the labeled mismatch. Polymorphic over the compared type
  (int, string, bool) via the same canonical-string equality as the imperative
  built-in.
- `assume cond -> Verdict` — QuickCheck's `==>`: a false precondition yields
  `Skip`, not `Fail`, so an unmet assumption is a distinct third outcome rather
  than a spurious failure.
- `andThen first rest -> Verdict` — sequences two verdicts; the first `Fail`
  or `Skip` short-circuits, so a case is a single `andThen` chain of checks.

`test` recognizes a `Verdict`-typed body by its inferred type, pattern-matches
it, and calls exactly one report primitive. These primitives are internal
codegen targets (like the TAP runtime's `osp_test_assert`), not user-facing
built-ins: `reportPass()`, `reportFail(reason)`, `reportSkip(why)`. A `Pass`
records nothing; a `Fail` fails the case and prints its reason; a `Skip` emits
the TAP `# SKIP` directive.

```osprey-ml
depositClears () =
    andThen (check ("balance", 425000, ledgerBalance))
        (check ("count", 1, txnCount))

overdraftRefused () = assume (not enoughFunds)   // Skip when the guard holds

test "a cleared deposit updates the balance" depositClears
test "an overdraft is refused" overdraftRefused
```

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
- A failing assertion outside any test prints its diagnostic and counts
  toward the run's failure total without producing a result line.
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

### `osprey test [path] [--filter <name>] [--quiet]` — `[TESTING-CLI-RUN]`

Runs test files and aggregates results. `path` (default `.`) is either a
single file (run as-is, regardless of naming) or a directory searched
recursively for `[TESTING-FILE-CONVENTION]` files (sorted, deterministic
order; hidden/`target`/`node_modules` dirs skipped; symlinks not followed). Each file is compiled and executed like
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
