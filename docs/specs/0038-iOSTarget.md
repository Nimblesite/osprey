# iOS Application Target [IOS-TARGET]

Osprey application logic compiles through LLVM into an ARM64 iOS static library with a generated C header. A Swift application links that library, owns the iOS lifecycle and platform APIs, and calls Osprey through the C ABI. This target covers iPhone devices and the Apple silicon iOS Simulator. Android and other host platforms are outside this implementation.

Implementation sequence, test evidence, and remaining validation are tracked in [plan 0029 — iOS C ABI application logic and Swift host](../plans/0029-ios-c-abi.md).

## Platform selection [IOS-TARGET-TRIPLE]

| CLI target | LLVM target triple | Xcode SDK | Minimum OS |
| --- | --- | --- | --- |
| `--target=ios` | `arm64-apple-ios15.0` | `iphoneos` | iOS 15 |
| `--target=ios-sim` | `arm64-apple-ios15.0-simulator` | `iphonesimulator` | iOS 15 |

Device and simulator archives are separate products even though both use ARM64. The host must link the archive for its platform. Intel simulators, universal archives, and XCFramework packaging are not implemented.

## Compiler options [IOS-TARGET-OPTIONS]

```sh
osprey app.osp --compile --target=ios -o libApp.a
osprey app.osp --compile --target=ios-sim -o libApp.a
```

Compilation emits the requested `.a` and a sibling `.h`. Other output extensions are rejected. `--run` is rejected because the result needs an iOS host. `--memory=default` is the only supported memory backend; `--memory=gc` and `--memory=arc` are compiler errors. Native debugger and profiler flags are rejected for these targets. These option restrictions also apply to `--check` and `--llvm`. `--llvm` emits the host-compatible LLVM representation after the same target legality checks; it cannot bypass those checks.

## Compile and link [IOS-TARGET-LINK]

The compiler resolves the selected SDK with `xcrun`, compiles LLVM IR with Apple clang and the target triple above, and merges the application object and matching Osprey runtime archive using `xcrun libtool -static`. The generated archive contains the Osprey runtime; the Swift host links this single library. SDK/toolchain failures are errors with a setup hint. `OSPREY_XCRUN` overrides the `xcrun` executable.

`make _runtime_ios` creates `libosprey_runtime_ios.a`; `make _runtime_ios_sim` creates `libosprey_runtime_ios_sim.a`. `make ios` builds both runtimes, the compiler, and both SwiftUI sample apps. Signing and deployment are performed by Xcode, outside the Osprey compiler.

## C function boundary [IOS-HOST-ABI]

The header is derived from inferred types. Every emitted top-level function with a fully resolved supported scalar signature is exposed as `osprey_<name>`. Qualified project names use underscores between namespace segments. The application entry is reserved as `osprey_main`. Invalid or colliding generated C names are compile errors.

| Osprey type | C type | Swift imported type |
| --- | --- | --- |
| `int` | `int64_t` | `Int64` |
| `float` | `double` | `Double` |
| `bool` | `bool` | `Bool` |
| `string` | `const char *` | `UnsafePointer<CChar>?` |
| `Unit`, return only | `void` | `Void` |

Boolean boundary thunks use Apple's ARM64 zero-extension convention. `Unit` thunks translate the internal unit representation to C `void`. Integers retain their 64-bit Osprey width. See Apple's [ARM64 calling convention](https://developer.apple.com/documentation/xcode/writing-arm64-code-for-apple-platforms) and LLVM's [parameter attributes](https://llvm.org/docs/LangRef.html#parameter-attributes).

Records, unions including `Result`, collections, closures, unconstrained generic functions, and effects are not C values. Such internal helpers are not exported. The compiler rejects unsupported `extern fn` signatures and scalar exports that require an effect handler supplied by their caller; use a scalar wrapper that handles effects inside Osprey. Compile-time omission of an internal helper from the header does not declare that helper callable by C.

Strings are NUL-terminated UTF-8. Host inputs are borrowed for the synchronous call, must remain valid until it returns, and must not be retained by Osprey. Hosts must copy Osprey output strings before the next call into the library and must not free those pointers. Embedded NUL bytes are outside this string boundary contract. General allocations in the default memory backend remain alive for the process lifetime; a stable host-facing release API is not implemented.

## Entry and lifetime [IOS-TARGET-ENTRY]

The library exposes `int32_t osprey_main(void)` and no executable `main` symbol. The Swift host calls `osprey_main()` on its main thread before any exported application function. Successful initialization returns zero. Repeated calls return the saved result without running top-level work again. Top-level Osprey values remain valid after initialization so exported functions may use them.

The host serializes entry and exported function calls on one thread. Reentrant initialization returns status one without rerunning top-level work. Concurrent initialization, independently initialized library instances, unloading, and teardown are not part of this contract. Host callbacks must return synchronously and avoid reentering initialization. Swift owns asynchronous iOS work and marshals subsequent calls back to the chosen host thread.

## Calling the host [IOS-HOST-IMPORTS]

Osprey declares platform services using `extern fn` over the scalar types above. The generated header lists those imports with their declared C names, and the final Swift application supplies matching C symbols. Missing host definitions fail the final application link. These are deliberate host imports, distinct from an unsupported language or runtime capability, which must fail in the Osprey compiler.

The sample implements `ios_host_log(const char *) -> int64_t` in Swift with `@_cdecl`, copies the incoming string, and calls Foundation's `NSLog`. Other iOS APIs belong behind the same host boundary. C/Swift interoperability remains outside Osprey's memory-safety guarantee.

## Target capability checking [IOS-TARGET-CAPABILITIES]

The compiler checks the complete program before generating LLVM IR or invoking a native tool. An unsupported construct in a helper is rejected even when a top-level execution path does not call that helper. The diagnostic names the target, offending construct or operation, and the unsupported capability. `--check`, `--compile`, and `--llvm` enforce the same rules.

Explicit `resume` and resumable effect handling are rejected for this C ABI target. A suspended Osprey continuation cannot cross or outlive a synchronous host call. Substituting effect handlers and effects eliminated at compile time remain available when their bodies use supported operations. Exported functions must discharge their own effect requirements.

Process launching and the built-in HTTP/WebSocket operations have no iOS implementation and are rejected at compile time. The Swift host supplies platform networking and services through explicit imports. The runtime includes allocation, strings, collections, JSON, file operations, fibers/channels, and ordinary effect handling. This does not authorize an effect continuation to cross the C boundary. GPU buffers and pure kernels use the existing CPU execution path; Metal execution is not implemented.

The compiler must never present an unsupported target feature as a successful static-library build and defer its discovery to the Swift linker. Missing user-supplied host symbols remain ordinary application link errors. Expanding target support requires implementation and passing target tests before removing a rejection.

## Swift application [IOS-SWIFT-HOST]

[`examples/ios/`](../../examples/ios/) contains Osprey counter logic, a checked-in SwiftUI Xcode project, a bridging header that imports the generated header, and build/launch scripts. Swift owns UI state and calls Osprey for increments, decrements, resets, thresholds, and message formatting. Osprey invokes the Swift host callback when the count changes.

The project links separate device/simulator archives through SDK-specific build settings. `build.sh ios` and `build.sh ios-sim` generate each archive before running `xcodebuild`; unsigned device output can be opened in Xcode for signing and installation.

## Verification [IOS-VERIFICATION]

`make ios-test` builds both platform variants and runs C ABI tests, supported language goldens in the simulator, and the actual SwiftUI app's smoke assertions. The Swift assertions cover initialization exactly once, scalar and allocated string returns, checked arithmetic, and calls back into Swift. The launch script removes old output and requires a fresh success marker from the app sandbox.

Compiler unit tests pin target selection, options, inferred header types, symbol collision rejection, boolean/Unit adaptation, initialization, and target capability diagnostics. Unsupported effect resumption and unavailable APIs have negative tests that run before SDK discovery, so target legality remains verifiable on machines without Xcode.
