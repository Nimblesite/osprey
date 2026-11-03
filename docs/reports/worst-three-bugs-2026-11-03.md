# Worst three bugs in Osprey, 2026-11-03

Source read at `main` (`78be07ec`). Each bug below was run against the local `target/release/osprey` on macOS arm64. That binary was built at 01:24 today and was not rebuilt for this scan, but the defective lines quoted are in the source at `78be07ec`.

| # | Bug | Trigger | Result |
|---|-----|---------|--------|
| 1 | Chunked response decoder trusts the size the server sends | One malicious HTTP response | Heap overflow, process dies with SIGBUS |
| 2 | Fiber and channel IDs are never reused, and spawn failure is ignored | The 999th fiber or channel in a process | Silent `-1` result, a segfault, or an abort |
| 3 | WebSocket server keeps dead connections | A client disconnects | Frames sent to the wrong socket, permanent capacity loss, likely use-after-free on stop |

## 1. Remote heap overflow in the HTTP client's chunked decoder

**Where:** [http_client_response.c:32-53](../../compiler/runtime/http_client_response.c#L32-L53)

```c
size_t cap = strlen(body) + 1;
char *out = malloc(cap);
...
long chunk = strtol(p, &end, 16); // chunk size in hex
...
memcpy(out + out_len, data, (size_t)chunk);
```

`chunk` comes straight from the server and is never compared with the bytes left in `body` or the space left in `out`. Any server that an Osprey program calls through `httpGetResponse` or `httpPostResponse` controls the size of this copy.

**Reproduced:**

- A local server answered `httpGetResponse` with `Transfer-Encoding: chunked` and the body `7FFFFFFF\r\nhi\r\n0\r\n\r\n`. The program exited 138 (SIGBUS) before reaching its next line.
- The decoder, copied verbatim into an AddressSanitizer harness and given `10\r\nhi\r\n0\r\n\r\n`, reported `heap-buffer-overflow`, `READ of size 16`, at the `memcpy`.

**Impact:** at minimum, any server can crash the client. A claimed size just past the buffer also writes past `out`, using bytes read from beyond `body`, so heap corruption is possible and neighbouring heap data can end up in the returned body. ASan stopped at the read, so the write was established by reading the code. I did not try to build an exploit.

**Fix:** parse the size as unsigned with an overflow check, and treat the body as malformed unless the size fits in the remaining input. Also require the CRLF after each chunk, which is currently optional. No test calls `http_decode_chunked` directly, so add hostile-input cases: an oversized size, a negative size, a missing CRLF and a truncated body.

## 2. The 999th fiber or channel breaks the program

**Where:** [fiber_runtime.c:165-170](../../compiler/runtime/fiber_runtime.c#L165-L170), [fiber_runtime.c:256-260](../../compiler/runtime/fiber_runtime.c#L256-L260), [fiber_runtime.c:413-417](../../compiler/runtime/fiber_runtime.c#L413-L417), [fiber.rs:44-52](../../crates/osprey-codegen/src/fiber.rs#L44-L52)

```c
int64_t id = next_id++;
if (id >= 1000) {
  pthread_mutex_unlock(&runtime_mutex);
  return -4; // Fiber array full
}
```

```c
int64_t fiber_await(int64_t fiber_id) {
  if (fiber_id < 1 || fiber_id >= 1000) {
    return -1;
  }
```

Fibers and channels draw from one `next_id` counter, with 999 usable IDs for the whole life of the process. IDs are not recycled when a fiber finishes or is awaited. `gen_spawn` passes the `-4` on as a normal handle, and `await` turns it into the value `-1`.

**Reproduced,** each with a loop that creates and finishes one at a time, so nothing was ever running concurrently:

| Program | After 999 IDs | Exit |
|---------|---------------|------|
| `await(spawn work(i))` returning `int` | Every later `await` returns `-1`; the program finishes and prints its last line | 0 |
| `await(spawn work(i))` returning `string` | `-1` is treated as a pointer and the program segfaults | 139 |
| `Channel(1)`, `send`, `recv` | `FATAL: Channel(1) has no channel to give` and abort | 134 |

**Impact:** the `int` case is the worst, because the work never runs, the answer is wrong, and the process reports success. Any long-running program that spawns per request or per job reaches this limit, however few fibers are alive at once. This is an unfinished design rather than an edge case: the fixed 1000-slot tables mean fibers can't be used in a server.

**Fix:** replace the tables with growable storage and recycle a slot when its handle is released. Exhaustion should then surface as a `Result` or a handled effect. A silent `-1` must not be reachable as a language value, and `channel_create_checked`'s `abort()` breaks the `AGENTS.md` rule that Osprey never aborts.

## 3. WebSocket server keeps dead connections

**Where:** [websocket_server_runtime.c:80](../../compiler/runtime/websocket_server_runtime.c#L80), [:94](../../compiler/runtime/websocket_server_runtime.c#L94), [:131](../../compiler/runtime/websocket_server_runtime.c#L131), [:303-304](../../compiler/runtime/websocket_server_runtime.c#L303-L304), [:335](../../compiler/runtime/websocket_server_runtime.c#L335)

When a client leaves, the connection thread closes the socket, but the `WebSocket` entry stays in `server->connections` with `is_connected` still true and the old descriptor number. Nothing removes entries or lowers `connection_count`. The OS then gives that descriptor number to the next socket the process opens, and `websocketServerBroadcast` writes to it.

**Reproduced:** client A connected and left, then client B connected. Five broadcasts each reported `recipients=2` with one live client, and B received 10 `hello` frames rather than 5.

**Impact:**

- **Wrong recipient:** the same reuse applies to any socket in the process, such as an outbound HTTP client or a file server connection, so WebSocket frames can be written into an unrelated connection. This follows from the descriptor handling; I only ran the WebSocket-to-WebSocket case.
- **Permanent lockout:** `connection_count` only rises, so after 1000 lifetime connections every new client completes the handshake and is then dropped.
- **Use-after-free on stop (by reading, not reproduced):** `websocket_stop_server` frees each `ws` at line 335, while the detached connection thread still reads `ws->is_connected` at line 94. That thread then closes its descriptor again at line 131, which can close an unrelated socket that has since taken the number.

**Fix:** have the connection thread remove its own entry under `server->mutex` before closing the socket. Give stop a way to shut down and wait for connection threads before freeing anything.

The same area is also unfinished. `parse_websocket_frame` rejects 64-bit payload lengths ([http_shared.c:237](../../compiler/runtime/http_shared.c#L237), `Not implemented for this simple version`) and ignores the FIN bit and opcode, so close and ping frames are handled as text.

## Runners-up

- **Compiler stack overflow:** `--check` aborts with `fatal runtime error: stack overflow` and exit 134 on 50,000 nested parentheses, 50,000 chained `+`, or 5,000 nested `[` in ML flavor. This needs an extreme or generated source file. The language server may share the weakness, but I did not test it.
- **Deep recursion in compiled programs:** non-tail recursion succeeded at a depth of 300,000 and segfaulted with no diagnostic at 1,000,000 (exit 139).
- **Unimplemented language features, rejected cleanly at compile time:** nested constructor patterns, and `Result` values in lists, maps and channels.

## What held up

- 1,500 random mutations of 79 regression programs caused no crash or hang in `--check`. This does not cover codegen or the language server.
- Out-of-range `substring` and list indexing return errors.

The mutation run found no Rust panics, but I did not audit the `panic!`, `unwrap` and `expect` sites one by one, so production Rust is not cleared of them.

## Not covered

This was a targeted scan, not an audit. I read only part of the C runtime, centred on the networking and fiber code; the GC, ARC, effects coroutines, JSON, and GPU and media code were not reviewed. Nothing was run on Linux, Windows, Android or wasm.
