# Branch review: `resume` compared with `main`

## Verdict

**DO NOT MERGE.**

The branch is a step forward in several substantial areas, and its test corpus is broader than `main`. However, the current tip introduces one catastrophic test-discovery false negative, two high-impact runtime/tooling regressions, and one compiler-contract regression. The first finding alone is a hard merge blocker because it can make real tests silently disappear from every discovery-driven surface.

Compared revisions:

- `main`: `713366f9e9e6`
- `resume`: `c551824de464`
- Scope: 258 changed files, approximately 28,328 insertions and 1,986 deletions
- Method: static analysis only. **No tests, builds, linters, formatters, generators, or project executables were run.**

## Findings

### 1. CRITICAL — HOLY FUCK: one unrelated private `test` declaration silently nukes the entire test inventory

**This is a fucking catastrophic false-negative factory.** A single scoped helper named `test` inside an unrelated module or namespace causes `collect_tests` to return an empty vector for the entire source file (`crates/osprey-lsp/src/testing.rs:120-160`). Legitimate tests elsewhere do not merely receive the wrong metadata: they completely vanish.

The failure is caused by this file-global precondition:

```rust
if shadows_test_builtin(&program.statements) {
    return Vec::new();
}
```

`shadows_test_builtin` recursively searches every module and namespace body for any function or extern named `test`. It does not ask whether that declaration is visible at the call site. This turns a lexical shadowing rule into an entire-program kill switch.

A minimal shape that exposes the defect is:

```osprey
module Helpers {
    fn test(name, body) = 0
}

test("must stay visible", fn() => expect(1, 1))
```

`Helpers.test` cannot shadow the sibling top-level builtin call, but the collector sees the nested spelling and returns `[]` before walking any call sites.

The blast radius is obscene:

- `osprey --list-tests` silently reports no tests.
- VS Code Test Explorer silently drops legitimate tests.
- Test hovers disappear.
- Static skipped-test diagnostics go blind.
- CLI skip-name validation loses the same inventory because it also derives names from `collect_tests`.
- There is no warning or parse error to distinguish this catastrophe from a genuinely test-free file.

The added shadowing test covers a file where a top-level user function named `test` legitimately owns that scope. It does not cover an unrelated nested declaration alongside a real builtin test call, so it blesses the broad early return without testing lexical isolation.

**Required fix:** remove the program-wide early return. Walk the tree with lexical scope state and suppress a builtin-looking call only when `test` is shadowed at that call site. Add a regression that places a private `test` declaration in one module/namespace and a legitimate test in a sibling or outer scope, then assert that listing, hover discovery, and skipped-test diagnostics retain the legitimate case.

### 2. High — coverage can report `100.0%` with zero evidence and still exit successfully

Coverage collection is not part of the command's success calculation. `run_suites` increments the failure count for suite-process failures and unexplained skips, but `collect_suite_coverage` returns no status (`crates/osprey-cli/src/test_cmd.rs:188-220`). Missing, unreadable, or malformed dump files only produce stderr messages (`test_cmd.rs:408-448`).

That creates a direct false-green path:

1. Every suite process exits successfully.
2. Every coverage dump is absent, unreadable, or rejected.
3. The aggregate contains zero coverable lines.
4. `percent(0, 0)` deliberately returns `100.0` (`test_cmd.rs:455-458`).
5. The command reports `# coverage total: 100.0% (0/0 lines)` and exits zero.

There are two related integrity failures:

- The dump path is stable and is not removed before launching a suite (`test_cmd.rs:398-427`). If the current run fails to write a dump, an artifact from an interrupted earlier run can be consumed as current evidence.
- Failure to write the requested coverage JSON only prints an error (`test_cmd.rs:480-495`). A passing suite run can therefore exit zero without producing the explicitly requested artifact.

The new unit coverage locks in `0/0 == 100%` and exercises malformed headers, but it does not prove that a passing suite with missing coverage fails. The bad-JSON-path end-to-end case already contains a failing suite, so it cannot establish that JSON-output failure affects the exit status.

**Required fix:** delete the per-suite dump before spawning the process; make dump collection return a result that contributes to command failure; reject missing/malformed/empty evidence when coverage was requested; and fail the command if the requested JSON cannot be written. Add static test cases around those result paths rather than relying on stderr text.

### 3. High — `input()` now mistakes a one-second pause for EOF and destroys valid pipe input

The POSIX runtime now waits at most one second before every byte read (`compiler/runtime/random_runtime.c:103-180`). `osp_input_ready` uses `select`, `osp_input_byte` maps a timeout to `EOF`, and `osp_input` returns the accumulated string as though the stream had actually ended.

This changes a blocking line read into lossy wall-clock polling. A valid producer that starts after one second returns an empty string; a delay longer than one second between bytes truncates a partially delivered line. For example, a program reading from a pipe equivalent to this can return before `hello` exists:

```sh
(sleep 2; printf 'hello\n') | osprey program --run
```

An open pipe with a future producer is not EOF. The documented contract says that `input()` reads one line and returns empty at EOF (`docs/specs/0012-Built-InFunctions.md:37-45`); elapsed silence is neither condition.

The motivating noninteractive hang already has the correct boundary fix in this branch: the VS Code launcher captures the child and closes its stdin (`vscode-extension/client/src/extension.ts:744-757`), while the test runner uses null stdin. The runtime timeout is therefore both redundant for those callers and harmful to legitimate streaming callers. It also creates platform divergence because the corresponding Windows path retains blocking `getchar` behavior.

The added `input_never_blocks` regression covers a permanently open, silent stdin. It does not cover a delayed producer or a delay between valid bytes, so it encodes the new truncation behavior without defending the original line-input contract.

**Required fix:** remove the wall-clock-as-EOF behavior from the runtime. Close stdin in noninteractive launchers that cannot supply input, as the branch already does. If nonblocking input is desired, expose it as an explicit API rather than impersonating EOF. Add delayed-first-byte and delayed-inter-byte regression coverage.

### 4. Medium — incomplete `Verdict` unions are now accepted

The new verdict-arm generator correctly adapts to payload-free states and field-name variations, but it no longer verifies that the union contains all three required states (`crates/osprey-codegen/src/testing.rs:80-174`). It checks that some union named `Verdict` exists, then generates arms only for the states that happen to be declared. Unknown extra states are rejected, but missing `Pass`, `Fail`, or `Skip` states are not.

As a result, a declaration equivalent to `type Verdict = Pass` can compile and report a passing test even though both the implementation's error text and the testing specification require `Pass`, `Fail`, and `Skip` (`docs/specs/0027-TestingFramework.md:93-123`). On `main`, code generation emitted all three arms, so a missing constructor could not silently disappear from the generated match.

This is a contract weakening introduced while making verdict payload layouts more flexible.

**Required fix:** validate that the declared state-name set is exactly `{Pass, Fail, Skip}` before generating layout-sensitive arms. Preserve the useful payload flexibility, and add negative cases for each individually missing state.

## Test-strength audit

The branch does make meaningful forward progress:

- The corpus ratchets increase from 179 to 203 native programs and from 126 to 142 WebAssembly programs; no ratchet reduction was found.
- The checked-in corpus contains 203 test programs and 105 expected-output files.
- Static registration counts match every checked-in TAP plan. The expanded Default/ML twin suites now have matching registration counts.
- No pre-existing assertion was found to have been simply deleted. The four removed `knownbugs/bug1` through `bug4` scenarios were migrated into active `fiber_showcase` and `user_defined_unions` suites in both syntax flavors.
- The random-runtime coverage threshold rises from 45 to 86; no lowered coverage threshold was found.
- The CI workflow changes do not remove a test, lint, build, or coverage gate.

Those gains do not compensate for finding 1. A larger corpus is meaningless if an unrelated symbol can make real cases disappear from discovery without a trace. Finding 2 compounds the problem by allowing absent coverage data to present itself as perfect coverage.

One additional limitation should stay explicit: `tests/core/collections/nested_generic_collections_fibers.test.osp.expectedoutput` accepts a skipped case because a `Channel<collection>` round trip still drops nested descriptors. Adding the scenario is useful, but it remains an acknowledged blind spot while still contributing to the 203-program corpus count.

## Disposition of findings from the previous report

The previous `BRANCH_REVIEW.md` described an older branch tip plus staged changes. Its five findings are no longer current:

- Default/ML registration parity and TAP-plan mismatches have been repaired.
- ML callable-name tracking now uses lexical scopes for parameters instead of one process-wide parameter set.
- Generic file-scope function values are seeded before global publication.
- Mixed-handler dispatcher recursion now uses a guaranteed `musttail` call.
- The earlier collection-owner issues remain repaired.

This report replaces those stale findings with the current-tip issues above.

## Recommendation

Do not merge the branch in its current state.

Fix finding 1 before anything else: it is a pants-on-fire, test-erasing merge blocker. Then make coverage evidence fail closed, restore lossless `input()` semantics, and reinstate exact `Verdict` state validation. Preserve the branch's genuine gains—expanded corpus coverage, repaired flavor parity, higher runtime coverage threshold, descriptor work, and tail-call hardening—while closing these regressions.
