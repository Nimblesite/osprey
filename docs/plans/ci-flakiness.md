# CI test flakiness review

Read-only reviews of the test suites for flakiness, made 2026-11-23 by two GLM pis (zai/glm-5.3) and not yet independently verified line by line. Each finding gives file:line, mechanism, a failure scenario and a suggested root-cause fix. Codex owns the open items (handed over on TMC).

## Fix plan

Root-cause fixes only: no retries, no wider timeouts used to hide a race, no skipped tests. Tick an item when its suite has passed twice in a row under `make -j9`, beside its AddressSanitizer twin.

- [x] Fiber spawn under allocation failure: an empty handler stack needs no snapshot, and a snapshot that cannot be allocated refuses the spawn (-2) instead of aborting. Pinned by `test_spawn_with_handlers_survives_every_failed_allocation` (a61b34502).
- [x] GPUI self-test keys reach the canvas beside a docked aside: `tree::inside` visits an aside's columns in drawing order (88dc8d98a). A context menu around a scene passes its free space down (a50913331).
- [x] The deleted keyboard dock's steps are gone from Prism's `smoke.jsonl` and `self-test.json`, and its goldens are deleted (88dc8d98a).
- [x] **Fixed loopback ports ([CI-TEST-PORTS]), runtime finding 1 and more.** `make -j2 _test_c_runtime` runs a suite beside other suites and beside its own ASan twin, and they all bind the same fixed ports. Those are 18085–18101 (including `ORIGIN_PORT` in `http_runtime_tests.h` and `WS_LIVE_PORT` and `WS_DEAD_PORT` in `websocket_runtime_tests.c`), plus listeners on 8080, 8082, 8083 and 19001.
  - Fix: `test_port(offset)` in `test_loopback.h` returns a port from a block of 24 chosen once per process. It starts at a pid-derived block from 20000 and takes the first block whose ports are all bindable.
  - Keep the base in `OSP_TEST_PORT_BASE` with `setenv`. A per-translation-unit `static` let the origin and its client pick different blocks, and `test_client_chunked_response` hung in `origin_stop`.
  - Every fixed port becomes a unique offset. URLs are built with `snprintf`.
  - Status: landed on 2026-11-23. The block is chosen in a constructor before main, so no thread or forked child picks its own. Verified by `make -j4 _test_c_runtime` (coverage and ASan twins side by side) on Ubuntu 26.04 with GCC 15.2, green, and the http, net and websocket suites in parallel on macOS.
- [ ] Runtime findings 2–11 and the UI findings below, in the order listed. Each fix replaces a sleep or a deadline with a signal the test waits on:
  - wall-clock bounds in `http_read_tests.c`
  - the dribble margin
  - sleep-then-assert in `effects_runtime_tests.c` and `fiber_channel_tests.c`
  - the SIGTERM-before-handler race in `websocket_runtime_tests.c`
  - one-second loopback ceilings
  - disconnect polling
  - the DNS `.invalid` assumption
  - ports assumed closed (18099 and port 1)
  - the WinUI 300 s kill against 90 s per step
  - the `build.py` deadlines
  - Playwright `waitForTimeout`, with `retries: 1` masking flakes

## Open issues blocking PR #428 (2026-11-23 hand-off)

- [ ] **Duplication gate red.** `make _deslop` reports 4.2% (23471 of 560981 lines) against the 3.996181% ceiling in `.deslop.toml`: about 1,050 duplicated lines must go, and the ceiling only goes down. Most of the excess arrived since the last green run (cacb69095):
  - `daw/test/main.ospml`, `voice.ospml`, `room.ospml` and `history.ospml`
  - the cleanarch_todo `multishot_*_cases.osp`
  - `osprey-cli/tests/common/many_programs.rs`, `staged_multiplicity.rs` and `staged_many.rs`

  The CleanArch suite must retain all 100 passing tests in both GC and ARC.

  Progress on 2026-11-23:
  - FerroUI is now measured, which raised the figure to 5.0%. It shares GPUI's toolkit-free modules (events, regions, tree, store, ffi, frame, the linked app symbols, the file and process adapters, and the ffi tests). Only its view, host, surface and scene are its own.
  - Benchmarks and `docs/book/` are excluded, because their programs stand alone.
  - Shared helpers were added to the C process table, the workbench kernel trees, the synth tests, the Prism tests and the cleanarch plan cases.
  - The figure is back to about 4.2%. Three GLM pis are deduplicating the collection corpus, the cleanarch multishot cases and the native tests.

  Some clones that remain cannot be merged in today's Osprey. These syntax proposals would let them merge:
  - **Operation-polymorphic handler factories** ([EFFECTS-HANDLER-VALUE]). `direct_recovery` and `resume_lifo_audit` repeat one arm shape per operation: count, log, `resume`, log, transform. That happens because a handler value names one fixed effect and `resume` cannot leave its arm. A factory parameterised by an operation reference, such as `audited(Alpha.alpha, "a", 10, |n| => n + 1)`, would install each arm from one definition.
  - **A shared corpus test prelude.** Each `src/osprey/tests/` program stands alone, so helpers such as `twice`, `isEven` and the fiber/handler fixtures are copied between programs. Two options would let them be defined once: an importable `tests/support` module, or more helpers in the prelude's `testing.osp`.
  - **Derived JSON codecs from typeDiagram models.** The workbench `save`/`restore` pairs (`store.ospml`, `tutor.ospml`) and every app's `*Json`/`*Of` pair are field-by-field mirrors. Generating them from the model, as AGENTS.md asks, would delete both sides.
  - **One generic UI test bench** (`uitest::Bench S A`). daw, signal_flow, casting, painting, flock and uitest each declare their own `Bench` effect and the same `post`/`pointer`/`tap`/`element`/`offers` wrappers. A generic effect in `uitest` needs effect type parameters to work across packages in ML. This was not tried yet.
- [x] **CleanArch retry and page tests.** The rival queue consumes one writer at a time, including duplicate titles, and the authorization, pagination and validation expectations match their fixtures and branch rules. All 100 cases pass under GC and ARC on the PR head.
- [ ] **Windows Prism docked panel:** a white toolbar strip where Undo and Redo belong, and a light-theme panel beside the dark canvas. Logged as [daw.md](daw.md) item 40. The 100% goldens record the current look; the 150% goldens still show the deleted keyboard dock.
- [ ] **Canvas scale with the panel docked.** The user said "scale a bit off". With [DAW-LAYOUT-FIXED-SCALE] at one unit per point, a 1152-point window with the 320-point panel hides the right of the patch. Decide whether a docked panel narrows the page or the canvas fits its pane.
- [ ] **CPython in CI.** `_prism_ui_test` and `_prism_vst_ui_test` run `python3 src/toolchain/hosts/synth_vst/build.py`, which AGENTS.md bans. Port it to Python-flavour Osprey.
- [x] **Rust lowering growth.** Only programs that declare a multi-shot operation need captured-state analysis. The gate is in the pushed commit `b87b7e245`. The 4,000-caller fixture fell from 4.02 s to 0.69 s locally, and the budget, staged-many and multiplicity tests passed.
- [ ] **Multi-shot review leftovers** ([MULTI-HANDLE-MANY]):
  - compile time grows about cubically with many resumes in one arm, and overflows the Rust stack near 10,000
  - resume is capped near 512 by the syntax nesting limit
  - the native stack overflows at a depth of 8,000
  - false rejections #5 and #7 (a closure built from resume results; a handler value passed as a parameter) came back when 113404804 was reverted, because it broke cleanarch_todo
- [ ] **The full `make ci` was not run before the 88dc8d98a push.** The failing jobs were reproduced and fixed locally one by one instead.

## CI runtime

- [ ] `Check: every Osprey program compiles` is the longest job (17 min; 994 s for the step in run 45027767330). It ran 600+ compiles in batches of 16, and each batch waited for its slowest member. `scripts/ci/check-programs.py` now keeps 16 in flight through an `xargs -P` pool, and each job writes its own exit code: 607 programs, 0 failed, in 4 min 55 s locally. Compare its CI time with that run. Split the programs across runners only if the pool alone does not bring the job under the Rust workspace's 17 min.
- [ ] `Tests: Rust workspace (rest)` (17 min) and WinUI (16 min) are the next critical paths. macOS jobs also wait for runners: iOS Inbox started 13 min late.

## Runtime test-suite flakiness review (read-only)

Scope: `src/toolchain/compiler/runtime/*_tests*.c|h`, their make recipes in `scripts/make/tests.mk`, and `src/toolchain/crates/osprey-cli/tests/common/*.rs`. Everything below was verified by reading the code; no builds/tests were run and no repo files were touched.

Key context verified up front: CI runs the C suites **concurrently** — `.github/workflows/ci.yml:793` is `make -j2 _test_c_runtime`, and `scripts/make/tests.mk:227` declares `_test_c_runtime: $(C_TEST_TARGETS) $(C_ASAN_TEST_TARGETS)` with no ordering between suites. With `-j2`, two suite binaries run at the same time on one machine (both in the coverage phase and in the ASAN phase, `tests.mk:217`). Every timing window below is therefore measured on a machine that is deliberately 2-suite-loaded.

---

## 1. Port 18094 is bound by two different suites that CI runs in parallel — HIGH

- **Where:** `src/toolchain/compiler/runtime/http_server_live_tests.c:23` (`#define KEEP_ALIVE_TEST_PORT 18094`, bound at `:442` inside `test_keep_alive_waits_for_its_listeners`, part of the `http_runtime_tests` suite, `tests.mk:170`) and `src/toolchain/compiler/runtime/net_runtime_tests.c:236` (`listen_loopback(18094)` in `test_tls_plain_listener_fails_handshake`, part of the `net_runtime_tests` suite, `tests.mk:196-199`).
- **Mechanism:** both suites bind the same fixed loopback port. In the suite list (`tests.mk:92-100`) `http_runtime_tests` and `net_runtime_tests` are three positions apart, and both are long-running, so under `make -j2` they overlap easily. The ASAN twins make it worse: `C_ASAN_SUITES = fiber http net websocket` (`tests.mk:217`), so `_test_c_asan_http_runtime_tests` and `_test_c_asan_net_runtime_tests` are adjacent prerequisites and run concurrently for most of their (slow, ASAN-instrumented) duration. `net` binds 18094 at its 5th test (`net_runtime_tests.c:274`), mid-suite — inside the overlap window.
- **Failure scenario:** while the http keep-alive server holds 18094, `net`'s `listen_loopback(18094)` hits `assert(bind(...) == 0)` (`net_runtime_tests.c:126`) and the whole net suite aborts. Conversely, if `net` binds first, `assert(http_listen(quiet, keep_alive_handler) == 0)` (`http_server_live_tests.c:443`) fails with -4, or `osprey_tls_connect(LOOPBACK, 18094, 500)` (`net_runtime_tests.c:237`) handshakes against the HTTP server and returns -6/-4 instead of the asserted -7. No code change is needed for any of these — only the interleaving.
- **Fix (root cause):** give every suite an OS-assigned ephemeral port (bind port 0, read back `getsockname()`), or partition fixed ports per suite and document the allocation; at minimum move `net`'s plain-listener test off 18094.

## 2. Wall-clock upper-bound assertions on timeout machinery — HIGH

- **Where:** `src/toolchain/compiler/runtime/http_read_tests.c:26-33` (`expect_bounded_failure`: `assert(took >= timeout_ms * HTTP_READ_BOUND_FACTOR - 100)` / `assert(took < timeout_ms * (HTTP_READ_BOUND_FACTOR + 5))`, `HTTP_READ_BOUND_FACTOR = 10` per `http_client_internal.h:16`), called with 100 ms from `check_whole_read_bound` (`:200-218`), with 300 ms from `check_handshake_bound`/`check_send_bound` (`:414-455`), and the TLS twin `http_tls_tests.c:318` (`took >= 900 && took < 1500` around a 100 ms-timeout dripped TLS read).
- **Mechanism:** the test measures wall-clock time around a read that is *supposed* to end at `timeout × 10` on the monotonic clock. The 100 ms cases allow only a 900–1500 ms window — 500 ms of slack for connect + request + drip-thread scheduling + the main thread being rescheduled to observe the deadline, on a runner already loaded by a sibling suite (and 2–3× slower under the ASAN build of the same suite).
- **Failure scenario:** the client's internal deadline trips at 1000 ms, but the test process is starved for >500 ms before `elapsed_ms_since(started)` runs → `took < 1500` fails → suite aborts although the code under test behaved exactly as specified.
- **Fix:** assert on the *mechanism*, not the wall clock (e.g. expose/verify the deadline the transport computed, or count poll wakeups), or widen the window to a factor that no realistic scheduler stall can cross (e.g. `timeout × 50`), keeping only the lower bound tight.

## 3. Dribbled response must *succeed* inside the whole-read bound with ~360 ms of slack — MEDIUM/HIGH

- **Where:** `src/toolchain/compiler/runtime/http_read_tests.c:220-223`: `slow = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok"` (41 bytes) sent one byte per `usleep(40 ms)` (`origin_drip_us`, `http_origin.c:105-110` sleeps after **every** byte) against a client with timeout 200 ms, i.e. a 2000 ms whole-read bound.
- **Mechanism:** 41 drips × ≥40 ms = ≥1640 ms mandatory floor (usleep never returns early; each call typically overruns by a scheduler quantum), versus the 2000 ms bound — ~360 ms total margin. Accumulated usleep overruns under load (41 × ~10 ms is ordinary on a loaded box) push the last byte past 2000 ms.
- **Failure scenario:** the read is cut at the bound and returns -16/OVER_TIME instead of 200/“ok” → `EXPECT_RESPONSE(slow, 200, "ok")` (`:223`) fails with a wrong-error-code abort, with no code change. The follow-up at `:230` (`expect_cut_off("HTTP/1.1 200 O", 200)`, 14 drips × 150 ms = 2.1 s vs the 2.0 s bound, comment says “1.95 s of 2 s”) has only 50 ms of margin by the author's own arithmetic, though that case fails-open into the expected error.
- **Fix:** derive the drip count from the timeout so the success case always lands at ~70% of the bound regardless of scheduling (e.g. `drips = bound_ms * 7 / 10 / drip_ms`), or drive the origin by a monotonic deadline rather than per-byte sleeps.

## 4. Sleep-then-assert another thread made progress (50 ms) — MEDIUM

- **Where:** `src/toolchain/compiler/runtime/effects_runtime_tests.c:367` (`QUEUE_PARK_US 50000`), `:383-385`: `pthread_create(...); usleep(QUEUE_PARK_US); CHECK(g_queued_entered == 1);`.
- **Mechanism:** a hard assert that a freshly spawned thread has *entered* a coroutine suspend within a fixed 50 ms. The comment (“Long enough for a created thread to reach suspend's in-flight wait”) encodes an assumption about scheduling speed, not a guarantee.
- **Failure scenario:** on a 2-core runner already executing a sibling suite (see context above), the new thread doesn't get a timeslot within 50 ms → `g_queued_entered == 0` → CHECK aborts the suite.
- **Fix:** have `queued_performer` signal a condition variable/atomic when it enters, and wait on that (bounded by a generous deadline) instead of a bare sleep.

## 5. SIGTERM can arrive before the keep-alive handler is installed (300 ms race) — MEDIUM

- **Where:** `src/toolchain/compiler/runtime/websocket_runtime_tests.c:407-410` (`ws_shutdown_signaller`: `usleep(300000); kill(getpid(), SIGTERM)`), `:416-420` (`test_keep_alive_until_signal` creates that thread, *then* calls `websocket_keep_alive()`), and `src/toolchain/compiler/runtime/websocket_server_runtime.c:446-450` where `websocket_keep_alive` installs `signal(SIGTERM, handle_shutdown_signal)` only after entry.
- **Mechanism:** if more than 300 ms elapse between `pthread_create` and the main thread reaching the `signal()` calls inside `websocket_keep_alive` (descheduling under load), the SIGTERM is delivered with the **default** disposition.
- **Failure scenario:** the whole test binary is killed by SIGTERM mid-run → the websocket suite (coverage *and* ASAN variants) reports a signal death with no failing assertion, and `make` fails.
- **Fix:** install the handler (or otherwise arm the shutdown latch) before spawning the signaller, or use a handshake that waits until `websocket_keep_alive` has installed its handlers before starting the 300 ms timer.

## 6. `fiber_done == 0` asserted within a 100 ms fiber lifetime — MEDIUM

- **Where:** `src/toolchain/compiler/runtime/fiber_channel_tests.c:191-194` (`test_cleanup_defers_while_a_fiber_runs`: `fiber_spawn(slow_function); assert(fiber_done(slow) == 0 && "the fiber must still be running")`), with `slow_function` = `usleep(100000); return 999;` at `src/toolchain/compiler/runtime/fiber_runtime_tests.c:22-25`.
- **Mechanism:** the test asserts the fiber is *not yet* finished, with no synchronization other than “the main thread will get here within 100 ms”.
- **Failure scenario:** the spawned thread runs its 100 ms sleep to completion while the main thread is stalled >100 ms after `fiber_spawn` returns (loaded runner) → `fiber_done` returns 1 → assert aborts. Also invalidates the follow-up cleanup assertions.
- **Fix:** make the fiber block on a channel/condvar the test releases (assert-not-done is then stable for as long as the test wants), or lengthen the sleep by an order of magnitude and accept the weakened guarantee — the blocking primitive is the correct fix.

## 7. 1-second wall-clock deadlines on loopback exchanges and server stop — MEDIUM

- **Where:** `src/toolchain/compiler/runtime/http_server_connection_tests.c:20` (`PROMPT_MS 1000`), asserted at `:140` (connect + full request/response round trip must finish), `:184` (half-close + reply + close), and `:204` (`http_stop_server` + close detection).
- **Mechanism:** each is a wall-clock ceiling over real socket traffic and thread-pool scheduling of the server runtime, measured while a second suite occupies the machine (see context).
- **Failure scenario:** a >1 s scheduling stall anywhere between `now_ms()` bookmarks → assert aborts with “a silent client must not stall the next one” even though the server serialized handlers exactly as designed.
- **Fix:** assert ordering/liveness by observation (e.g. the reply arrives, the second client is served *at all*, the socket closes) rather than within a fixed second; if a regression guard on latency is truly wanted, gate it on an unloaded-machine marker or raise it far above scheduler noise.

## 8. 3-second bounded retry for server-side disconnect detection, then hard assert — MEDIUM

- **Where:** `src/toolchain/compiler/runtime/websocket_lifecycle_tests.c:14-15` (`WS_POLLS = 300`, `WS_POLL_US = 10000`), `:43-50` (`broadcast_until` retries ≤3 s), asserted at `:74-79` (`broadcast_until(server, 1, &frames) == 1` right after `close(leaves)`), `:118-121`, `:152-153`, and `:163`.
- **Mechanism:** after the test closes a client socket, the *server's* reader thread must be scheduled, observe EOF/`recv == 0`, and remove the connection within 3 s. `websocket_server_broadcast` keeps counting the dead connection until then.
- **Failure scenario:** on a starved 2-core runner the server thread isn't scheduled for >3 s → `broadcast_until` returns 2 → `assert(... == 1)` aborts the suite. The loop runs 10 rounds (`:65`), multiplying exposure.
- **Fix:** raise the retry budget for the “server noticed the disconnect” waits (this is environment synchronization, not the behavior under test), or add an explicit quiesce hook (e.g. a test-only `websocket_server_wait_idle`) instead of polling broadcasts.

## 9. DNS resolution of `no-such-host.invalid` must fail — MEDIUM (environment-dependent)

- **Where:** `src/toolchain/compiler/runtime/net_runtime_tests.c:30` (`NOWHERE = "no-such-host.invalid"`), asserted at `:224` (`osprey_tls_connect(NOWHERE, ECHO_PORT, 100) == -2`), resolution via `getaddrinfo` in `src/toolchain/compiler/runtime/socket_safety.h:87-105`.
- **Mechanism:** the test requires the resolver to return no A record. `.invalid` is reserved (RFC 2606) but the code path still consults the system resolver: search-domain suffixing, captive portals, and corporate wildcard resolvers can answer.
- **Failure scenario:** on a network whose resolver returns an address for any query, the dial proceeds and returns -4/-6 instead of -2 → assert aborts the net suite on that machine only.
- **Fix:** assert the unresolvable-host contract against a name the resolver provably cannot answer without network involvement — interpose `getaddrinfo` in the test binary (the suite already interposes other libc functions elsewhere, cf. `random_entropy_tests.c`) — or skip the -2 expectation when resolution unexpectedly succeeds, loudly.

## 10. Fixed ports assumed free on the host — MEDIUM/LOW

- **Where:** `src/toolchain/compiler/runtime/net_runtime_tests.c:223` (`osprey_tls_connect(LOOPBACK, 18099, 500) == -4` — requires *nothing* on the host to listen on 18099) and `src/toolchain/compiler/runtime/websocket_runtime_tests.c:355` (`websocket_connect("ws://127.0.0.1:1/chat") == -6` — same assumption for port 1).
- **Mechanism:** both asserts conflate “connection refused because nothing listens” with “the code correctly reports refusal”; any unrelated process on the machine holding that port changes the outcome.
- **Failure scenario:** a developer's dev server (or another CI container sharing the network namespace) listens on 18099 → the dial connects or times out differently → -6/-7/-8 instead of -4 → abort. Port 1 additionally requires privileges to bind, but nothing prevents a root daemon from having it.
- **Fix:** for the refused-dial contract, bind a listener, `close()` it, and immediately dial the port it held (still racy but self-contained), or pick a port via bind(0)+close inside the test and accept the tiny race consciously; better, assert on a refused dial to a port the test itself knows is closed.

## 11. 5-second retry budget for the profiler session handoff — MEDIUM/LOW

- **Where:** `src/toolchain/compiler/runtime/profiler_stream_test_support.c:16` (`TEST_RETRIES = 500` × 10 ms), `:154-165` (`test_retry_start` asserts the endpoint released the previous connection's CPU session within ~5 s), used by `profiler_stream_tests.c:150` and `profiler_stream_lifecycle_tests.c:110`.
- **Mechanism:** after `test_close(&a)`, the endpoint thread must notice the disconnect and end the session before 500 retries elapse; the deadline is a fixed 5 s of *test-thread* wall time while the endpoint thread competes for CPU.
- **Failure scenario:** a starved runner delays disconnect detection >5 s → `assert(!"the endpoint never released the session")` fires.
- **Fix:** make the retry budget generous (30 s+) since only ordering matters, or expose a synchronous “session released” observation from the endpoint.

## 12. Rust scheduler test requires genuine thread overlap within a 2 s deadline — MEDIUM/LOW

- **Where:** `src/toolchain/crates/osprey-cli/tests/common/package_matrix.rs:59-70` (`overlap_job` spins up to `Duration::from_secs(2)` waiting for `active >= 2`) and `:100-107` (`verify_overlap` asserts `peak >= 2`).
- **Mechanism:** the test proves the matrix scheduler runs jobs concurrently by waiting a fixed 2 s for a second worker thread to be scheduled; the assertion fails if the machine can't schedule two worker threads inside that window (RUST_TEST_THREADS=4 + cargo llvm-cov + the rest of the workspace running concurrently).
- **Failure scenario:** heavily oversubscribed CI runner → peak stays 1 → “scheduler ran serially” aborts an otherwise correct scheduler.
- **Fix:** treat “no overlap within the deadline” as a skip-with-loud-message (the ordering assertions above it are the real contract), or extend the deadline by an order of magnitude.

## 13. 120-second run deadline and RSS polling in `bounded.rs` — LOW

- **Where:** `src/toolchain/crates/osprey-cli/tests/common/bounded.rs:14-16` (`RUN_DEADLINE = 120 s`, `MEMORY_BUDGET_MB = 1024`), `:41-53` (50 ms polling loop; each poll spawns `ps`).
- **Mechanism:** a legit compile+run must fit 120 s on the current machine; the measurement loop itself adds `ps` fork cost every 50 ms, and on a loaded runner both the child and `ps` slow down together.
- **Failure scenario:** a cold-cache or heavily loaded runner exceeds 120 s for a healthy program → `stopped: Some(...)` → callers that assert `stopped.is_none()` fail without any code defect.
- **Fix:** scale the deadline (or make it configurable/CI-aware), and poll `ps` less often (e.g. every 500 ms) so the watcher doesn't compound the load it measures.

## 14. Assorted wall-clock ceilings around loopback exchanges — LOW

- **Where:** `src/toolchain/compiler/runtime/http_read_tests.c:44-46` (`expect_prompt`: every framed exchange must finish < 1500 ms), `:405-407` (`expect_linear_scan`: a 16 MB scan must finish < 20000 ms — also executed in the ASAN build), `src/toolchain/compiler/runtime/http_tls_tests.c:310` (two TLS handshakes + exchanges < 3000 ms).
- **Mechanism:** all three are upper bounds on elapsed monotonic time for real work under whatever load the machine carries; none has any lower-bound or mechanism counterpart that would catch the regression they aim at if the ceiling were removed.
- **Failure scenario:** a transient multi-second stall (suite neighbor, gcov flush, ASAN overhead) crosses the ceiling → assert aborts.
- **Fix:** replace with functional assertions (response correct, scan result correct) or ceilings sized like `min(20 × median, hard cap)`; at minimum multiply by ASAN's slowdown factor for the sanitizer build.

## 15. Fixed shared `/tmp` paths in the file-runtime suite — LOW

- **Where:** `src/toolchain/compiler/runtime/test_file_runtime.c:26` (`/tmp/osprey_file_runtime_test.txt`), `:54` (`/tmp/osprey_file_runtime_fifo`), `:190` (`/tmp/osprey_file_runtime_dir`), `:278` (`/tmp/osprey_nul_seeded_reason`).
- **Mechanism:** the suite uses process-shared fixed names in the global `/tmp`. It cleans up on success (`remove`/`rmdir`), but a previously killed run (or a concurrently running second instance — e.g. a developer invoking the suite while CI holds the same machine) leaves or races those names: `test_read_file_reports_a_stream_error` does `rmdir(FILE_DIRECTORY_PATH)` then `assert(mkdir(...) == 0)` (`:239-241`), which fails if the leftover dir is non-empty or a racing instance recreated it in between.
- **Failure scenario:** stale non-empty `/tmp/osprey_file_runtime_dir` from an aborted run → `rmdir` fails silently (cast to void) → `mkdir` returns EEXIST → assert aborts; or two instances interleave writes on the fixed file and one reads the other's content.
- **Fix:** root every fixture in a `mkdtemp` directory (the NUL tests at `:283-291` already do exactly this — extend that pattern), or key the paths by pid.

## 16. Whole-job 14-minute timeout over ~39 suite runs — LOW (CI-level)

- **Where:** `.github/workflows/ci.yml:771` (`timeout-minutes: 14` on the job that runs `make -j2 _test_c_runtime`, `:793`) plus `tests.mk:92-100/217` (39 coverage suites + 4 ASAN suites, several of which deliberately sleep for seconds each — findings 2, 3, 5, 7, 8 above).
- **Mechanism:** the suite set has a fixed sleep floor of tens of seconds even on an idle machine, and the ASAN twins run 2–3× slower; the entire set must fit 14 minutes with only `-j2`.
- **Failure scenario:** a slow GitHub runner (or one where several of the timing windows above stretch out) exceeds 14 minutes → the job is cancelled mid-suite and reported red with no assertion to name.
- **Fix:** measure the p95 duration and size the timeout with headroom, or split the ASAN suites into their own job/step so a slow sanitizer run can't take the coverage results down with it.

---

### Notes on things checked and found *not* flaky (for the record)

- The profiler endpoint suites use pid-keyed sockets under a private `mkdtemp` TMPDIR (`profiler_stream_test_support.c:14-25, 64-70`) — no cross-suite socket collision.
- `media_render_runtime_tests.c:400-414` stages its fake tools in a `mkdtemp` dir and overrides `PATH` — isolated.
- Rust temp helpers key directories by pid (+ thread id in `staging.rs:56-59`), so parallel tests don't collide.
- `fiber_handle_tests.c`'s 12000-fiber probe, the websocket byte-dribble pacing (`websocket_frames_tests.c:146`), the death-test 30 s budget (`test_death.h`), and `wait_for_start`'s 30 s poll (`media_render_runtime_tests.c:102-107`) all have either no hard deadline on their outcome or budgets far above scheduler noise.
- Suites that could plausibly collide on their *other* ports (18085–18101) each own distinct numbers; the single cross-suite clash found is 18094 (finding 1).

## UI / app-harness flakiness review (read-only)

Repo: `/Users/christianfindlay/Documents/Code/osprey`
Scope: UI self-tests (`self-test.json`, `smoke.jsonl`), GPUI self-test host, WinUI `*.ps1` harnesses, `synth_vst/build.py`, VS Code extension tests, website Playwright tests, and the test steps of `gpui.yml` / `winui.yml` / `ci.yml`.
Method: source reading only. Every finding below was verified against the cited lines. Positive notes: the GPUI windowless smoke/self-test driver is fully deterministic (fixed `FRAME_SECONDS` deltas, inline capability answers), and the WinUI host's `Steady` clock (`Session.cs:36-45, 275-291`) makes granted-frame counts machine-independent — those designs are *not* flagged.

---

## 1. WinUI self-test: every golden screenshot is preceded by a fixed 400 ms sleep — HIGH
- **Where:** `src/toolchain/hosts/winui/OspreyHost/SelfTest.cs:29` (`Settle = 400ms`), `:157` (`await Task.Delay(Settle)` in `Shoot`), `:309` (same after `ScrollIntoView`), and `:128-143` (`Shot` gives up waiting for the drawn frame after `Undrawn = 3s` and captures anyway).
- **Mechanism:** sleep-as-synchronisation. After a step's state is satisfied, the harness unconditionally sleeps 400 ms and then captures `-content.png` / `-screen.png`, which `goldens.ps1` compares pixel-by-pixel. WinUI entrance/theme animations, flyout close animations and list-layout settle times are not bounded by the step's `wait`; the 3 s "undrawn frame" wait explicitly "gives up after a moment instead of failing", so a slow machine captures a blank/mid-animation frame and the failure surfaces later as a golden diff.
- **Failure scenario:** on a loaded windows-latest runner (WARP CPU rendering, cold MSYS2/dotnet caches), a `shot` right after an `invoke` that opens a dialog or populates a ListView catches the entrance animation at frame ~10 instead of finished. >0.2 % of pixels differ → `goldens.ps1` throws, no code changed.
- **Fix (root cause):** replace the fixed `Settle`/`Undrawn` sleeps with event-driven readiness: capture only after `CompositionTarget.Rendering` has delivered N consecutive frames whose readback hash is stable (or after the XAML `TranslateTransform`/animation timeline reports `Completed`), and fail with a clear timeout instead of silently capturing after 3 s.

## 2. WinUI `self-test.ps1`: raising the window silently gives up after 30 s — HIGH
- **Where:** `src/toolchain/hosts/winui/self-test.ps1:62-68` — `Show-Window` polls `MainWindowHandle` for 300 × 100 ms, then `if ($Process.MainWindowHandle -eq 0) { return }` with no error; `SetForegroundWindow`'s return is also discarded (`:71`).
- **Mechanism:** a bounded wait that fails open. If the host takes >30 s to create its window (cold publish output, Defender scanning the fresh Native AOT exe, slow AppContainer registration), the harness never foregrounds the window but continues the run.
- **Failure scenario:** WinUI "draws nothing in a window that is never shown" (the script's own comment), so every subsequent `wait` burns its 90 s patience and the run dies at the `-Seconds 300` deadline, or — worse for on-screen goldens — the window stays behind another surface and `-screen.png` pictures the wrong content, failing in `goldens.ps1` far from the cause.
- **Fix:** make the 30 s handle wait a hard failure (`throw "the host opened no window within 30 s"`), and fail the run when `SetForegroundWindow` returns false rather than discarding it.

## 3. Screenshot goldens compare pixels with tight tolerances on machine-variable rendering — MEDIUM-HIGH
- **Where:** `src/toolchain/hosts/winui/goldens.ps1:20-21` (`-Tolerance 24` per channel, `-Fraction 0.002`), `:106-127` (`Near` allows only a 1-pixel shift), `:66` (`Slack = 8` alignment search for on-screen shots), plus `src/toolchain/hosts/winui/OspreyHost/SelfTest.cs:162-166` (`Capture.Screen` of the live desktop).
- **Mechanism:** pixel comparison sensitive to font rasterisation, GPU/driver and compositor state. The `-content.png` goldens were recorded on one Windows image; ClearType/DPI/WARP differences, Segoe UI variable-font hinting changes from a runner image update, or Mica backdrop tint differences (estimated as the screen's mean colour, `goldens.ps1:112-124`) can shift text subpixel rendering beyond ±24/channel + 1 px. The `-screen.png` comparison additionally includes whatever Windows composes over the window at capture time (toasts, focus rings); only rectangles listed in `live.json` are excluded.
- **Failure scenario:** a routine `windows-latest` image bump changes font smoothing defaults; every app's text-heavy goldens drift past 0.2 % of pixels and the whole winui job goes red with no code change (winui.yml "Every app sandboxed in a real window, against its screenshot goldens" step).
- **Fix:** compare perceptual/structural hashes of text-bearing regions (or DOM/accessibility-tree assertions plus coarse layout rectangles) instead of raw pixels; keep raw-pixel comparison only for the app's own scene canvases, and assert on a machine-pinned font/smoothing configuration before capturing.

## 4. WinUI `self-test.ps1`: fixed 300 s wall-clock kill for whole scripted journeys — MEDIUM
- **Where:** `src/toolchain/hosts/winui/self-test.ps1:17` (`[int] $Seconds = 300` default), `:83-86` (`WaitForExit($Seconds * 1000)` → `Stop-Process -Force` + throw). Per-step patience is 90 s (`src/toolchain/hosts/winui/OspreyHost.Core/Launch.cs:66`), and `app.ps1`'s `Test-Package` never passes `-Seconds`.
- **Mechanism:** a short deadline on slow CI. The DAW self-test (`src/osprey/examples/daw/self-test.json`) has ~60 steps, each `shot` adds `Settle` + up to 3 s `Undrawn` + three PNG captures; only a handful of slow waits are needed before 300 s of wall time is gone, even though every step would pass given its own patience.
- **Failure scenario:** on a contended runner several `wait`s take 10-30 s, the process is killed at 300 s mid-script and reported as "The window did not finish its self-test in 300 seconds" — a failure with no code change. (The storage drive run avoids this only because winui.yml passes `-Seconds 480` explicitly.)
- **Fix:** budget the deadline from the script (e.g. `steps × patience` + capture allowance) or make the kill advisory (collect diagnostics, then fail with the unfinished step name) rather than a hard 300 s.

## 5. `synth_vst/build.py`: 3 s deadline for worker/process teardown plus PID-reuse liveness probe — MEDIUM
- **Where:** `src/toolchain/hosts/synth_vst/build.py:339-352` — `deadline = time.monotonic() + 3`; loop does `os.kill(identity, 0)` with `time.sleep(.02)`; asserts `not identities` ("Stop or host shutdown left owned workers alive").
- **Mechanism:** wall-clock deadline on process teardown + PID liveness. Three seconds is not a correctness bound for the OS reaping four processes on a loaded machine; and `os.kill(pid, 0)` succeeding can also mean the PID was already reused by an unrelated process.
- **Failure scenario:** the Prism live journey finishes but the host's owned VST workers take 3.5 s to exit under CI CPU contention (or a fresh process reuses a worker PID) → `check_prism_live` fails "left owned workers alive" with no code change.
- **Fix:** have the worker write a PID-named exit sentinel (or wait on the actual child handles the recorder holds, via `subprocess.Popen.wait(timeout=…)`) instead of probing recycled PIDs against a 3 s wall clock; scale the deadline or poll on an event, not time.

## 6. `synth_vst/build.py`: `time.sleep(.25)` synchronises the audio-queue starvation assertion — MEDIUM
- **Where:** `src/toolchain/hosts/synth_vst/build.py:362-372` — after enqueueing three 2048-frame waves, `time.sleep(.25)`, then asserts `receipt["emptyCallbacks"] > 0`.
- **Mechanism:** sleep-as-synchronisation. The assertion *requires* the device callback to have starved within 250 ms of wall time; whether the audio thread has drained the 6144-frame queue and hit an empty callback in that window depends entirely on scheduler timing.
- **Failure scenario:** on a loaded runner the audio callback thread doesn't get scheduled within 250 ms (or drains slower than expected), `emptyCallbacks` is still 0 → `assert receipt["emptyCallbacks"] > 0` fails; rerun passes.
- **Fix:** poll the worker for the drain/queue-empty diagnostic (`audio.queue_empty` on stderr, or a receipt field) with a real deadline instead of a fixed sleep before a single assertion.

## 7. `synth_vst/build.py`: 5 s per-response deadline on real VST graph blocks — MEDIUM
- **Where:** `src/toolchain/hosts/synth_vst/build.py:89` — `reader.submit(child.stdout.readline, …).result(timeout=5)` in `session_audio` (also `:365, :371, :375` at 5 s and `:385` at 5 s in the recorder/device paths).
- **Mechanism:** short deadline on slow CI. Each response requires the worker to process a real VST3 graph block (JX10 + ADelay chain) and hex-encode up to 4 MiB; under CPU contention (the prism UI journey runs beside it) 5 s can be exceeded even though the pipeline is correct.
- **Failure scenario:** `TimeoutExpired` from `.result(timeout=5)` on a merely slow block → `session_audio` raises, `stream_tests`/`check_prism_audio` fail, rerun passes.
- **Fix:** derive the deadline from the payload size (bytes/second floor measured once) or raise it for the CI profile; treat timeout as "retry once, then fail" only if the retry also misses, so slowness is distinguishable from hangs.

## 8. Website: scroll-spy asserted after a fixed 400 ms sleep — MEDIUM
- **Where:** `src/toolchain/website/tests/interactions.spec.js:232-236` — `page.evaluate(() => window.scrollBy(0, 1200)); await page.waitForTimeout(400); await expect(…toc-link.active).toHaveCount(1);`
- **Mechanism:** sleep-as-synchronisation. The scroll-spy (IntersectionObserver/scroll handler) updates asynchronously; the test guesses 400 ms is enough instead of waiting for the state.
- **Failure scenario:** on a 4-worker CI run with a big page, the observer callback lands after 400 ms → `.active` count is 0 → test fails; identical code passes when the machine is idle.
- **Fix:** `await expect.poll(() => page.locator('.docs-nav-desktop .toc-link.active').count()).toBe(1)` (the suite already uses `expect.poll` elsewhere, e.g. `:153`) and delete the sleep.

## 9. Bank e2e: fixed ports 18790/18791 and a global `/tmp` hold marker between two sequential CI runs — MEDIUM
- **Where:** `src/osprey/examples/talon/playwright-e2e-tests/web-client.js:3-4` (hard-coded `127.0.0.1:18790` / `:18791`), `playwright.config.js` webServer blocks (`reuseExistingServer: false`, `gracefulShutdown` 5 s), `ledger.js:13` (`HOLD = '/tmp/talon_bank.hold'`), `:27` (refuses to start if anything answers on 18790); both runs execute back-to-back in one job (`.github/workflows/ci.yml` "Bank e2e (Playwright)" then "Bank journey and goldens (Playwright)").
- **Mechanism:** fixed ports + shared global state across processes. If the first run's server is not fully torn down when `ledger.js` probes (SIGTERM grace is 5 s; the marker lives in `/tmp` shared by anything on the machine), the journey either aborts ("A Talon server already answers on 18790") or — if the old server dies between the probe and the new bind — the fresh-seed assertions (`$7,980.25` in `app/self-test.json`) race a server that may answer from a mutated ledger.
- **Failure scenario:** graceful shutdown takes 5.1 s on a contended runner → the journey run refuses to start, or the stale server answers one request with post-run balances before exiting → golden text mismatch. No code change involved.
- **Fix:** let the server pick an ephemeral port (print it, feed both configs via env), and make the hold marker per-run (`mkdtemp`); or have `ledger.js` wait for the port to be *free* (probe + small bounded retry) instead of failing on first sight of an answer.

## 10. Bank journey: full-page Chromium screenshot goldens with 0.5 % pixel budget — MEDIUM
- **Where:** `src/osprey/examples/talon/playwright-e2e-tests/tests/journey.js:53` — `expect(page).toHaveScreenshot(…, { fullPage: true, animations: 'disabled' })`; `journey.config.js` — `expect: { toHaveScreenshot: { maxDiffPixelRatio: 0.005 } }`, goldens under `app/goldens/web/linux`.
- **Mechanism:** screenshot pixel comparison sensitive to font rendering. The goldens are tied to one Playwright/Chromium build (the config comment even documents the docker recipe used to record them); any Chromium/fontconfig/freetype bump in the runner image or `package-lock` re-renders text beyond the 0.5 % budget. No code change is needed to flip this red.
- **Failure scenario:** a Dependabot bump of `@playwright/test` changes text rasterisation → every journey shot at both widths fails with pixel diffs; locally (different platform) the same run never reproduces.
- **Fix:** pin the browser build and fonts in CI exactly as the golden-recording image (or record goldens in CI via `--update-snapshots` on a locked image); better, assert journey state via accessible text (the self-test verbs already do) and keep screenshots as artifacts, not assertions.

## 11. Website Playwright: fixed port 8099 — MEDIUM
- **Where:** `src/toolchain/website/playwright.config.js:7` (`PORT = Number(process.env.PW_PORT) || 8099`), `:30-32` (webServer binds that port; CI sets `reuseExistingServer: false`), `src/toolchain/website/tests/serve.cjs:6` (`PORT = … || 8099`).
- **Mechanism:** fixed port. Two suites on one machine (a developer running website tests while the talon e2e or a second Playwright project runs, or self-hosted runners sharing a host) cannot both bind 8099; in CI Playwright will wait for its URL and time out rather than reuse.
- **Failure scenario:** anything already listening on 8099 → `webServer` fails after its 120 s timeout and the whole `test-website` job fails without a code change.
- **Fix:** default to an ephemeral port (`server.listen(0)`, print it, consume it from the config via a wrapper) or at minimum try 8099 then a small scan before failing.

## 12. Website Playwright: `retries: 1` in CI hides flaky tests — MEDIUM
- **Where:** `src/toolchain/website/playwright.config.js:13` — `retries: process.env.CI ? 1 : 0`.
- **Mechanism:** retries make a flaky test *pass* nondeterministically: any timing-sensitive failure in this suite (several are listed in this report) is silently absorbed on the second attempt, so the suite's verdict depends on machine speed, not just code.
- **Failure scenario:** the scroll-spy or wasm-startup tests fail once and pass on retry intermittently; developers never see the flake locally (`retries: 0` off CI), and CI stays green until the day both attempts lose the race.
- **Fix:** keep retries at 0 and fix the timing-sensitive assertions; if retries are kept for triage, emit a loud warning artifact ("passed on retry") so flakes are tracked, not hidden.

## 13. Website tests: hard 20-30 s deadlines around real compiler invocations — MEDIUM-LOW
- **Where:** `src/toolchain/website/tests/helpers.js:114` (`execFileSync(osprey, …, { timeout: 20_000 })` per doc example), `api-docs.spec.js:40` (30 s per fixture example), `playground.spec.js:24` (20 s).
- **Mechanism:** short deadlines on slow CI. Each documentation example spawns the real compiler; dozens run across 4 parallel workers on the 2-core-budget runners shared with the wasm build. A cold page-cache/GC pause can push a single compile past 20 s.
- **Failure scenario:** `execFileSync` throws `ETIMEDOUT` mid-suite on one loaded example; rerun passes.
- **Fix:** raise the deadline for CI (env-scaled), or batch examples into one compiler invocation per file; at minimum distinguish timeout from real assertion failure in the error message.

## 14. Website tests: `networkidle` navigation while pages pull third-party analytics/CDNs — MEDIUM-LOW
- **Where:** `src/toolchain/website/tests/pages.spec.js:88` and `:136` (`waitUntil: "networkidle"` for every page × viewport), with `helpers.js:90` (`IGNORED_REQUEST_HOSTS = /monaco|cdnjs|jsdelivr|unpkg|googleapis|gstatic|googletagmanager|google-analytics/`) proving the pages really do issue third-party requests.
- **Mechanism:** network/DNS dependence. `networkidle` waits for those requests to settle; a slow or stalled GA/CDN request stretches navigation up to Playwright's 30 s goto timeout. The ignore-list keeps them out of the *failure assertions* but not out of the *waiting*.
- **Failure scenario:** googletagmanager.com has a bad minute during CI → `page.goto` times out on a handful of pages → `test-website` fails with no code change.
- **Fix:** navigate with `domcontentloaded` and await the specific things the test asserts on (the suite already does this elsewhere); block third-party routes (`page.route(…, route.abort())`) as `interactions.spec.js:111` already does for jsdelivr.

## 15. Website tests: video decode-throughput assertions — MEDIUM-LOW
- **Where:** `src/toolchain/website/tests/homepage.spec.js:146-148` — `expect.poll(… currentTime).toBeGreaterThan(2)` then `expect(… totalVideoFrames).toBeGreaterThan(15)`; `videos.spec.js` uses the same default-5 s `expect.poll` loops for every film (e.g. `:95-97, :191-193`).
- **Mechanism:** frame-rate/throughput assertion on software video decode. `totalVideoFrames > 15` within ~2 s of playback requires ≥8 effective decoded fps in headless Chromium on a contended runner; the polls themselves default to a 5 s expect timeout for 1080p media startup.
- **Failure scenario:** on a fully loaded 4-worker run, decode startup exceeds 5 s or delivers <16 frames → poll times out or count assertion fails; passes when idle.
- **Fix:** assert on `.readyState`/`currentTime` progression with an explicitly generous timeout and drop raw frame-count thresholds (or gate them on `video.getVideoPlaybackQuality().droppedVideoFrames` sanity rather than absolute counts).

## 16. `window-picture.ps1`: 20 s sleep then one-shot window liveness check — MEDIUM-LOW
- **Where:** `scripts/mobile/window-picture.ps1:33-40` — `Start-Sleep -Seconds $Seconds` (default 20), then single `HasExited` / `MainWindowHandle` checks; used by the gpui windows job for Flock and Painting (`.github/workflows/gpui.yml` "Flock and Painting in a real window") and by `graphics/pictures.ps1`.
- **Mechanism:** sleep-as-synchronisation with a point-in-time check. Window creation slower than 20 s (cold exe, MSYS2 PATH, first-run DLL resolution on the runner) trips `throw "$Exe opened no window"` even though the app would have opened at 25 s.
- **Fix:** poll for `MainWindowHandle != 0` with a deadline (like `self-test.ps1` does, but *failing loudly*), then wait a short settle before `PrintWindow`.

## 17. Website wasm demos: 20-30 s deadlines for in-browser wasm/SQLite startup — MEDIUM-LOW
- **Where:** `src/toolchain/website/tests/interactions.spec.js:122` (`toContainText("Database ready", { timeout: 20_000 })`), `playground.spec.js:12` and `:63` (`toHaveText('Ready', { timeout: 30000 })`).
- **Mechanism:** short deadlines on slow CI for real wasm compile+instantiate plus sql.js DB seeding (studio) and the playground boot, running beside 4 parallel workers.
- **Failure scenario:** wasm instantiation crosses 20 s on a contended runner → banner assertion fails; retry (finding 12) may mask it, making the flake intermittent.
- **Fix:** scale these specific timeouts for CI (60 s) — they are startup waits, not product SLAs — or run the wasm specs in a dedicated worker with `workers: 1`.

## 18. VS Code tests: wall-clock upper-bound assertion (must answer in < 2000 ms) — LOW-MEDIUM
- **Where:** `src/toolchain/vscode-extension/test/suite/package-workspace-wasi-guests.test.ts:80-82` — `const started = Date.now(); await assert.rejects(runtime.event(touch), …); assert.ok(Date.now() - started < 2000, "a call after the worker died is refused at once…")`.
- **Mechanism:** timing assertion on a negative path. The test asserts the rejection takes under 2 s; a GC pause or scheduler stall on a shared macOS runner can exceed that even though the code correctly refuses immediately.
- **Failure scenario:** event-loop blockage >2 s between the two `Date.now()` reads → assertion fails; rerun passes.
- **Fix:** assert the *mechanism* (rejection reason mentions the dead worker, no call-timeout error) and either drop the duration bound or make it generous (10 s) — it guards against regression to the full call timeout, which is itself tens of seconds.

## 19. Storage drive self-test: `sleep 60` / `sleep 45` synchronise a real filesystem walk — LOW-MEDIUM
- **Where:** `src/osprey/examples/storage/self-test-drive.json:2-8` (`{"sleep": "60"}`, `{"sleep": "45"}`), run by `.github/workflows/winui.yml` "Osprey Storage measures the C drive in a real window" with `-Seconds 480`; the walk itself "may stop at its two-minute deadline" (workflow comment).
- **Mechanism:** sleeps as progress bars. The steps assume the walk of `C:\` reaches `Program Files` within 60 s (plus the following `wait`'s 90 s patience) and `Users\Public` within 45 s more — on a runner whose disk image grows (SDKs, caches), or under Defender scanning, the marker rows appear late or the partially-measured listing differs.
- **Failure scenario:** the `wait: "Public"` step exhausts its 90 s patience on a slow scan → self-test fails with "nothing shows Public"; rerun on a quieter runner passes. (The shots are artifacts only, so pixel risk is limited to the run failing.)
- **Fix:** replace sleeps with the app exposing a "walk finished / walked N folders" accessible state the script can `wait` on (the app already caps at two minutes), keeping the script's total under the app's own deadline by construction.

## 20. WinUI capability tests: `Task.Delay(300)` to get a request in flight before cancelling — LOW
- **Where:** `src/toolchain/hosts/winui/OspreyHost.Tests/CapabilitiesTests.cs:258-266` — start the slow request, `await Task.Delay(300, …)`, then send `cancel:2` and assert the outcome is exactly `"cancelled"`.
- **Mechanism:** sleep-before-act race. If dispatch of the HTTP request takes >300 ms on a loaded runner, the cancel can be processed before/while the request starts; depending on the host's ordering the error may not be the asserted literal.
- **Failure scenario:** CI hiccup → `done.Error` isn't `"cancelled"` (or the 5 s-delaying server answers first) → assertion fails; passes on rerun.
- **Fix:** synchronise on an observable "request started" signal (the `Loopback` server records `Heard`; wait until `server.Heard` contains the request) before cancelling, instead of a fixed 300 ms.

## 21. Agent self-test: fixed shared temp workspace wiped at start — LOW
- **Where:** `src/osprey/examples/agent/self-test.ps1:20` — `$work = Join-Path ([IO.Path]::GetTempPath()) 'osprey-agent-work'`, `:50` — `Remove-Item $work, $records -Recurse -Force`.
- **Mechanism:** shared global temp dir between parallel invocations. Two runs of `app.ps1 -App agent -Test` (or a run beside a developer's manual run) on one machine delete each other's stores mid-journey.
- **Failure scenario:** concurrent runs interleave `Remove-Item` with the other run's `--store` reads → the agent's UI shows missing-store errors → waits fail; each run alone passes.
- **Fix:** use a per-run folder (`[IO.Path]::GetRandomFileName()` under temp) since nothing depends on the fixed name except golden cosmetics ("its name shows in the pictures" — in which case keep the name but take an exclusive lock or refuse if it already exists).

## 22. Website Playwright: site build inside the webServer's 120 s budget — LOW
- **Where:** `src/toolchain/website/playwright.config.js:29-33` — `command: 'npm run build && node tests/serve.cjs ${PORT}'`, `timeout: 120_000`.
- **Mechanism:** the whole eleventy+wasm site build shares one 120 s deadline with server bind; the build regenerates reference docs through the freshly downloaded compiler (`WEBSITE_PREBUILT_COMPILER`), so a slow artifact/compiler step eats the budget before the first test runs.
- **Failure scenario:** cold-cache build takes 121 s → `webServer` times out, every test in `test-website` fails at setup; a rerun with warm caches passes.
- **Fix:** build in a separate CI step (cached) and let the webServer only `node tests/serve.cjs`; or split the timeout so build and serve are budgeted independently.

## 23. VS Code DAP harness: fixed 30 s polling deadlines around debug sessions — LOW
- **Where:** `src/toolchain/vscode-extension/test/suite/dap-harness.ts:90-99` (`waitForDebugSessionStart`, 30 s), `:107-119` (`waitForDebugSessionEnd`), `:332-361` (`waitForStop`, 30 s + `SOURCE_RESOLVE_GRACE_MS`).
- **Mechanism:** generous but fixed deadlines around process launch + lldb-dap on shared runners; a cold lldb-dap start (first-launch symbol resolution on macOS) can exceed 30 s under load. The polls themselves are correct (100 ms interval, deadline-bounded); only the budget is machine-dependent.
- **Failure scenario:** cold debug-adapter start >30 s → "Debug session did not start within 30000ms" / "Timed out waiting for the debuggee to stop" on one run, green on the next.
- **Fix:** scale `timeoutMs` from env for CI (e.g. 120 s) — the values guard hangs, and hangs are equally caught by a larger budget — and keep the default tight only for local runs.

## 24. Website pages suite: fixed sleeps after resize/navigation before layout assertions — LOW
- **Where:** `src/toolchain/website/tests/pages.spec.js:90` (`waitForTimeout(300)` before asserting zero console/request errors), `:137` (`waitForTimeout(200)` after `setViewportSize` + `networkidle` before measuring `scrollWidth - innerWidth ≤ 2`).
- **Mechanism:** fixed sleeps around async layout/error collection. The overflow measurement after a viewport resize relies on 200 ms being enough for reflow+scrollbar appearance; async console errors arriving after 300 ms are simply missed (a false-negative risk rather than a failure, but the overflow check can also fail if a lazy-loaded font swaps in late and widens the document).
- **Failure scenario:** late font swap on a slow runner widens `scrollWidth` by >2 px after the 200 ms window... or conversely the measurement happens before the swap and passes — the verdict depends on timing, not code.
- **Fix:** wait for `document.fonts.ready` (already used in `homepage.spec.js:45`) before measuring overflow, and collect console errors on `page.close` via a fixture teardown instead of after a sleep.

---

### Not flagged (verified as sound)
- GPUI windowless smoke/self-test driver (`smoke.rs`, `self_test.rs:217-258`): deterministic frame deltas, inline capability answers, frame-count budgets (`FRAMES = 600`) independent of wall time.
- WinUI steady clock (`Session.cs:275-291`): granted frames are exactly 1/60 s and frozen between steps, making the flock/painting frame-count goldens machine-independent.
- `smoke.jsonl` expectations (`primitives: 21`, ordered `requests`): pure functions of the replayed event stream.
- Website docs fixture (`docs-fixture.cjs:36-39`) uses `mkdtempSync` under `_site`, and `builtPages()` (`helpers.js:16-24`) excludes that prefix, so parallel workers cannot observe each other's fixtures.
- Workflow concurrency groups (`gpui.yml:26-28`, `winui.yml:26-28`, `ci.yml:53-55`) all use `cancel-in-progress: false`, so no in-flight-cancellation flakiness between jobs.
