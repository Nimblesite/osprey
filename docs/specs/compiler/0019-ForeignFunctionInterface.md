# Foreign Function Interface

Osprey calls C-ABI functions through `extern fn` declarations. Declaration
grammar and ABI types are specified in [Syntax](../language/0003-Syntax.md#extern-declarations);
`--no-ffi` and `--sandbox` are specified in
[Security and Sandboxing](0016-SecurityAndSandboxing.md).

Each flavor has its own declaration spelling; both lower to `Stmt::Extern`.
ABI mapping, link directives, and linking are shared after that boundary
([FLAVOR-BOUNDARY]).

For an iOS or macOS application, the compiler also exposes Osprey functions to a C/Swift host: `--target=ios`, `--target=ios-sim` and `--target=macos` emit a static library with a generated scalar C header. Swift owns the platform APIs and supplies `extern fn` implementations. The narrower supported signatures, string ownership, initialization, and compile-time rejection of unsupported target features are specified by the [iOS application target](../platforms/0038-iOSTarget.md), with a runnable [SwiftUI example](../../../examples/ios) and a [Mac storage example](../../../examples/macos) that calls Swift. General native FFI features below must not be assumed available across that target's host boundary.

## Link Directives [FFI-LINK-DIRECTIVES]

A source comment directive links a system library at compile time:

```osprey
// @link: sqlite3        → clang -lsqlite3
// @linkdir: /opt/lib    → clang -L/opt/lib
```

Directives are read from the source file and passed to the compiler driver by
both `--run` and `--compile`. Each value is one process argument; no shell parses
it. Invalid library names or paths therefore fail in the compiler driver rather
than executing as commands.

A directive may name the operating system whose native executables it links:
`// @link(windows): ospgfx` and `// @linkdir(windows): examples/graphics` apply
when the compiler builds on Windows and are ignored elsewhere, so shared source
can carry one platform's library. `windows`, `macos` and `linux` are recognized.

An app-library target (`ios`, `ios-sim`, `macos`, `android-*`) builds an archive
whose host links every import, so it rejects an unqualified directive before any
toolchain work: link the library in the host application and declare its
functions with `extern` ([IOS-TARGET-CAPABILITIES](iOSCodegen.md#target-capability-checking-ios-target-capabilities)).
A directive qualified with a native operating system does not apply to those
targets.

## Unit Returns [FFI-UNIT-RETURN]

An extern whose return type is `Unit` calls a C `void` function. Its LLVM declaration and call use `void`, with no result register; the caller receives Osprey's `Unit` value. An omitted extern return type also means `Unit`. Declare the actual return type of a C function that returns a value, even when the caller discards it.

Osprey function values retain their normal calling convention. A wrapper around a `Unit`-returning extern calls the C function with its `void` ABI and returns Osprey's `Unit` value.

## The `Ptr` Type [FFI-PTR]

`Ptr` is an opaque C pointer (`i8*`). It also crosses the app-library C boundary as `void *` ([IOS-HOST-ABI](iOSCodegen.md#c-function-boundary-ios-host-abi)). It may appear in signatures and may be
stored or passed. It supports no arithmetic, dereference, or field access.

C out-parameters (`sqlite3_open(path, &db)`) use the runtime's **pointer cells** — themselves plain `extern fn` declarations against the bundled runtime archive, not builtins:

```osprey
extern fn osprey_ffi_cell() -> Ptr      // allocate a pointer-sized cell (pass where C expects T**)
extern fn osprey_ffi_deref(cell: Ptr) -> Ptr   // read back the pointer C wrote
extern fn osprey_ffi_free(cell: Ptr) -> int    // release the cell
extern fn osprey_ffi_null() -> Ptr             // a NULL argument
```

```osprey-ml
extern osprey_ffi_cell -> Ptr      // allocate a pointer-sized cell (pass where C expects T**)
extern osprey_ffi_deref (cell : Ptr) -> Ptr   // read back the pointer C wrote
extern osprey_ffi_free (cell : Ptr) -> int    // release the cell
extern osprey_ffi_null -> Ptr             // a NULL argument
```

## Callbacks [FFI-CALLBACKS]

A named top-level function passed where an `extern fn` expects a function parameter lowers to a raw C code pointer. A capture-free lambda is accepted the same way; a **capturing** lambda is a compile-time error (captures cannot cross the C boundary; use a named function).

## Raw TLS Sessions [FFI-TLS-CLIENT]

The native HTTP archive exports a protocol-agnostic TLS client for libraries that speak their own wire format. Bytes cross the boundary as lowercase hex text, two characters per byte, because an Osprey string cannot hold a NUL byte.

```osprey
extern fn osprey_tls_connect(host: string, port: int, timeoutMs: int) -> int
extern fn osprey_tls_send_hex(handle: int, hex: string) -> int
extern fn osprey_tls_recv_hex(handle: int, count: int, timeoutMs: int) -> string
extern fn osprey_tls_status(handle: int) -> int
extern fn osprey_tls_close(handle: int) -> int
extern fn osprey_net_local_address(host: string, port: int) -> string
```

`osprey_tls_connect` returns a handle of at least 1, or a negative reason: `-1` bad argument, `-2` the host does not resolve, `-3` no socket, `-4` refused or timed out, `-5` all 64 sessions in use, `-6` no TLS context, `-7` handshake failed. `osprey_tls_send_hex` returns `0`, `-1` for an unknown handle, `-2` for text that is not hex, or `-3` when the write failed. `osprey_tls_recv_hex` returns exactly `count` bytes, or `""` when they did not all arrive in time. `osprey_tls_status` then gives `0` ok, `1` timed out, `2` the peer closed, `3` failed, or `-1` for an unknown handle. A timed-out read keeps the bytes it has, so the next read of the same count resumes the frame. `osprey_net_local_address` names the local IPv4 interface that routes to a host, or `""`. A program that names any of these links the HTTP archive and OpenSSL. `compiler/runtime/net_runtime_tests.c` tests each result code against a loopback OpenSSL server.

`[FFI-TLS-UNVERIFIED]` The session does not verify the peer. It exists for local devices with self-signed certificates, such as a Cast device on port 8009. `httpCreateClient` keeps mandatory verification for `https://` ([HTTP-STATUS-CLIENT](../runtime/0014-HTTP.md#status-only-client-http-status-client)).

## One-File Media Server [FFI-FILE-SERVER]

`httpListen` returns the body as an Osprey string, which cannot carry a NUL byte, so it cannot serve media. The same archive serves one file instead:

```osprey
extern fn osprey_file_server_start(path: string, contentType: string, address: string) -> int
extern fn osprey_file_server_stop(port: int) -> int
```

`osprey_file_server_start` binds a fresh port on `address` and returns it, or `-1` bad argument, `-2` the path is not a readable regular file, `-3` all 16 servers in use, `-4` the address cannot be bound, `-5` no accept thread. The file is served at `/media` only, one thread per connection. GET returns `200` with the whole file, or `206` with `Content-Range` for `Range: bytes=start-` or `bytes=start-end` (an end past the file is clamped). A start past the end gives `416`. HEAD sends headers only. Any other path gives `404`, any other method `405`, and a malformed request line `400`. `osprey_file_server_stop` returns `0`, or `-1` when no server runs on that port.

## Databases Are Libraries [FFI-NO-DB-BUILTINS]

Database access is not compiler surface. Database drivers use `extern fn`
declarations; the SQLite example is tested in `tests/regressions/db/`.
