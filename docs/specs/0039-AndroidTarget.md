# Android Application Target [ANDROID-TARGET]

Osprey application logic compiles through LLVM into a native Android archive and generated C header. An Android application links the archive through a small JNI bridge and provides the lifecycle, native rendering, and platform services. The same source can target [iOS](0038-iOSTarget.md); the [reactive mobile application spec](0040-ReactiveMobileApplications.md) defines the shared application boundary. Delivery and verification are tracked in [plan 0030](../plans/0030-reactive-mobile-apps.md).

## Platform selection [ANDROID-TARGET-TRIPLE]

| CLI target | NDK target triple | Android ABI | Minimum API |
| --- | --- | --- | --- |
| `android-arm64` | `aarch64-linux-android26` | `arm64-v8a` | 26 |
| `android-x64` | `x86_64-linux-android26` | `x86_64` | 26 |

The archive must match the host application's ABI. The initial targets do not produce 32-bit ARM or x86 artifacts. The NDK documents the [target triple and API suffix convention](https://developer.android.com/ndk/guides/other_build_systems).

## Compiler options [ANDROID-TARGET-OPTIONS]

```sh
osprey app.osp --compile --target=android-arm64 -o libApp.a
osprey app.osp --compile --target=android-x64 -o libApp.a
```

Compilation emits the requested `.a` and sibling `.h`; other output extensions fail. `--memory=default` is the only supported backend. GC, ARC, native debugger/profiler flags, and `--run` are errors. The target checks also apply to `--check` and `--llvm`, before any external tool invocation.

## C function boundary [ANDROID-HOST-ABI]

The shared mobile ABI exposes emitted functions with fully resolved scalar signatures as `osprey_<name>`, replacing namespace separators with underscores. The scalar mappings and synchronous borrowing contract are the same as [the iOS C boundary](0038-iOSTarget.md#c-function-boundary-ios-host-abi): `int64_t`, `double`, C `bool`, UTF-8 `const char *`, and return-only `void` for `Unit`. Boolean attributes follow the selected Android ABI. Invalid names, collisions, unsupported extern signatures, and independently unhandled exported-function effects are compile errors.

## Entry and lifetime [ANDROID-TARGET-ENTRY]

`osprey_main()` initializes the library once, retains its status, and preserves application globals for subsequent calls. The host serializes all Osprey calls and copies returned strings before the next call. It must not free those pointers. The initial default allocator retains general allocations for process lifetime and supplies no library teardown or returned-string release API.

Records, collections, closures, and effect handlers remain internal. A scalar wrapper can expose them as an application-defined protocol. The sample uses ordinary UTF-8 byte arrays across JNI so Java's modified UTF-8 string convention cannot corrupt Osprey strings; see Android's [JNI string guidance](https://developer.android.com/ndk/guides/jni-tips).

## Target capability checks [ANDROID-TARGET-CAPABILITIES]

The compiler rejects resumable algebraic effects, including explicit `resume`, and unavailable native process, HTTP, and WebSocket operations. Diagnostics identify the target and offending operation before LLVM or linking. The same principle applies to [WASM](0022-WebAssemblyTarget.md) and iOS.

Supported substituting handlers and effects eliminated during compilation remain usable. Platform networking is provided by the host through the C ABI or an application command protocol. Explicit scalar host imports remain legal; the final Android application link must provide their implementations.

## Compile and link [ANDROID-TARGET-LINK]

The driver locates the Android NDK through `ANDROID_NDK_HOME` or `ANDROID_NDK_ROOT`, otherwise through the installed SDK. SDK discovery supports `ANDROID_HOME` and `ANDROID_SDK_ROOT`. It lowers LLVM with the NDK clang for the selected API and combines the application object with its matching runtime archive. Missing SDK, NDK, runtime, or toolchain prerequisites fail with setup diagnostics.

`make _runtime_android` builds both runtime archives. `make android` builds the compiler and the shared mobile application's debug APK. The sample links the C ABI and JNI adapter into `libosprey_inbox.so` for both ABIs and packages them in the APK. APK construction and Android deployment belong to the native host build, outside the Osprey compiler.

## Verification [ANDROID-VERIFICATION]

`make android-test` checks the native C ABI fixture, Default/ML language goldens on an Android device or emulator, and the actual application's deterministic reactive/SQLite smoke test. The separate live smoke performs real GitHub requests. ARM64 execution and x64 compilation are recorded separately; a packaged ABI is not evidence that it executed.

Implementation lives in [android.rs](../../crates/osprey-cli/src/android.rs), the shared [mobile ABI generator](../../crates/osprey-cli/src/ios_abi.rs), [target capability validator](../../crates/osprey-cli/src/target_capabilities.rs), and [Android runtime/build scripts](../../scripts/android.mk). Application sources and runnable commands are in [examples/mobile](../../examples/mobile/README.md).
