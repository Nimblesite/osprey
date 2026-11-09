# Plan 0048 — Runtime memory safety and failure handling

Contract: [0014 HTTP](../specs/runtime/0014-HTTP.md) [0015 WebSockets](../specs/runtime/0015-WebSockets.md) [0011 Fibers](../specs/language/0011-LightweightFibersAndConcurrency.md) [0019 FFI](../specs/compiler/0019-ForeignFunctionInterface.md)

## Prerequisites in place
- Name refusal (`is_plain_name` rejects `/`, `\`, `:`, and a leading dot before `resolve` builds the path) and stop-waits-for-connections already ship in `src/toolchain/compiler/runtime/file_server_runtime.c` (commit `8d9b5c99`).
- Growable fiber/channel tables (single `next_id`, no 999 limit) already ship in `src/toolchain/compiler/runtime/fiber_runtime.c` (commit `6fbf9b05`).

## Cross-cutting
- [ ] Add an AddressSanitizer build of `net_runtime_tests`, `http_runtime_tests` and `fiber_runtime_tests` to the existing C-runtime job (`make _test_c_runtime` in `.github/workflows/ci.yml`), not a new job. Check: the three test binaries build and pass under `-fsanitize=address` in CI. No ASan build of them exists today (only `-fsanitize=undefined` in `scripts/make/config.mk`).

## Runaway memory (top priority)
Evidence: `/Library/Logs/DiagnosticReports/JetsamEvent-2026-11-05-090847.ips`. A compiled `tests-56195-*.out` reached 86.6 GB in about 351 s and `suite-60e67c363b6a174f.out` reached 15.6 GB in about 21 s; the Mac then stalled shutting down and rebooted at 09:12. Which program each was is reported by peers, not established here: `tests-56195` as `safetensors/tests`.
- [ ] Bound the heap in `src/toolchain/compiler/runtime/memory_gc.c` and `memory_arc.c`: neither has a ceiling (the GC budget is `max(min-heap, 2 x live)`), so a runaway program grows until the OS kills unrelated processes. Check: a program allocating without end gets a typed failure with its peak size, in GC and ARC, without touching the rest of the machine.
- [x] [MEM-GC-SAFEPOINT] Keep the GC collecting once a fiber allocates. Under GC, the first allocation by a fiber stopped collection for the rest of the process. Check: `src/toolchain/crates/osprey-cli/tests/fiber_gc_memory.rs`.
- [ ] Find what makes each suite above grow, and fix it. Check: the suite passes under the cap in GC and ARC, with its peak recorded.
- [ ] Reported, not reproduced: killing `osprey --run` leaves its compiled child running. Check: killing the parent ends the child, on every kill route the test runners use.
- [ ] Make runner logs enough to diagnose this: `src/toolchain/crates/osprey-cli/tests/python_packages.rs` buffers child output and records no peak memory, deadline or program name, so the jetsam report was the only record of this crash. Check: a killed or over-cap suite logs its command, memory backend, peak RSS and elapsed time.

## File server [FFI-FILE-SERVER-FOLDER]
- [x] Document lookup-time symlink refusal in [FFI-FILE-SERVER-FOLDER]. `resolve` refuses a symbolic link detected by `lstat`, or a Windows reparse point. `test_file_server_refuses_every_separator` checks that both GET and HEAD return 404 with an empty body for a planted outside link. The Linux `net_runtime_tests` suite passed on checkout `ac5bd75`. [C runtime CI](https://github.com/Nimblesite/osprey/actions/runs/42527979600/job/128877403693)
- [x] #355, the rest (done: both Swift adapters pin read, list and delete through `pinnedDirectory` and `Bounded.read(parent:leaf:)`; the escape test was red first, 4/4 green after; WinUI `Store.cs` still open): 366c025ef and b0673d79e pinned only `store-write` to descriptor-relative directories. `store-read`, `store-list` and `store-delete` in both Swift `StoreAdapter.swift` copies and in WinUI `Store.cs` still resolve the name through `location()`, so a symlinked parent inside the store still reads, lists or deletes outside it. Pin them the same way and add read, list and delete escape cases beside the write one in `StoreAdapterTests.swift` and `StoreTests.cs`.
- [ ] Prevent a concurrent replacement of the checked entry before the server opens it. Open the folder entry without following a link and serve from that same handle. Test that substituting an outside link between lookup and open never returns outside bytes.
- [ ] Run the hostile-name traversal tests on Windows: `.github/workflows/ci-windows.yml` builds the C archives but never runs the C test suites, so `test_file_server_refuses_every_separator` (`a\b`, `\..\..\a`, `C:a`, `a::$DATA`) has never executed on Windows. Build and run `net_runtime_tests` (compiles `file_server_tests.c`) in `ci-windows.yml`. Check: those names all answer 404 on `windows-latest`.

## WebSocket server [BUILTIN-WEBSOCKET-FRAMING]
- [ ] Fix the zero-length frame payload leak: `serve_messages` (`src/toolchain/compiler/runtime/websocket_server_runtime.c`) frees `payload` only when `parse_websocket_frame` returns `> 0`, but `parse_websocket_frame` (`src/toolchain/compiler/runtime/http_shared.c`) `malloc`s 1 byte for an empty frame and returns 0. Free when `payload != NULL`, or do not allocate for a zero-length payload. Check: send `{0x81, 0x00}` frames repeatedly from the raw client in `websocket_lifecycle_tests.c` under the ASan build above; no leak is reported. Done so far: `serve_messages` frees every payload (`websocket_server_runtime.c`) and `websocket_lifecycle_tests.c` sends 64 empty frames and expects the connections released; the ASan report is what remains.

## Handle limits [CONCURRENCY-HANDLES] [CONCURRENCY-CHANNEL]
- [ ] Add the missing item-5 test shapes: a record-returning fiber at scale in `src/osprey/tests/regressions/fiber/many_handles.test.osp` (it scales only `int` and `string`), and a bad-capacity-through-a-runtime-variable death case in `test_unusable_capacities_are_refused_not_returned` (`src/toolchain/compiler/runtime/fiber_runtime_tests.c`), which today calls `channel_create_checked` with literals only. Check: the corpus passes under GC and ARC.
- [ ] Measure the per-handle cost in `src/toolchain/compiler/runtime/fiber_handle_tests.c` (`check_thousands_of_fibers`, `check_thousands_of_channels`), pinning the spec's "few hundred bytes per fiber" to a measured number. Check: the test asserts a concrete per-handle byte figure.
