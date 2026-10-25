# Android code generation

The corresponding AndroidTarget contract is [here](../platforms/0039-AndroidTarget.md).

## Platform selection [ANDROID-TARGET-TRIPLE]

| CLI target | NDK target triple | Android ABI | Minimum API |
| --- | --- | --- | --- |
| `android-arm64` | `aarch64-linux-android26` | `arm64-v8a` | 26 |
| `android-x64` | `x86_64-linux-android26` | `x86_64` | 26 |

The archive must match the host application's ABI. The initial targets do not produce 32-bit ARM or x86 artifacts. The NDK documents the [target triple and API suffix convention](https://developer.android.com/ndk/guides/other_build_systems).

## Compiler options [ANDROID-TARGET-OPTIONS]

```sh
osprey app.osp --compile --target=android-arm64 -o libApp.a --memory=none
osprey app.osp --compile --target=android-x64 -o libApp.a --memory=none
```

Compilation emits the requested `.a` and sibling `.h`; other output extensions fail. Explicit `--memory=none` is the only supported backend. GC, ARC, native debugger/profiler flags, and `--run` are errors. The target checks also apply to `--check` and `--llvm`, before any external tool invocation.

## C function boundary [ANDROID-HOST-ABI]

The shared mobile ABI exposes the entry source's functions with fully resolved scalar signatures as `osprey_<name>`, replacing namespace separators with underscores. The scalar mappings and synchronous borrowing contract are the same as [the iOS C boundary](iOSCodegen.md#c-function-boundary-ios-host-abi): `int64_t`, `double`, C `bool`, UTF-8 `const char *`, and return-only `void` for `Unit`. Boolean attributes follow the selected Android ABI. Invalid names, collisions, unsupported extern signatures, and independently unhandled exported-function effects are compile errors.

`--entry-only` exposes only `osprey_main` for a host that does not call any other Osprey function. The corpus harness uses it; application archives still export and check their scalar functions by default.

## Entry and lifetime [ANDROID-TARGET-ENTRY]

`osprey_main()` initializes the library once, retains its status, and preserves application globals for subsequent calls. The host serializes all Osprey calls and copies returned strings before the next call. It must not free those pointers. The unmanaged `none` allocator retains general allocations for process lifetime and supplies no library teardown or returned-string release API.

Records, collections, closures, and effect handlers remain internal. A scalar wrapper can expose them as an application-defined protocol. The sample uses ordinary UTF-8 byte arrays across JNI so Java's modified UTF-8 string convention cannot corrupt Osprey strings; see Android's [JNI string guidance](https://developer.android.com/ndk/guides/jni-tips).

## Target capability checks [ANDROID-TARGET-CAPABILITIES]

The compiler rejects dynamic `control` operations, including arms that never resume, and unavailable native process, HTTP, and WebSocket operations. Diagnostics identify the target and offending operation before LLVM or linking. The same principle applies to [WASM](0022-WebAssemblyTarget.md) and iOS.

Callable value handlers, block-scoped value handlers, and effects eliminated during compilation remain usable. Platform networking is provided by the host through the C ABI or an application command protocol. Explicit scalar host imports remain legal; the final Android application link must provide their implementations.

## Compile and link [ANDROID-TARGET-LINK]

The driver locates the Android NDK through `ANDROID_NDK_HOME` or `ANDROID_NDK_ROOT`, otherwise through the installed SDK. SDK discovery supports `ANDROID_HOME` and `ANDROID_SDK_ROOT`. It lowers LLVM with the NDK clang for the selected API and combines the application object with its matching runtime archive. Missing SDK, NDK, runtime, or toolchain prerequisites fail with setup diagnostics.
