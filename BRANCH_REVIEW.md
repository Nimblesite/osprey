# Branch review: `resume` compared with `main`

## Verdict: **DO NOT MERGE**

This branch is broadly a step forward, but it is **not mergeable in its current
state**. The catastrophic input regression found during this review has now been
fixed and committed. Coverage, however, can still launder broken evidence into
a green result and can claim to have written JSON that is not valid JSON.

Put bluntly: most of the engineering is moving in the right direction, but the
remaining failures are not cosmetic shit. They sit exactly where users and CI
decide whether reality happened.

## Scope and method

- Live static-audit snapshot: **2026-08-26 09:40 AEST**. This timestamp and the
  disposition below are refreshed on the requested ten-minute cycle.
- Compared local `main` (`713366f9`) with `resume` (`9f506c01`): **272 files,
  31,037 insertions, 2,573 deletions**.
- Also inspected the current uncommitted worktree separately. Those edits are
  promising, but they are **not part of the branch** and cannot be credited as
  merged protection.
- Static analysis only: diffs, source, test registration, expected-output
  inventory, thresholds, CI wiring, and documentation contracts.
- **No tests, builds, linters, formatters, generators, or project executables
  were run.** `git diff --check` was the only mechanical validation, and it was
  clean for both the committed branch and the worktree.

## Merge blockers

### 1. HIGH — the coverage “fix” still accepts corrupt evidence and can report green

**ONE VALID ROW CAN LAUNDER A TRUNCATED COVERAGE DUMP INTO FUCKING SUCCESS.**

`parse_dump` in `crates/osprey-cli/src/test_cmd.rs:464-482` checks the header,
then silently skips every malformed row. `collect_suite_coverage` rejects only
an entirely empty map. Therefore this dump is accepted:

```text
# osprey-coverage v1
1 1
2 garbage
```

The resulting report says 100% (`1/1`). Line 2 vanishes from the denominator.
The command can exit green even though its evidence was partially written,
corrupted, or truncated after a valid prefix.

This is not theoretical protocol pedantry. `coverage_runtime.c:40-47` writes the
header and rows directly to the final file; a failed/truncated close can leave a
valid prefix. Worse, `cov_reserve` explicitly continues after allocation failure
and documents that coverage will under-report (`coverage_runtime.c:50-66`). The
reader has no declared row count, checksum, completion footer, or atomic publish
marker with which to distinguish complete evidence from a corpse.

The new tests reinforce the hole: lines 813-816 explicitly specify that bad rows
are skipped, and only verify failure when **every** row is junk. There is no
mixed-valid-and-invalid case.

Required before merge:

- Reject the entire dump on any non-empty row that is not exactly two valid
  columns.
- Reject duplicate or invalid line numbers instead of overwriting/skipping them.
- Add a declared row count or completion footer written only after a successful
  flush, preferably via temporary file plus atomic rename.
- Add mixed valid/invalid, duplicate-row, extra-column, and truncated-prefix
  regression cases. Any one must fail the command.

### 2. MEDIUM — coverage JSON can be invalid while `--coverage-json` reports success

`json_string` (`crates/osprey-cli/src/test_cmd.rs:538-543`) escapes only quotes
and backslashes. Its comment claims discovery cannot produce control characters
in paths. That claim is false on Unix: filenames can contain tabs and newlines,
and the discovery walk does not reject them. JSON requires every U+0000–U+001F
character to be escaped.

The writer can therefore emit malformed JSON and return `true`, making the CLI
claim it successfully produced the requested machine-readable artifact. The
only test path contains a quote, so the control-character hole is invisible.

Use a real JSON serializer, or correctly escape the complete JSON string domain,
and parse the written artifact in a regression containing newline/tab characters.

## The thermonuclear discovery bug: fixed

The earlier implementation contained an **absolute fucking catastrophe**: an
unrelated `test` helper inside a namespace or module could make legitimate
sibling tests disappear from editor/list discovery. No diagnostic. No partial
inventory. No hint. An innocent scoped declaration elsewhere in the file became
a global kill switch for the fucking test explorer.

That is now fixed. `shadows_test_builtin` is deliberately non-recursive
(`crates/osprey-lsp/src/testing.rs:134-145`), and namespace/module bodies are
checked at their own lexical boundary (`:191-201`). The regression
`a_test_declared_inside_a_container_does_not_erase_sibling_cases` covers both
container forms and asserts the top-level case survives (`:436-459`). This is a
real step forward and exactly the kind of failure the branch needed to pin.

One residual discovery limitation remains: shadow detection recognizes only
function and extern declarations. A value binding or function parameter named
`test` can still produce phantom editor tests even though the call resolves to
the local value. This is the inverse failure—false positives rather than erased
tests—and should be handled by scope-aware resolution rather than more
file-wide heuristics.

## Other findings revalidated as fixed

- **`input()` forging EOF after one second:** fixed in `9f506c01`. The runtime
  now blocks until data or real EOF (`compiler/runtime/random_runtime.c:105-129`),
  and the launcher obligation remains at the launcher. The previously
  contradictory `input_never_blocks.rs` now tests `--run` with closed stdin and
  verifies that `osprey test` closes each case's stdin even when its own is open.
  The committed `input_descriptor_states.rs` is the real regression lock: it
  synchronizes on a readiness marker, waits three seconds—past the deleted
  one-second grace—and separately covers delayed first byte, mid-line pause,
  final line at EOF, open silence, and release by real close. **The fucking clock
  is no longer allowed to impersonate EOF.**
- **Incomplete `Verdict` unions:** fixed. Codegen now rejects any declaration
  missing `Pass`, `Fail`, or `Skip` before generating report arms, with exact
  missing-state tests and matching spec text.
- **Out-of-bounds HTTP client handles:** fixed. Creation, use, and close paths
  validate handles; exhaustion has a defined error and regression coverage.
- **Server self-stop dropping its active response:** fixed. The current client
  socket is preserved for an in-handler stop and covered by a regression.
- **Old TAP/ML/generic-global/musttail findings:** the repairs remain present;
  this re-audit found no reversal.

## Test-strength and gate audit

### Acceptance standard: coverage tourism can fuck off

A percentage increase does not close a finding. A meaningful regression test
must be derived from a named specification obligation and must prove the
observable contract, not merely execute the implementation. For every affected
operation, this review requires the applicable combination of:

- the success-state table and exact returned values;
- every specified rejection, including boundary and just-outside-boundary cases;
- externally visible side effects and the absence of forbidden side effects;
- ownership/lifetime behavior across repeated use and cleanup;
- failure ordering when more than one precondition is invalid;
- resource cleanup and stale-artifact prevention;
- process/command exit status, not just a diagnostic printed to stderr; and
- cross-consumer parity where CLI, runtime, LSP, editor, native, ML, or wasm are
  supposed to implement the same clause.

Tests added solely to walk uncovered branches, without assertions tied to the
governing spec ID, are **not evidence that the branch is safer**. They are gcov
tourism wearing a fake moustache.

The broad direction is positive:

- Native corpus ratchet rises from 179 to 203 programs; wasm rises from 126 to
  142. Neither is lowered.
- The known-bug programs removed from `tests/regressions/basics/knownbugs` were
  migrated into active suites rather than silently deleted.
- Coverage floors are raised, often aggressively: `random_runtime` 45→95,
  `fiber_runtime` 68→85, both WebSocket runtimes to 85, and multiple HTTP/C
  components to 85 or 95. No threshold reduction was found.
- CI gates remain wired; the branch does not delete the dead-code, corpus, or
  coverage enforcement paths.
- The expanded fiber tests replace narrow capacity/bounds checks with stronger
  FIFO, wraparound, blocking, handle-guard, exhaustion, repeated-await, and
  cleanup cases. The removed assertions are substantively covered by the new
  ones rather than weakened.

These improvements matter, but a higher threshold cannot rescue a lying
coverage protocol.

### Live worktree review: proposed fixes are not clear yet

The uncommitted runtime test expansion is mostly the right kind of work:

- coverage-runtime cases cite `[TESTING-COVERAGE-RUNTIME/DUMP]` and assert the
  diagnostic, failing destination, absence of a false artifact, normal process
  exit, and the distinction between open failure and finish failure;
- effect death tests cite `[EFFECTS-OPERATION-MAILBOX]`/`[EFFECTS-RESUME]` and
  assert `SIGABRT` across null continuation/body/mailbox plus both sides of the
  mailbox-index boundary; and
- file-runtime cases cite `[BUILTIN-FILE]`/`[BUILTIN-FILE-ERRMSG]` and assert
  exact failure shape, operation/path attribution, owned-message lifetime,
  clearing, unknown errno, zero errno, and null inputs.

The earlier system-runtime fork test was vacuous because it measured the
parent's descriptor table after making a child perform the operation. That
fucking cardboard oracle has now been replaced. `test_fork.h` deterministically
denies the runtime's `fork` in the same process; the test pins `-6`, repeats the
failure, inventories the same descriptor table, removes the injection, performs
a successful spawn/await/cleanup, and proves the slot and descriptors are
reusable. `[BUILTIN-PROCESS-FAILURE]` now states the exact error precedence and
resource obligations. The production -4 and -6 arms explicitly close every
pipe pair before abandoning the record. Static disposition: **accepted**, with
the seam's documented single-threaded boundary kept intact.

The `-3` and `-5` paths have since gained real seams. Production now checks
mutex initialization and command duplication on both platforms, maps either to
`-3`, and never asks `abandon_process` to destroy an uninitialized mutex. The
allocator test separately refuses the record and command copy, asserts exact
`-3`, live-allocation baselines, and a recovery spawn. The thread interposer
deterministically reaches `-5` in the same process and asserts the exact code,
descriptor inventory, no remaining child, and recovery. This closes the record
and command-copy holes statically. Mutex-initialization failure is now pinned by
a dedicated interposer too: it asserts exact `-3`, allocation baseline, the
actual attempted slot, exact handle consumption, and recovery.

The POSIX `-5` deadlock is now closed in source and by an adversarial oracle.
Teardown uses unignorable `SIGKILL`, retries `waitpid` across `EINTR`, and reaps
before returning. The test makes the child install a `SIGTERM`-ignore trap and
write a readiness marker *before* the thread interposer refuses the monitor, so
the old polite-TERM implementation cannot win a race and pretend to work. It
then asserts a bounded return, exact `-5`, descriptor restoration, the actual
attempted table slot, `ECHILD`, exact next handle, and a successful recovery
spawn. **That is a fucking test:** it derives every observable assertion from
the no-child/no-resource/no-latched-state clause instead of merely touching the
branch. Static disposition for the POSIX process failure matrix: **accepted**.

The Windows acquisition unwind has now caught up in source: the two
`CreatePipe` calls are split, partial construction closes the first pair, and
`-4`, `-5`, and `-6` all flow through an `abandon_process` that destroys the
mutex and frees the command/record. That closes the concrete leaks previously
reported. `SetHandleInformation` is now checked and unwound too. The `-5` arm
now waits unconditionally after requesting termination, so an already-exited
child and an accepted asynchronous termination both reach the same confirmed
state before its handle is closed. Static disposition for the Windows source
unwind: **accepted**. There is still no Windows parity suite for injected
failure, handle inventory, actual table-slot clearance, confirmed child
termination, and recovery. Proof only for the machine under the author's arse
is not cross-platform regression protection.

The focused spec/code/test chain is now complete at the reference level:
`[BUILTIN-PROCESS-FAILURE]` appears in the specification, the POSIX and Windows
production entries, and the failure suite. The remaining Windows concern is
behavioral evidence, not missing labels.

The in-progress v2 coverage protocol has now fixed the original valid-prefix
lie: it uses an exact header, strict two-column rows, duplicate rejection, a row
count footer, rejection after the footer, incomplete-table refusal, sibling
staging plus rename, and full JSON control-character escaping. Its tests are
loaded with the right assertions: thirteen malformed shapes, report exclusion,
normal child exit on runtime collection failure, diagnostics, published/staging
file absence, exact success shape, and hostile JSON paths. An intermediate
version planted a FIFO at the staging path and then deadlocked because the
writer deleted that FIFO before opening anything. That fucking CI landmine has
now been removed. The replacement uses a directory at the destination to force
`rename` failure deterministically, and asserts clean child exit, the exact
finish diagnostic, preservation of the destination, and removal of staging.
Static disposition: **accepted**.

The worktree now rejects line `0` and pins both zero and negative line numbers.
It also changes staging to exclusive `fopen("wx")`: a symlink recreated in the
unlink/open race already exists, so exclusive creation refuses it rather than
following it and truncating its target. Those production corrections are sound.

The reader now rejects any dump without its final newline, the spec explicitly
requires every record to be terminated, and the table pins an unterminated
footer and final row. That closes the footer-truncation hole statically.

The writer oracle is now byte-exact too. Its earlier `fscanf` accepted tabs,
doubled spaces, and missing row newlines that the Rust reader rejects, allowing
writer and reader tests to stay green while every real dump failed framing. The
current assertion compares each complete row and footer byte-for-byte, including
the one-space separator, registration order, and newline. Static disposition:
**accepted**.

The security correction now has an adversarial regression as well. Its
test-only `remove` interposer reproduces the unlink/open window's observable
state by leaving a symlink at staging, then asserts exact refusal, no published
dump, an untouched symlink, and a byte-identical sentinel. Changing `"wx"` back
to `"w"` would follow the link and destroy the sentinel, so this test actually
fails on the exploit it names. Static disposition: **accepted**.

The first-table-allocation edge is now closed as well. Flush checks
`cov_truncated` before the empty-table return, and a dedicated first-line OOM
case asserts the OOM diagnostic, the named “line table incomplete” refusal, and
absence of a dump, alongside the already-pinned later-growth failure. Static
disposition: **accepted**.

The allocator injector's pointer-range UB has been fixed with `uintptr_t`, and
its counters/one-shot failure handoff are atomic. The documented boundary is
now precise: it may be linked into a threaded suite, but a failure may be armed
only while one thread can allocate. Current arming windows occur before worker
threads exist and assert both return values and live-allocation deltas, so it is
acceptable within that boundary. Do not arm it amid concurrent allocation and
pretend the intended caller necessarily received the failure.

The same shim briefly had a separate bounds bug: `osp_bootstrap_alloc` performed
`OSP_BOOTSTRAP_BYTES - taken` after an unconditional `fetch_add`. One failed
reservation pushed `taken` past the arena; the next subtraction wrapped and
could return a pointer outside the array—the exact opposite of the comment
promising exhaustion returns `NULL`. `bytes + 7` could overflow before alignment
as well. The live source now rejects oversized requests before rounding and uses
a compare-exchange reservation that never advances past the arena. Static source
disposition: **accepted**. The new direct oracle rejects `SIZE_MAX` and the
rounding-overflow near miss, drains the arena with zero-byte requests while
proving every address stays inside and remains distinct, then proves a spent
arena refuses zero, one, and full-capacity requests. The old `fetch_add` code
would push the cursor past the arena on the loop's first refusal and the very
next spent check would return non-NULL, so the arithmetic corpse is genuinely
pinned. Static regression disposition: **accepted**.

The death-test harness now resolves `__gcov_dump` before `fork`, so the specific
loader-lock disaster of calling `dlsym` inside `SIGABRT` is fixed. The false
async-signal-safety claim is gone and the residual libgcov risk is stated
honestly. More importantly, the deadline now lives in the parent: nonblocking
`waitpid` polling ends in `SIGKILL` plus a blocking reap and returns a distinct
`OSP_DEATH_STALLED`. The harness pins both an ordinary infinite body and one
that ignores `SIGALRM` and blocks every signal. A wedged body or gcov handler can
now fail the assertion but cannot wedge CI. Static disposition: **accepted**.

The JSON work now has a real `[BUILTIN-JSON-STRING]` contract: exact escape
alphabet, four hex digits, paired surrogates, raw-control rejection, OOM mapping,
and all-or-nothing ownership. The implementation has correspondingly added
unknown-escape, raw-control, and surrogate validation. The old permissive
`parse_ok("\\x")` oracle has been deleted and replaced by a broad exact-malformed
table covering unknown escapes, bad hex, truncated escapes, both surrogate
boundaries, wrong pairings, raw controls, valid adjacent boundaries, exact UTF-8
bytes, and allocator failure. That part is now serious, spec-shaped evidence.

The punctuation-soup number scanner is now gone. `[BUILTIN-JSON-NUMBER]` states
RFC 8259's exact sign/integer/fraction/exponent production, and `scan_number`
implements those transitions explicitly. The assertion table pins accepted
boundaries, malformed bare/missing components, repeated punctuation, embedded
signs, leading-zero behavior, exact root error codes versus container errors,
exact source-text round trips, and cleanup after rejection. This would fail on
the old scanner. Static disposition: **accepted**.

Escaped U+0000 is now explicitly rejected in spec and implementation because
the runtime ABI cannot represent it. The test pins the whole document, a middle
value (`"a\\u0000b"`), and a key, so prefix truncation cannot masquerade as
success. The literal path now validates UTF-8 rather than blindly copying bytes,
and the spec/test table pins valid two/three/four-byte boundaries, stray
continuations, truncation, bad continuation slots, overlong forms, UTF-8-spelled
surrogates, U+110000, five-byte forms, invalid leads, and keys as well as values.
Static disposition for string grammar, literal UTF-8, number grammar, and NUL
handling: **accepted**.

The HTTP log-label gap is now covered meaningfully in the request unit: it
captures the log and asserts both placeholders survive, the empty `path= ` form
does not, and the placeholder no longer collides with query stripping. The
listener-spawn rollback test has also been loaded properly: after forcing `-6`
it asserts `socket_fd == -1`, `is_listening == false`,
`loop_scheduled == false`, `server_fiber_id == -1`, unchanged same-process
descriptor counts across two attempts, and no extra release during stop. These
tests would fail on the leak and latched-state regressions they name. Static
disposition: **accepted**.

The original VS Code profiler twins test was toothless because stable
`Array.sort` made it survive deletion of the `a.i - b.i` tie-break. The worktree
now exports `byFrameKeyThenIndex` and directly asserts both key directions, both
index directions, the diagonal, and every pair in a three-element equal-key
set. Deleting the tie-break now fails the test. That fixes the oracle.

`[PROF-VSCODE-FLAME]` now states the exact user-visible obligation as well:
colors depend only on the document, frames rank by file, then name, then original
index, and the ordering is total so distinct equal-key frames cannot compare
equal. The implementation and direct comparator assertions now derive from the
same clause. Static disposition: **accepted**.

### CRITICAL worktree blocker — Channel representation and invalid-handle semantics are not closed

The proposal has moved beyond its original direct-identifier hack, which is
good. `HandleSig` now derives the element ABI from inference for `let` bindings,
named-function parameters, and function returns; let-bound aliases and a
receive-before-local-send no longer have to wait for a prior `send` traversal to
learn `T`. Capturing closures clone the full `Value` metadata. Those are real
repairs, not cosmetic churn.

This cycle caught the direct send path stamping a flat-list tag and then sending
a different runtime-list layout. That specific corruption has already been
repaired in the live tree: `listlit::escaping` now materializes first and the
binding is tagged from the value that actually goes on the wire. The correction
is in the right order and the ownership handoff retains the materialized value
before boxing it. Static disposition for the direct-local representation
mismatch: **accepted**. It does not fill the ABI holes below.

The new ownership handoff initially created a deterministic ARC leak on rejected
sends. The live runtime now accepts a managedness bit and consumes the
transferred `+1` on every invalid/unassigned handle path, while successful sends
leave that reference owned by the buffer. The native table pins every
bounds/unassigned route to the same rejection helper. Static source disposition:
**accepted**. The former ARC-backed rejected-send census has now been replaced by
a constructor-rejection test, so there is no managed ARC oracle for this helper.
That becomes harmless if invalid construction is made unrepresentable; until
then, do not claim the rejected-send ownership test matrix is closed.

The receive side initially made the undefined surface even nastier:
`channel_recv(-1)` returns native `-1`; for `Channel<List<T>>`, codegen unboxed
that word to a pointer, while for `Channel<int>` it looked like legitimate data.
The live source now closes the language route correctly: every `Channel(n)` call
uses `channel_create_checked`, which aborts with a diagnostic on every negative
native result rather than returning it as `Channel<T>`. The check is at runtime,
so literals, parameters, allocation failure, and handle exhaustion share it.
Static source disposition: **accepted**.

The red-to-green test direction has improved too: it now covers literal and
function-parameter zero/negative capacities, plus an end-to-end `Channel(1)`
success with value and ARC assertions. One loophole remains in the rejection
oracle: each bad case calls `send` before the forbidden print. A patch that left
the poison constructor intact but made only `send` abort would satisfy it while
direct `recv` still converts `-1` into typed garbage. Put the forbidden print
immediately after construction, before any other channel operation, and assert
the exact fatal diagnostic/capacity/code. Then the test proves the constructor
itself refused the value rather than some later operation tripping over it.

Successful-but-undrained sends initially had the same fatal ownership hole. The
live `Channel` now records managedness, `channel_cleanup` releases every queued
managed word, and codegen emits that cleanup after fiber-result cleanup whenever
channel operations were lowered. The ARC-backed regression sends two distinct
managed lists, receives only the first, asserts the exact received length, and
requires zero live objects at process exit; that specifically leaves one runtime
root for teardown to discharge. The native case additionally proves cleanup
drains the queue, is idempotent, leaves the channel reusable, and does not drain
an unmanaged queue. Static disposition for partially-undrained ARC ownership:
**accepted**. These are finally spec-derived ownership assertions, not gcov
confetti.

The named-record field route has now gained a static recovery seam:
`ctor_field_handle` derives the handle and element owner from the declared field
type, and `gen_field_access` restores them after loading the raw slot. Both
syntax twins now exercise named-function parameters and a concrete
`MatrixHub.channel` field with outer lengths and exact nested edge values.
Static disposition for those two routes: **accepted**.

It is **still not a general fix**. `ObjField` remains only
`(String, LType, Option<String>)`; the field repair looks back into a named
constructor declaration instead of carrying metadata with the value. Generic
record instantiations and anonymous object fields are not pinned and can bypass
that lookup. A named function return restores `FiberSig` at
`expr.rs:1171-1175`, but `fn_ret_owner` calls `owner_name` on the `Channel`
itself—which deliberately returns `None`—instead of restoring the element tag.
A direct `recv(makeNestedChannel())` can consequently recover “pointer” while
still losing the nested-list descriptor. Function-value/lambda parameters have
the same owner hole: `closure::bind_params_from` calls
`incoming_param(..., None)`, unlike the named-function path that supplies
`handle_elem_owner`.

`gen_recv` still falls back to raw `i64` whenever this metadata is absent, and
its own comment still names fields/data structures as corrupt routes. That is the
exact pointer-as-integer failure the patch claims to eliminate, still reachable
through ordinary language constructs while the new spec promises that **every**
`recv(Channel<T>)` returns `T` with its full representation.

The live corpus now proves direct locals, named parameters, and one concrete
named field. There are still no route-specific observable assertions for a
function return, let alias, receive-before-send, function-value parameter,
closure capture, generic record, or anonymous field. Some of those paths look
correct in source; a claim that **all** routes are closed is nevertheless false
on the checked-in assertions. The separate type-checker collateral regression
has been repaired: handle variables are no longer collected through `Type::Fun`,
and safe channel factory plus generic `firstOf(Channel<T>)` counterexamples
prove the value restriction targets stateful **values**. That is meaningful
evidence for the type half, not absolution for the representation/ownership half.

Activating the direct/parameter/concrete-field corpus case is a forward step,
not test weakening; its new assertions are meaningful. Do not use that one pass
to erase the broader known limitation or declare `[CONCURRENCY-CHANNEL]`
complete. Carry `FiberSig` plus element owner/payload metadata through every
field and callable ABI, returns, aliases, and captures; then add spec-derived
observable assertions for every route above. Each corpse
should send at least two structurally distinct nested payloads, receive them in
FIFO order, assert outer and inner lengths plus exact edge values, exercise both
Default and ML syntax, and return ARC live-object/byte counts to baseline after
cleanup. A single “did not crash” or outer `listLength` assertion is nowhere
near enough. Calling 203/203 green while untested ABI paths still strip the
descriptor is weapons-grade fucking self-deception.

## Remaining known limitation

At committed `resume` HEAD,
`tests/core/collections/nested_generic_collections_fibers.test.osp.expectedoutput`
contains one explicit skip: a `Channel<collection>` round trip drops nested
descriptors. It is visible and reasoned rather than silently missing. The live
worktree meaningfully activates and expands that exact case, but returned-channel
element ownership, function-value parameters, generic/anonymous fields, and
invalid-constructor/receive semantics remain lossy or unproven. Channel ARC
teardown itself now has a source path and a partially-drained exit-census oracle.
The specific skip can disappear; the universal limitation cannot disappear from
the review until the entire handle ABI and failure surface are real and proven.

## Final disposition

This branch is a substantial net improvement in scope, regression inventory,
runtime hardening, editor behavior, and coverage ratchets. It is moving forward.

It is also **not ready to merge**. Make coverage evidence fail closed and make
the JSON artifact valid over every discoverable path. Before crediting the live
Channel work, close the ordinary return/function-value/generic-field ABI routes
and stop a rejected constructor from becoming a typed poison handle that `recv`
can reinterpret as user data or a pointer. Pin the new platform-neutral process
failure contract on Windows rather than shipping POSIX-only evidence for both
implementations. Until then, the branch can certify partial coverage as complete
and the worktree can manufacture typed garbage—and that is way too much
dangerous bullshit for a green button.
