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

## Numeric widths [FFI-NUMERIC-WIDTH]

An `extern` parameter or return of a numeric type crosses the boundary as the C type of its row in [TYPE-NUMERIC-SIZED](../language/0004-TypeSystem.md#sized-numeric-types--type-numeric-sized): the LLVM type and the C type both come from the one numeric table (`crates/osprey-types/src/numeric.rs`), and the mobile ABI tables read the same row. `bool` remains `i1`.

| Type | LLVM | C | Parameter ABI |
| --- | --- | --- | --- |
| `int8` | `i8` | `int8_t` | `signext` |
| `int16` | `i16` | `int16_t` | `signext` |
| `int32` | `i32` | `int32_t` | — |
| `int` | `i64` | `int64_t` | — |
| `uint8` | `i8` | `uint8_t` | `zeroext` |
| `uint16` | `i16` | `uint16_t` | `zeroext` |
| `uint32` | `i32` | `uint32_t` | — |
| `float16` | `i16` (bits) | `uint16_t` (bits) | `zeroext` |
| `float32` | `float` | `float` | — |
| `float` | `double` | `double` | — |

The extension column applies to parameters and returns on Apple ARM64, x86 System V and wasm32. Signed 8- and 16-bit integers use `signext`; unsigned 8- and 16-bit integers and the binary16 bits use `zeroext`. AAPCS64 (including Android ARM64) and Windows x64 omit these attributes; 32-bit values need no extension attribute on these targets. The compiler's native calls, extern function values, C callbacks and mobile thunks preserve the selected convention. A C function taking `float` receives a `float` register, never a widened double. No numeric conversion is inserted on either side; a C signature that differs from the declaration is the program's error, as it is for any other extern.

```osprey
extern fn sqrtf(x: float32) -> float32
extern fn memset(p: Ptr, value: int32, count: int) -> Ptr
```

```osprey-ml
extern sqrtf (x : float32) -> float32
extern memset (p : Ptr, value : int32, count : int) -> Ptr
```

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

`[FFI-FILE-SERVER-FOLDER]` When `path` is a folder, the server serves the files in it by name: `/<name>` answers that file, typed by its extension (`.m3u8` as `application/vnd.apple.mpegurl`, `.ts` as `video/mp2t`, anything else as `application/octet-stream`). Ranges, HEAD and the status codes are the same as for one file. A name containing a slash, backslash or colon, or starting with a dot, gives `404` on every platform, so nothing outside the folder can be reached. A name that does not exist yet is waited for, for up to 120 seconds, because a renderer may still be writing it. Stopping the server ends the wait.

`[FFI-FILE-SERVER-STOP]` Stopping a server ends its connections before it returns, and only then releases what they read, so its port and slot can be reused at once. A connection still reading its request, or blocked on a peer that stopped reading, ends within a tenth of a second, and one streaming a file ends at its next chunk, so a stalled player can't hold the stop up. A request the peer leaves unfinished gets no answer.

## Visualiser Render [FFI-MEDIA-RENDER]

A Cast receiver shows still artwork for audio. A sender that wants the TV to move with the music sends video instead, rendered by the same archive:

```osprey
extern fn osprey_media_render(audio: string) -> string
extern fn osprey_media_unrender(folder: string) -> int
```

`osprey_media_render` makes a fresh `osprey-cast.*` folder under `TMPDIR` and at once writes `media.m3u8`: a complete VOD playlist of 4-second segments, computed from the duration `ffprobe` reports, so the receiver knows the whole length and can seek straight away. It then starts `ffmpeg` in the background. ffmpeg draws a constant-Q spectrum of the audio at 1280×720 and 30 fps and cuts `segNNNNN.ts` files on exactly those 4-second keyframes, each written under a temporary name and renamed when it is complete. A [FFI-FILE-SERVER-FOLDER] server on the folder waits for a segment that is not there yet. Rendering runs far faster than playback, so the wait happens only after a seek far ahead. Render returns the folder, or `""` when the file does not exist, ffmpeg or ffprobe is not on the `PATH`, or the platform has no `posix_spawn` (Windows). The sender then casts the audio as it is. `osprey_media_unrender` stops ffmpeg and deletes the folder, returning `0`, or `-1` for anything but a folder this process's render made and has not yet stopped. The runtime keeps its own record of each folder and child. It reads nothing from disk to decide what to stop, so a planted folder can't get a process killed or files deleted. At most 16 renders run at once, and render returns `""` past that. `compiler/runtime/net_runtime_tests.c` serves a folder over loopback, and `@osprey/chromecast` reaches both functions through its `Wire` effect.

## Databases Are Libraries [FFI-NO-DB-BUILTINS]

Database access is not compiler surface. Database drivers use `extern fn`
declarations; the SQLite example is tested in `tests/regressions/db/`.
