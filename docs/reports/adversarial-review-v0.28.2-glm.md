# Adversarial review: v0.28.2 to main (78be07ec)

Reviewer: pi agent (`zai/glm-5.3`), read-only adversarial pass over the 3-commit range (33.1k insertions / 745 files), deepest on the new C runtime units, the select feature end-to-end, and the new network-facing packages.

**Verdict: not safe to release as-is.** The new `file_server_runtime.c` carries a use-after-free reachable from the shipped casting flow and a Windows-only path traversal that its own comment claims is impossible; the compiler work (select, exit status, strict warnings, UTF-8 `take`/`drop`, HTTP `Result` externs) survived adversarial reading, and I found no manufactured-green tests — the corpus floors were actually hardened (env overrides removed).

---

## Findings

### [HIGH] File server: `osprey_file_server_stop` frees the `FileServer` while detached connection threads still use it (UAF)

**Location:** `compiler/runtime/file_server_runtime.c:316-321` (`release_server`), `:388-400` (`osprey_file_server_stop`), `:210-231` (`resolve`), `:279` (`pthread_detach`).

**Status:** Confirmed by reading.

**Evidence:**
```c
static void release_server(FileServer *server) {
  free(server->path);
  free(server->content_type);
  memset(server, 0, sizeof *server);   // stopping reverts to false, path becomes NULL
  server->listen_fd = -1;
}
```
`osprey_file_server_stop` sets `stopping`, wakes and joins only the **accept** thread, then calls `release_server` (lines 394-399). Connection threads are detached (`pthread_detach(thread);`, line 279) and keep running: the header comment says "Connections in flight finish on their own" (line 20), but they finish by dereferencing the freed server. In single-file mode `resolve` aliases the heap pointer (`served->content_type = server->content_type;`, line 213) which `serve_media` later reads into its response headers (line 188); in folder mode the wait loop re-reads `server->path` every 100 ms for up to `FS_WAIT`=120 s (`snprintf(served->path, ..., "%s/%s", server->path, name)`, line 220) and its exit condition `atomic_load(&server->stopping)` (line 223) is false again after the `memset`. Worse, `free_server()` (line 313) immediately re-offers the zeroed slot, so a new `osprey_file_server_start` installs a new `path`/`content_type` that the stale thread reads — serving one session's file under another session's server.

**Failure scenario:** The shipped casting flow (`packages/chromecast/src/player.py:242-246` `end()` → `Media.unserve` → `osprey_file_server_stop`) runs while the Chromecast still has open connections fetching HLS segments (its documented behavior — "a player that seeks opens several connections"). Stop frees `server->path`/`content_type` under the connection thread → UAF read; if the slot is reused by the next cast, the stale thread serves the *new* media file to the old device.

**Fix:** Reference-count the `FileServer` (or keep per-connection copies of `path`/`content_type` taken at accept time) and only `release_server` when the last connection thread exits; keep `stopping` set after release.

---

### [HIGH] File server folder mode: Windows backslash path traversal — the in-code safety claim is false on Windows

**Location:** `compiler/runtime/file_server_runtime.c:216-221` (`resolve`); built into the Windows runtime via `scripts/make/config.mk:148` (`HTTP_OBJ … bin/file_server_runtime.o`) and `crates/osprey-cli/src/desktop.rs:200-208` (`DLL_OPENSSL`, `dll_args(…, http)`, "[FFI-TLS-CLIENT]").

**Status:** Confirmed by reading (guard omits `\`; Windows path normalization resolves `..` across both separators).

**Evidence:**
```c
  const char *name = path + 1;
  if (path[0] != '/' || name[0] == '\0' || name[0] == '.' || strchr(name, '/')) {
    return false;
  }
  snprintf(served->path, sizeof served->path, "%s/%s", server->path, name);
```
The file header claims "Names with a slash or a leading dot are refused, so nothing outside the folder is reachable" (lines 17-18). Only `/` and a *leading* `.` are refused. On Windows, where this unit is compiled into `libhttp_runtime*.a` and linked into desktop DLLs, `\` is a path separator and interior `..` components are resolved by the CRT: `folder\a\..\..\x` escapes the folder, and `name[0]` is not `.` so the leading-dot test never fires.

**Failure scenario:** On a Windows host, any Osprey program that starts the folder file server on its LAN interface (the API the chromecast package publishes, `packages/chromecast/src/wire.py:48`, bound to the route address per `media.py:67-69`), a LAN peer sends `GET /\..\..\Users\me\Documents\secret.txt HTTP/1.1` — `name` is `\..\..\Users\me\Documents\secret.txt`, passes the guard, and the server streams a file outside the served folder.

**Fix:** Refuse any name containing `/`, `\`, or a `.`-leading *component* (split on both separators and reject `.`/`..` components); or canonicalize the joined path and verify it is prefixed by the served folder.

---

### [MEDIUM] Chromecast protocol parser: unbounded recursion over device-controlled bytes crashes the process (violates "Osprey never crashes")

**Location:** `packages/chromecast/src/protocol.py:94-101` (`read_varint_from`), `:135-146` (`parse_fields`), `packages/chromecast/src/hex.py:88-96` (`points_from`), `:102-104` (`to_text`).

**Status:** Confirmed by reading (no depth limit; general user recursion gets no tail calls — `musttail` is emitted only for handler dispatch, `crates/osprey-codegen/src/effects/drive.rs:119-124`).

**Evidence:**
```python
def read_varint_from(hex, i, acc, shift) -> Varint:
    b = Hex.byte_at(hex, i)
    ...
    if bitAnd(b, 128) == 0:
        return Varint(value=value, next=i + 1)
    else:
        return read_varint_from(hex, i + 1, value, shift + 7)   # one stack frame per byte
```
`Session.body` (`packages/chromecast/src/session.py`) reads whatever byte count the device's 4-byte header announces (C side caps it at `TLS_MAX_READ` = 16 MB, `tls_client_runtime.c:31`), then `Protocol.parse(rest.hex)` walks it: `parse_fields` recurses per protobuf field and `read_varint_from` per continuation byte; `Hex.to_text` additionally recurses per UTF-8 code point over the whole body. A body that is just 0x80 bytes recurses ~one frame per byte with no bound.

**Failure scenario:** A hostile device on the LAN — or any LAN attacker, since the motivating TLS session is deliberately unverified (`SSL_CTX_set_verify(ctx, SSL_VERIFY_NONE, NULL)`, `tls_client_runtime.c:80`, [FFI-TLS-UNVERIFIED]) and so trivially impersonated — sends a frame header of `00 10 00 00` followed by 1 MB of `0x80` bytes: `read_varint_from` recurses ~1M native frames deep and the casting app dies with SIGSEGV. Per AGENTS.md this is the worst class of failure ("Osprey never panics, aborts or exits").

**Fix:** Rewrite `read_varint_from`, `parse_fields` and `points_from` as loops over an explicit index (or bound depth and fail with `Error`), as the reference `pychromecast` parser does.

---

### [MEDIUM] TLS/net runtime: `gethostbyname` called with no synchronization from concurrent fibers

**Location:** `compiler/runtime/tls_client_runtime.c:56-61` (`tcp_dial`, called from `osprey_tls_connect` *before* taking `tls_mutex`, line 100→129) and `:339-341` (`osprey_net_local_address`, no lock at all).

**Status:** Suspected (unsynchronized calls confirmed by reading; corruption is libc-dependent — glibc `gethostbyname` returns a static `hostent`, not thread-safe).

**Evidence:**
```c
static int tcp_dial(const char *host, int64_t port, int64_t timeout_ms) {
  ...
  struct hostent *server = gethostbyname(host);
  ...
  memcpy(&addr.sin_addr.s_addr, server->h_addr_list[0], (size_t)server->h_length);
```
Fibers are 1:1 pthreads (`fiber_runtime.c:130`), so two fibers calling `Wire.route(host)` / `Wire.dial(host, …)` concurrently both execute `gethostbyname`; `osprey_net_local_address` takes no mutex, and `osprey_tls_connect` only locks *after* dialing. Concurrent resolution can race on the shared `hostent`/`h_errno`, yielding a torn `h_addr_list` (the `memcpy` then reads a stale/garbage pointer) or the wrong destination address.

**Failure scenario:** An Osprey program spawns two fibers that each open a Cast session (`osprey_net_local_address` + `osprey_tls_connect` on glibc Linux): both resolves interleave in `gethostbyname`, one thread's `memcpy` reads the other's half-written address entry → wrong host connected or a crash. (The pre-existing `http_client_runtime.c:188` has the same flaw; this range adds two more unlocked callers.)

**Fix:** Resolve under a dedicated mutex (or switch to thread-safe `getaddrinfo`/`getaddrinfo`-style APIs) in all three units.

---

### [LOW] `media_unrender` kills whatever currently owns a stale pidfile pid

**Location:** `compiler/runtime/media_render_runtime.c:193-207` (`stop_renderer`).

**Status:** Confirmed by reading.

**Evidence:**
```c
  if (fscanf(file, "%ld", &pid) != 1) { pid = 0; }
  ...
  if (pid > 0) {
    kill((pid_t)pid, SIGTERM);
    waitpid((pid_t)pid, NULL, 0);
  }
```
**Failure scenario:** ffmpeg dies mid-render (or finishes a short clip); the OS reuses its pid for an unrelated process; the user later stops the cast and `osprey_media_unrender` SIGTERMs that innocent process (`waitpid` on a non-child silently fails). Same-user impact only, but classic pidfile unsoundness.

**Fix:** Verify the target is still our ffmpeg (e.g. check `/proc/<pid>/cmdline` on Linux, `proc_pidpath` on macOS) before killing, or keep the child handle and `waitpid`-validate.

---

### [LOW] `Dns.discover` interpolates its `seconds` parameter into a `popen` shell command

**Location:** `packages/chromecast/src/dns.py:35-38`.

**Status:** Confirmed by reading (current callers pass constants: `packages/chromecast/live/main.py:108`, `tests/main.py:135`).

**Evidence:**
```python
_browsed = run(f"mkdir -p {SCRATCH} && perl -e 'alarm {seconds}; exec @ARGV' dns-sd -Z _googlecast._tcp local > {out} 2>/dev/null")
```
`run` reaches `spawnProcess`, which is `popen` (`compiler/runtime/system_runtime.c:115-121`) — a shell. Any future caller passing user-influenced text as `seconds` (the README documents the parameter as "seconds") gets arbitrary command execution. An integer argument with a `'` in it would already break the quoting.

**Fix:** Validate `seconds` is a plain non-negative integer before building the command, or pass it as an argv value to a non-shell spawn.

---

### [LOW] WinUI `run` capability checks the output limit only after buffering unbounded output

**Location:** `hosts/winui/OspreyHost.Core/Processes.cs:40-41, 59`.

**Status:** Confirmed by reading.

**Evidence:**
```csharp
var stdout = child.StandardOutput.ReadToEndAsync(deadline.Token);
...
stdout.Length > Envelopes.Number(request, "limit")
```
The limit is applied to the *fully buffered* string; until the deadline fires, a child can stream gigabytes into host memory. The Swift twin enforces the limit during the drain (`hosts/swift/Sources/OspreyNative/ProcessAdapter.swift:74-88`, "keeps at most one byte past the limit; the rest is read and discarded").

**Failure scenario:** A scene's `run` request with `timeout: 60000`, `limit: 1000` against a child that emits output continuously: the WinUI host buffers ~all of it for 60 s before rejecting — memory exhaustion of the app.

**Fix:** Read in chunks and stop (kill) once the limit is passed, as the Swift adapter does.

---

### [LOW] HTTP extern builtins changed `int` → `Result` with in-repo migration but no external migration path

**Location:** `crates/osprey-codegen/src/extern_call.rs:84-91`; migration example `crates/osprey-codegen/src/monofn.rs:381-383`.

**Status:** Confirmed by reading.

**Evidence:**
```rust
"httpCreateServer" => sig("http_create_server", &[I64, Str], Ret::ResultInt),
"httpListen"       => sig("http_listen",       &[I64, Ptr], Ret::ResultInt),
```
(v0.28.2: `Ret::Int`.) Every in-repo caller was rewritten to unwrap (`httpListen(server, handler) ?: -1`), and `docs/specs/runtime/0014-HTTP.md` was updated — but any published package or user program that calls `httpCreateServer`/`httpListen`/`httpGet`… compiled against v0.28.2 now fails to type-check after upgrade, with only the new "undecoded Result" diagnostic to guide them.

**Failure scenario:** `let server = httpCreateServer(3001, "127.0.0.1")` then `httpListen(server, h)` — the handle is now a `Result`, so `httpListen` rejects it; the program that served fine on v0.28.2 no longer compiles, and no release note/migration shim exists in this range.

**Fix:** Ship a migration note (signature table old→new) in the release notes / package-manager changelog, or keep `Int` variants under distinct names for one release.

---

### [LOW] Stale comment claims a test pins a segfault that the test actually asserts is fixed (pre-existing, observed while verifying the diff's adjacent additions)

**Location:** `crates/osprey-cli/tests/cli_e2e.rs:141-158`.

**Status:** Confirmed by reading (not introduced by this range).

**Evidence:** the doc comment says "`join` … **SEGFAULTS (exit 139, no output) on every runtime list** … This pins the crash", while the very next assertions are `assert_eq!(out.code, Some(0))` and `assert_eq!(out.stdout, "a-b c0-c1 x-y\n")`. Per AGENTS.md, stale comments are defects; a reader currently believes `join` still crashes.

**Fix:** Delete or rewrite the comment to describe the regression the test actually pins.

---

## Areas not reviewed

- `hosts/gpui/**` (scene.rs, access.rs, atlas.rs, self_test.rs, ~1.5k new lines) and `hosts/winui` beyond `Processes.cs`/`Sandbox.cs` (Scenery, HostWindow, goldens.ps1) — only skimmed.
- New Osprey packages `signal_flow`, `uitest`, `graphics`, `json`, and the `native` package's new board/canvas/document/machine modules — only spot-checked, not traced.
- `packages/pydantic_ai/src/messages.py` (518 lines), `models_test.py`, `transcript.py`, `settings.py` (API-key plumbing) and the `examples/agent` Python driver — only `engine.py`'s command construction reviewed.
- `crates/osprey-lsp` feature changes beyond `wire.rs` (the prompt excluded `features/tests.rs`; `context.rs`, `engine.rs` only skimmed); `crates/osprey-syntax` python flavor and ML lexer/cst internals beyond select and strings.
- New apps/examples scaffolding (`apps/Makefile`, `apps/windows.json`, casting/synth app sources, `scripts/signal-flow-skins/**`, `scripts/link-skills.mjs`), the website and Book changes, `benchmarks/run.sh`, and the wasm runtime archive contents.
- `memory_gc.c` mark/sweep internals beyond the new hash/bounds changes; `http_origin_tests.c`/`net_runtime_tests.c` test bodies; tree-sitter grammar corpus updates; `docs/plans/0045` ownership plan (no code).

## Verification notes (added by Claude)

Apart from the reviewer line, the text above is GLM's. The model named itself as "Claude Sonnet 4.5"; it ran as `zai/glm-5.3`, so I corrected that line. These notes record what I checked against `target/release/osprey` and the source at `78be07ec`.

| Finding | Result |
|---------|--------|
| File server use-after-free on stop | The stop code matches what Kimi quoted, and both reviewers found this independently. Not run. |
| Windows backslash traversal | The guard doesn't reject `\`, and both reviewers found this independently. Not run, because I have no Windows machine here. |
| Chromecast recursion crash | **Overstated; crash not demonstrated.** See below. |
| `gethostbyname` races | The unlocked calls are in the code as quoted. Not run. The `SSL_VERIFY_NONE` line it cites is at `tls_client_runtime.c:89`, not `:80`. |
| `media_unrender` pidfile | The code is as quoted. Kimi rates the same code HIGH because any folder path containing `/osprey-cast.` is also accepted, which GLM missed. |
| `Dns.discover` shell interpolation | The code is as quoted. Not run. |
| WinUI output limit | The code is as quoted, and Kimi found it too. Not run. |
| HTTP `Result` migration | Not re-checked beyond the signature change, which I made in #337. |
| Stale `join` comment | The comment does say the test pins a crash. |

**Chromecast recursion:** the report says user recursion gets no tail calls and marks this confirmed. A Default-flavor self tail call in the same shape as `read_varint_from` ran to a depth of 20,000,000 without crashing, while non-tail recursion segfaulted at 1,000,000. So `read_varint_from` may not overflow at all. I did not run the Chromecast parser itself or review `parse_fields` and `points_from`, whose calls may not be tail calls, so this finding stays open as unproven.

**Compiler verdict:** the verdict line says the compiler work "survived adversarial reading". That is too generous. The deterministic-mode channel deadlock and the escaped-quote interpolation bug in [the Kimi review](adversarial-review-v0.28.2-kimi.md) both reproduce on this commit.
