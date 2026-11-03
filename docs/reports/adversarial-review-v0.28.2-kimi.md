# Adversarial review: v0.28.2 to main (78be07ec)

Reviewer: Kimi, read-only; no tests were executed.

**Verdict: not safe to release.** The new media server permits arbitrary file disclosure on Windows and has a stop-time memory-safety race. Additional release blockers include arbitrary process/file destruction, a deterministic-scheduler deadlock, and an explicit `abort()` contrary to the repository’s core failure contract.

## Findings

### [CRITICAL] Windows folder requests can traverse outside the served directory

**Location:** `compiler/runtime/file_server_runtime.c:210-225`

**Status:** Confirmed by reading

**Evidence:**
```c
const char *name = path + 1;
if (path[0] != '/' || name[0] == '\0' || name[0] == '.' || strchr(name, '/')) {
  return false;
}
snprintf(served->path, sizeof served->path, "%s/%s", server->path, name);
...
for (int waited = 0; stat(served->path, &info) != 0; waited++) {
```

The guard rejects `/` but not `\`, which is a path separator on Windows. The resulting path is subsequently opened with `fopen` in `send_body`.

**Failure scenario:** Serve `C:\Users\me\Music\render`, then make a raw request:
```http
GET /\..\..\Desktop\secret.txt HTTP/1.1
```
The name begins with a backslash and contains no `/`, so it passes. Windows resolves `render/\..\..\Desktop\secret.txt` outside the served folder and returns the secret over the network. This directly contradicts the claim at `file_server_runtime.c:15-16` that nothing outside the folder is reachable.

**Fix:** Reject both path separators and all parent/drive forms, then enforce containment using canonicalized paths or handle-based traversal. Also reject symlink/reparse-point escapes. Add a Windows loopback regression using raw backslashes.

### [HIGH] Stopping a file server frees state still used by detached connection threads

**Location:** `compiler/runtime/file_server_runtime.c:210-225,255-279,318-322,383-400`

**Status:** Confirmed by reading

**Evidence:**
```c
pthread_detach(thread);
```
```c
static void release_server(FileServer *server) {
  free(server->path);
  free(server->content_type);
  memset(server, 0, sizeof *server);
  server->listen_fd = -1;
}
```
```c
atomic_store(&server->stopping, true);
wake_listener(server);
pthread_join(server->thread, NULL);
close(server->listen_fd);
release_server(server);
```

Only the accept thread is joined. Each detached `Connection` retains the same `FileServer *`, and accepted sockets have no read deadline.

**Failure scenario:** Start a server, open a connection without sending a request, and call `osprey_file_server_stop`. The connection remains blocked in `read_request` while stop frees and clears its server state. Sending `GET /media HTTP/1.1\r\n\r\n` afterward makes `resolve` access the cleared/freed fields. Similarly, a folder request sleeping in `resolve` can observe `stopping` reset to false by `memset` and continue waiting rather than terminating. Depending on timing and libc behavior, this produces use-after-free, null-pointer formatting, stale-slot reuse, or a 120-second hang.

**Fix:** Track active connections and their sockets. Stop accepting, signal waiters, close or otherwise wake every connection, and join/reap all of them before freeing or clearing the slot. Do not reuse a slot until its connection count reaches zero.

### [HIGH] `osprey_media_unrender` accepts arbitrary directories and PIDs

**Location:** `compiler/runtime/media_render_runtime.c:193-229`; pass-through caller at `packages/chromecast/src/wire.py:124-125`

**Status:** Confirmed by reading

**Evidence:**
```c
if (pid > 0) {
  kill((pid_t)pid, SIGTERM);
  waitpid((pid_t)pid, NULL, 0);
}
```
```c
int64_t osprey_media_unrender(char *folder) {
  if (!folder || !strstr(folder, "/osprey-cast.")) {
    return -1;
  }
  stop_renderer(folder);
  DIR *dir = opendir(folder);
  ...
  if (entry->d_name[0] != '.') {
    snprintf(path, sizeof path, "%s/%s", folder, entry->d_name);
    unlink(path);
  }
  ...
  return rmdir(folder) == 0 ? 0 : -1;
}
```

A substring is the entire ownership check, despite the comment promising `-1` for a folder no render made. The PID is read from an untrusted `pid` file, and every non-hidden directory entry is unlinked.

**Failure scenario:** Create `/tmp/osprey-cast.fake`, write the PID of another same-user process to its `pid` file, and place documents beside it. Calling `osprey_media_unrender("/tmp/osprey-cast.fake")` sends SIGTERM to that process and deletes the documents. A stale genuine render folder can also kill an unrelated process after PID reuse.

**Fix:** Return opaque render handles from a runtime-owned registry rather than accepting arbitrary pathnames. Track child PIDs directly, preferably with pidfds where available, and delete only the exact registered directory after validating its identity. Do not trust a writable PID file as proof of process ownership.

### [HIGH] A blocked deterministic `select` deadlocks while `await` holds `runtime_mutex`

**Location:** `compiler/runtime/fiber_runtime.c:270-277,615-635`

**Status:** Confirmed by reading

**Evidence:**
```c
if (is_deterministic) {
  pthread_mutex_lock(&runtime_mutex);
  for (int64_t i = 0; i < queue_size; i++) {
    ...
    execute_fiber_directly(current_fiber);
```
```c
static bool select_run_queued(void) {
  pthread_mutex_lock(&runtime_mutex);
  ...
}
```
```c
while (chosen == count && timeout_ms != 0 && select_run_queued()) {
  chosen = select_scan(count, kinds, ids, words, received);
}
```

`fiber_await` executes queued thunks while holding a non-recursive mutex. A thunk that reaches a not-ready `select` immediately tries to acquire that same mutex.

**Failure scenario:**
```osp
extern fn fiber_set_deterministic_mode(enabled: bool) -> int
let _ = fiber_set_deterministic_mode(enabled: true)
let c = Channel(1)
let reader = spawn(select { x = recv(c) => x })
let writer = spawn(send(c, 1))
print(await(reader))
```
`await(reader)` runs the reader first because it was spawned first. Its channel is empty, so `select_run_queued` deadlocks before the queued writer can send. The new select tests do not put a blocked select inside deterministic fiber execution.

**Fix:** Do not hold `runtime_mutex` while executing user thunks. Extract scheduling state under the lock, release it, and execute outside it. Track the currently executing fiber so the scheduler cannot select itself recursively. Add this reader-before-writer regression in both flavors.

### [HIGH] `Channel` construction deliberately aborts the process

**Location:** `compiler/runtime/fiber_runtime.c:449-458`; emitted by `crates/osprey-codegen/src/fiber.rs:367-375`; test expectation at `compiler/runtime/fiber_runtime_tests.c:340-350`

**Status:** Confirmed by reading

**Evidence:**
```c
int64_t channel_create_checked(int64_t capacity) {
  int64_t id = channel_create(capacity);
  if (id >= 1) {
    return id;
  }
  fprintf(stderr, ...);
  abort();
}
```
The test suite explicitly requires this:
```c
assert(signalled == SIGABRT);
```

**Failure scenario:** An ordinary program evaluating `Channel(0)`, `Channel(-1)`, or a valid channel after handle exhaustion terminates with SIGABRT instead of producing a typed failure. This violates `AGENTS.md:14`: “Osprey never panics, aborts or exits” and every failure must be a `Result` or handled effect.

**Fix:** Change the language-level constructor to return `Result<Channel<T>, Error>` or route failure through a defined effect, and lower that contract explicitly. Replace the death-test expectations with typed-error assertions; do not quarantine this as an intentional C abort.

### [MEDIUM] Native store confinement is bypassed through intermediate symlinks

**Location:** `hosts/swift/Sources/OspreyNative/StoreAdapter.swift:36-39,60-64` and `hosts/winui/OspreyHost.Core/Store.cs:50-59,86-94`

**Status:** Confirmed by reading

**Evidence:**
```swift
let parts = name.split(separator: "/").map(String.init)
guard !name.isEmpty, !name.hasPrefix("/"), !parts.contains(".."), !parts.contains("") else { ... }
return parts.reduce(root) { $0.appendingPathComponent($1) }
...
try Data(...).write(to: url, options: [.atomic, ...])
```
```csharp
var safe = !Path.IsPathRooted(name) && !name.Contains(':') &&
           parts.All(part => part is not ("" or ".."));
return safe ? Path.Combine([root, .. parts]) : null;
...
File.WriteAllText(temporary, text, Strict);
File.Move(temporary, path, overwrite: true);
```

Both implementations validate path text but never reject a symlink in an intermediate component. Swift’s no-follow read protection applies only to the final opened path and does not protect writes.

**Failure scenario:** With `store/link` symlinked to an external directory, a `store-write` for `link/secret` writes `secret` through that link on both hosts. Reads and deletes can similarly operate outside the claimed store wherever OS permissions allow.

**Fix:** Traverse from a trusted directory handle, rejecting symlinks/reparse points at every component, and perform the final operation relative to that verified handle. Canonical-path checks alone need a race-safe open/rename strategy. Add intermediate-link tests for read, write, list, and delete on both hosts.

### [MEDIUM] WinUI’s process output limit is enforced only after unbounded buffering

**Location:** `hosts/winui/OspreyHost.Core/Processes.cs:38-65`

**Status:** Confirmed by reading

**Evidence:**
```csharp
var stdout = child.StandardOutput.ReadToEndAsync(deadline.Token);
var stderr = child.StandardError.ReadToEndAsync(deadline.Token);
...
static Completion Finished(..., string stdout, string stderr) =>
    stdout.Length > Envelopes.Number(request, "limit")
        ? Completion.Failed(ticket, "The output passed its limit")
        : Completion.Done(...);
```

`ReadToEndAsync` accumulates the complete streams before `Finished` checks stdout’s length. Stderr is never checked against the limit at all.

**Failure scenario:** A child launched with a small output limit continuously writes stdout or stderr until the deadline. The host continues allocating for the entire run and can exhaust memory before returning the promised limit error. This differs from the Swift adapter’s capped streaming drain.

**Fix:** Drain both streams incrementally, retain at most the configured limit plus one byte, discard further bytes while draining, and terminate the child as soon as either stream exceeds its bound. Test a child that writes continuously past the deadline.

### [MEDIUM] The HTTP provider converts body and cleanup failures into success

**Location:** `packages/pydantic_ai/src/providers_http.py:21-38`

**Status:** Confirmed by reading

**Evidence:**
```python
closed = unwrap_or(httpCloseClient(client), -1)
return result if closed == 0 else result
```
```python
text = unwrap_or(httpResponseBody(handle), "")
freed = unwrap_or(httpResponseFree(handle), 0)
return Success(value=P.HttpReply(status=status, body=text)) if freed == 0 else Success(value=P.HttpReply(status=status, body=text))
```

Both conditional expressions return the same value regardless of the failure they inspect. Body failure additionally fabricates an empty response.

**Failure scenario:** If `httpResponseBody` fails because of allocation failure or an invalid response slot, `post_json` returns `Success` with an empty body. If response freeing or client closing fails, it still returns the same success and hides the resource failure. The sole real transport test covers only connection refusal, so it cannot catch these branches.

**Fix:** Propagate body-read errors and return a typed error when free/close fails, while preserving the original request error if one already exists. Replace the empty-string fallback and identical conditional branches; add tests using failing response and cleanup handlers.

### [MEDIUM] Escaped quotes corrupt interpolation scope and break later lambdas

**Location:** `crates/osprey-syntax/src/strings.rs:205-231,255-264`

**Status:** Confirmed by reading

**Evidence:**
```rust
(Some(Frame::Code { .. }), '"') => self.open.push(Frame::Text),
(Some(Frame::Text), '"') => {
    let _ = self.open.pop();
}
```
```rust
let lambda =
    scope.opens_word() && opens_lambda(text.get(next..).unwrap_or_default());
...
scope.observe(escaped);
```

An escaped quote is passed to the syntax-scope observer as though it were a real delimiter.

**Failure scenario:** In the valid interpolated source:
```osp
"${f("\"", \n => n)}"
```
the `\"` inside the nested string incorrectly pops `Frame::Text`; the nested string’s actual closing quote then pushes a fresh `Text` frame. Consequently `\n`, which should remain the parameter spelling of a lambda, is decoded to a physical newline. The resulting fragment is misparsed or rejected rather than producing the lambda.

**Fix:** Track lexical escapes separately: an escaped character must not open or close a string/interpolation frame. Add regressions with escaped quotes before `\n => n` and other escape-shaped lambda parameters in both flavors.

## Areas not reviewed

- No dynamic execution, sanitizer run, network reproduction, or platform-specific build was available; all findings above are source-traced.
- I did not deeply review the HTTP server, WebSocket, GPU, profiler, JSON, collection, or ARC/GC implementations beyond their interactions with the changed select/fiber code.
- Most effect inference, generalization, record specialization, and mobile ABI changes were not exhaustively validated.
- GPUI rendering, app behavior and goldens, the agent UI, most Chromecast protocol logic, website changes, and deployment scripts received only limited or no review.
- `crates/osprey-lsp/src/features/tests.rs` was excluded as instructed.

## Verification notes (added by Claude)

The findings above are Kimi's text, unchanged. These notes record what I checked against `target/release/osprey` and the source at `78be07ec`.

| Finding | Result |
|---------|--------|
| Windows folder traversal | The guard matches the code quoted and doesn't reject `\`. Not run, because I have no Windows machine here. |
| File server stop frees state in use | The stop code is as quoted. Not run. |
| `osprey_media_unrender` | The code is as quoted, including the substring check and the trusted `pid` file. Not run. |
| Deterministic deadlock | **Reproduced, and wider than reported.** See below. |
| `Channel` aborts | **Reproduced.** A handle-exhaustion loop exits 134, and the `AGENTS.md:14` rule is quoted correctly. |
| Store symlinks | Not checked. |
| WinUI output limit | The code is as quoted. Not run. |
| HTTP provider hides failures | The code is as quoted: both branches of each conditional are identical. Not run. |
| Escaped quotes in interpolation | **Reproduced in ML flavor, but Kimi's example is wrong.** See below. |

**Deterministic deadlock:** `select` isn't needed. With deterministic mode on, a fiber that only calls `send(c, 1)`, then `await`, hangs until the timeout, while a fiber with no channel use completes. Code reading points to `fiber_await` holding `runtime_mutex` while the thunk runs and `channel_send` taking the same mutex, so the fix has to cover every channel operation. It only affects programs that turn on deterministic mode through `fiber_set_deterministic_mode`.

**Escaped quotes:** Kimi's snippet uses `\n => n` in a Default-flavor file, which fails with a syntax error even without the escaped quote, so it proves nothing there. In ML flavor, `print "${pick "q" (\n => n)}"` runs and prints `q`, while `print "${pick "\"" (\n => n)}"` fails with a type mismatch. The same happens with `\row => row`. The `Scope` code at fault came in with the #323 fix on `codex/finish-plans`.
