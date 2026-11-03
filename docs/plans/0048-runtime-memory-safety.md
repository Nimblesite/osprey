# Plan 0048 — Runtime memory safety and failure handling

Evidence: [worst three bugs](../reports/worst-three-bugs-2026-11-03.md), [Kimi review](../reports/adversarial-review-v0.28.2-kimi.md), [GLM review](../reports/adversarial-review-v0.28.2-glm.md).
Specs: [0014 HTTP](../specs/runtime/0014-HTTP.md), [0015 WebSockets](../specs/runtime/0015-WebSockets.md), [0011 Fibers](../specs/language/0011-LightweightFibersAndConcurrency.md), [0019 FFI](../specs/compiler/0019-ForeignFunctionInterface.md).

## Goal

Fix the five worst defects found by the reviews of `v0.28.2` to `78be07ec`. Each fix ships with a test that fails on `78be07ec`, a spec ID, and the reproduction in its report turned into a permanent test.

| Order | Defect | Why it ranks here | Evidence |
|-------|--------|-------------------|----------|
| 1 | Chunked decoder heap overflow | One response from any server corrupts or crashes the client | Reproduced |
| 2 | File server use-after-free and Windows traversal | LAN peers can reach files outside the folder on Windows, and stop frees memory still in use | By reading; both reviewers |
| 3 | WebSocket server keeps dead connections | Frames go to the wrong socket, and clients are locked out after 1000 connections | Reproduced; stop use-after-free by reading |
| 4 | Deterministic mode deadlocks on channels | Any channel use inside a deterministic fiber hangs | Reproduced |
| 5 | Fiber and channel handle exhaustion | The 999th handle gives a silent `-1`, a segfault or an abort | Reproduced |

The order puts small, isolated fixes first. 4 comes before 5 because both change locking and tables in `fiber_runtime.c`, and 5 is the only one that changes the language.

## Checklist

### 1. Chunked decoder [HTTP-CHUNKED-DECODE]

- [x] Spec: a chunk size that is malformed, overflows, or exceeds the remaining input makes the response an `Error`; no partial body is returned
- [x] `http_decode_chunked` takes the body's length, parses sizes as unsigned with an overflow check, bounds every copy, and requires the CRLF after each chunk
- [x] `http_request_capture` returns a new negative code on a malformed body instead of falling back to the raw framing at `strdup(body_start)`
- [x] Test: a real program calling `httpGetResponse` against a local hostile server, with each of an oversized size, `7FFFFFFF`, a negative size, a missing CRLF, a truncated body and a valid multi-chunk body
- [x] Review: `204` and `304` with chunked framing are empty, not an error; a NUL byte anywhere in a response is an `Error`, not a body cut short at it [HTTP-RESPONSE-FRAMING]
- [x] Review: repeated `Transfer-Encoding` lines are one list; a control byte in a chunk extension is refused; running out of memory copying headers is `-9`, not raw framing returned as the body
- [x] Adversarial review: a status is exactly three digits, so a hostile one can't overflow or pose as this API's negative codes; interim `1xx` responses are skipped; a head that is cut short, or that hides framing, is refused rather than returned as a body; a body short of its `Content-Length` is refused; control bytes in trailers are refused [HTTP-RESPONSE-FRAMING]
- [x] Test: all of the above against the hostile Osprey server, except what its server can't send (NUL bytes, interim responses, heads cut short) and allocation failure, which the scripted C origin covers, with bodies past the transport's buffer in both, and every byte mutation of whole responses at the C level

### 2. File server [FFI-FILE-SERVER-CONTAINMENT], [FFI-FILE-SERVER-STOP]

- [ ] Spec: a name containing `/`, `\`, `:` or a `.`-leading component is refused, on every platform
- [ ] `resolve` applies that rule before building the path, and stops following symlinks out of the folder by comparing canonical paths
- [ ] Spec: stop returns only after every connection has ended; the current "connections in flight finish on their own" text goes
- [ ] The server tracks live connections and their sockets; stop shuts those sockets down, wakes waiters, and waits for the count to reach zero before `release_server`
- [ ] `read_request` gets a read deadline, so a silent client can't hold a thread forever
- [ ] Tests in `net_runtime_tests.c`: traversal attempts on every platform, including the `ci-windows.yml` run; stop while one client is silent, one is mid-download and one is waiting on a missing folder file; start a new server in the same slot straight after

### 3. WebSocket server [BUILTIN-WEBSOCKET-CONNECTION-END]

- [ ] Spec: a connection leaves the server's table when it ends, and a broadcast counts only live connections
- [ ] The connection thread removes its entry under `server->mutex` before closing its socket
- [ ] `websocket_stop_server` shuts down each connection, waits for its thread, and alone frees shared state; no descriptor is closed twice
- [ ] Stop no longer holds `runtime_mutex` across `pthread_join`
- [ ] Zero-length frames no longer leak the payload; the table-full path closes and frees everything it allocated
- [ ] Test: a real program and a raw client: A connects and leaves, B connects, a broadcast reports 1 recipient and B gets 1 frame; 1100 sequential connections all get served; stop with live clients

### 4. Deterministic mode [CONCURRENCY-DETERMINISTIC]

- [ ] Thunks run with `runtime_mutex` released, in `fiber_await` and `select_run_queued`; a fiber being executed is marked so it can't be picked again
- [ ] A blocking `recv` on an empty channel and a `send` on a full one run queued fibers first, as `select_deterministic` already does
- [ ] Spec: decide what happens when nothing is left to run and the operation still can't proceed. Follow item 5's decision, so it surfaces as a failure and not a hang
- [ ] Tests in both flavors: the four programs from the Kimi review notes (`send` then `await`, `select` on a ready channel, writer before reader, reader before writer), plus the true-deadlock case

### 5. Handle exhaustion [CONCURRENCY-HANDLE-LIMITS]

- [ ] Replace `fibers[1000]`, `channels[1000]` and `execution_queue[1000]` with growable storage, separate counters for fibers and channels, and one bounds check in place of the scattered `1000` tests
- [ ] Spec and language: `Channel(capacity)` returns `Result<Channel<T>, Error>` and `spawn` returns `Result<Fiber<T>, Error>`, covering a bad capacity, an allocation failure and `pthread_create` failing
- [ ] Remove `channel_create_checked`'s `abort()`, and replace the `SIGABRT` death test in `fiber_runtime_tests.c` with typed-error assertions
- [ ] Migrate all callers across `tests/`, `examples/`, `packages/`, the Book and the website, as #213 did; about 31 Osprey files construct channels today
- [ ] Test, long-form and in both flavors: 5000 sequential fibers returning `int`, then `string`, then a record; 5000 channels; a bad capacity through a variable; all under GC and ARC

### Cross-cutting

- [ ] Add an AddressSanitizer build of `net_runtime_tests.c`, the HTTP tests and `fiber_runtime_tests.c` to an existing CI job, not a new one. The repo has no ASan build of these tests today, which is how item 1 got through
- [ ] Fix the stale claim in the `file_server_runtime.c` header comment and the `FFI-FILE-SERVER` spec text
- [ ] Retire this plan and move the contracts into the specs when done

## Decisions

**`spawn` and `Channel` return `Result`.** `AGENTS.md` bans aborts and requires every failure to be a `Result` or a handled effect. An effect would force every program using fibers to install a handler, as `Arith` does, for a failure most never see. `Result` matches what #213 did for the HTTP builtins. This is a breaking change to the language, and the migration lands in the same commit.

**Growable tables, not recycled slots.** A `Fiber` handle can be awaited any number of times and its cached result stays valid, so a slot can't be freed at the first `await`. There is also no language-level point at which a channel is finished with.

**Limit of this plan:** after item 5, a long-running server no longer fails at 999 handles, but it uses a little more memory for every fiber and channel it creates until the process exits. Reclaiming them needs handle ownership or scopes, which belong to [plan 0026](0026-structured-concurrency.md). Add that as a 0026 requirement when item 5 lands, and measure the per-handle cost in the test so the growth is known.

## Out of scope

These stay open as findings in the reports:

- `osprey_media_unrender` accepting arbitrary `/osprey-cast.` paths and trusting the `pid` file
- The escaped-quote interpolation bug in `crates/osprey-syntax/src/strings.rs`, which came in with the #323 fix and is small enough to fix on its own
- `providers_http.py` hiding body and cleanup failures
- WinUI output buffering and store symlinks
- Compiler stack overflow on extreme nesting, and segfaults on deep non-tail recursion
- The Chromecast recursion finding, which is unproven
- WebSocket close, ping and fragmented frames, which [BUILTIN-WEBSOCKET-FRAMING] documents as unimplemented
- Found while fixing item 1, and left for their own plans: `http_perform` reads a response into memory until the peer closes, with no size limit, so a hostile server can exhaust the client's memory; and response bodies travel as NUL-terminated strings, so a body holding a NUL byte can only be refused [HTTP-RESPONSE-FRAMING], which rules out binary downloads through this API; and HTTP client and server handles share one counter that is never reused (`get_next_id`), so after 100 clients a process can't create a server and after 1,000 it can't create a client, however many were closed

## Risks

- **Item 5's migration is large,** and it touches goldens and the docs site. Do it last, in one commit, and run the full corpus under GC and ARC before pushing.
- **Stop now waits for connections,** so a host could block if a socket isn't woken. Every wait gets a test with a client that never reads or sends.
- **Items 4 and 5 share locks.** Land 4 first and keep them in separate commits, so a regression can be traced to one of them.
- **Windows behaviour can only be tested in CI** from this machine, so item 2's traversal test costs a `ci-windows.yml` run per attempt. Get the logic right on POSIX first by testing the name rule on all platforms.
