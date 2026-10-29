# iOS code generation

The corresponding iOSTarget contract is [here](../platforms/0038-iOSTarget.md).

## Platform selection [IOS-TARGET-TRIPLE]

| CLI target | LLVM target triple | Xcode SDK | Minimum OS |
| --- | --- | --- | --- |
| `--target=ios` | `arm64-apple-ios15.0` | `iphoneos` | iOS 15 |
| `--target=ios-sim` | `arm64-apple-ios15.0-simulator` | `iphonesimulator` | iOS 15 |
| `--target=macos` | `arm64-apple-macos12.0` | `macosx` | macOS 12 |

Device, simulator and Mac archives are separate products even though all three use ARM64. The host must link the archive for its platform. Intel Macs and simulators, universal archives, and XCFramework packaging are not implemented.

## The Mac slice [IOS-TARGET-MACOS]

`--target=macos` is the iPhone target built for a Mac: the same options, header, thunks, entry, capability rules and runtime sources (`libosprey_runtime_macos_arc.a` compiles the iOS runtime sources with the iOS flags against the macOS SDK). The Mac runtime also carries the Metal device adapter ([GPU-DEVICE]), so a desktop host such as GPUI runs kernels on the GPU, and a host linking the Mac archive links the Metal and Foundation frameworks. For one program, the Mac slice emits the same adapted LLVM IR as `--target=ios` and a header that differs only in the slice it names, so Swift `@_cdecl` imports and calls written against one compile unchanged against the other. A Swift macOS host, command-line or app, links the archive exactly as an iPhone app does and owns every platform API. `--run` is rejected for this slice too; the host runs the program.

## Compiler options [IOS-TARGET-OPTIONS]

```sh
osprey app.osp --compile --target=ios -o libApp.a --memory=arc
osprey app.osp --compile --target=ios-sim -o libApp.a --memory=arc
```

Compilation emits the requested `.a` and a sibling `.h`. Other output extensions are rejected. `--run` is rejected because the result needs an iOS host. ARC is the default memory backend; `--memory=arc` selects it explicitly. Tracing GC is unsupported for these hosted archives and is rejected before toolchain work. Unmanaged allocation remains an explicit `--memory=none` opt-out. Native debugger and profiler flags are rejected for these targets. These option restrictions also apply to `--check` and `--llvm`. `--llvm` emits the host-compatible LLVM representation after the same target legality checks; it cannot bypass those checks.

## Compile and link [IOS-TARGET-LINK]

The compiler resolves the selected SDK with `xcrun`, compiles LLVM IR with Apple clang and the target triple above, and merges the application object and matching Osprey runtime archive using `xcrun libtool -static`. The generated archive contains the Osprey runtime; the Swift host links this single library. SDK/toolchain failures are errors with a setup hint. `OSPREY_XCRUN` overrides the `xcrun` executable.

## C function boundary [IOS-HOST-ABI]

The header is derived from inferred types. Every function the entry source declares with a fully resolved supported scalar signature is exposed as `osprey_<name>`. A single-file program is its own entry source. Functions in a project's other source files are internal, whatever their signatures. If the compiler cannot emit an exposed function, the build fails and names it, with the reason when the effect check has one. The function is never silently missing from the archive and header. Qualified project names use underscores between namespace segments. The application entry is reserved as `osprey_main`. Invalid or colliding generated C names are compile errors.

`--entry-only` builds a mobile archive whose host can call only `osprey_main`. Internal functions remain compiled and checked at their call sites, but they are not C exports. The mobile golden harness uses this mode because its host calls only the entry point; ordinary application archives continue to validate every exported function's effect requirements.

| Osprey type | C type | Swift imported type |
| --- | --- | --- |
| `int` | `int64_t` | `Int64` |
| `float` | `double` | `Double` |
| `bool` | `bool` | `Bool` |
| `string` | `const char *` | `UnsafePointer<CChar>?` |
| `Ptr` | `void *` | `UnsafeMutableRawPointer?` |
| `Unit`, return only | `void` | `Void` |

A `Ptr` is an opaque host handle ([FFI-PTR](0019-ForeignFunctionInterface.md#the-ptr-type-ffi-ptr)): Osprey stores it and passes it back unchanged and never reads through it, so a host can hand Osprey a window or a session and receive it in later imports. The host owns the pointee and its lifetime.

Boolean boundary thunks use Apple's ARM64 zero-extension convention. `Unit` thunks translate the internal unit representation to C `void`. Integers retain their 64-bit Osprey width. See Apple's [ARM64 calling convention](https://developer.apple.com/documentation/xcode/writing-arm64-code-for-apple-platforms) and LLVM's [parameter attributes](https://llvm.org/docs/LangRef.html#parameter-attributes).

Records, unions including `Result`, collections, closures, unconstrained generic functions, and effects are not C values. Such internal helpers are not exported. The compiler rejects unsupported `extern fn` signatures and scalar exports that require an effect handler supplied by their caller; use a scalar wrapper that handles effects inside Osprey. Compile-time omission of an internal helper from the header does not declare that helper callable by C.

Strings are NUL-terminated UTF-8. Host inputs are borrowed for the synchronous call, must remain valid until it returns, and must not be retained by Osprey. Hosts must copy Osprey output strings before the next call into the library and must not free those pointers. Embedded NUL bytes are outside this string boundary contract. ARC reclaims internal values when their last owner is released. Application globals remain owned by the library, and returned strings have no host-facing release API. `--entry-only` has no callable exports and releases its program globals on return; its corpus must finish with zero live ARC allocations.

## Entry and lifetime [IOS-TARGET-ENTRY]

The library exposes `int32_t osprey_main(void)` and no executable `main` symbol. The Swift host calls `osprey_main()` on its main thread before any exported application function. Successful initialization returns zero. Repeated calls return the saved result without running top-level work again. Top-level Osprey values remain valid after initialization so exported functions may use them.

The host serializes entry and exported function calls on one thread. Reentrant initialization returns status one without rerunning top-level work. Concurrent initialization, independently initialized library instances, unloading, and teardown are not part of this contract. Host callbacks must return synchronously and avoid reentering initialization. Swift owns asynchronous iOS work and marshals subsequent calls back to the chosen host thread.

## Calling the host [IOS-HOST-IMPORTS]

Osprey declares platform services using `extern fn` over the scalar types above. The generated header lists those imports with their declared C names, and the final Swift application supplies matching C symbols. Missing host definitions fail the final application link.

A string an import returns is not copied: Osprey keeps the pointer and may retain it in its data for the rest of the process. The host must return memory that stays valid for the process lifetime, such as a heap copy it never frees, and must not reuse or release the buffer. These are deliberate host imports, distinct from an unsupported language or runtime capability, which must fail in the Osprey compiler.

## Target capability checking [IOS-TARGET-CAPABILITIES]

The compiler checks the complete program before generating LLVM IR or invoking a native tool. An unsupported construct in a helper is rejected even when a top-level execution path does not call that helper. The diagnostic names the target, offending construct or operation, and the unsupported capability. `--check`, `--compile`, and `--llvm` enforce the same rules.

Dynamic `control` operations, including arms that never resume, are rejected for this C ABI target. A suspended Osprey continuation cannot cross or outlive a synchronous host call. Callable value handlers, block-scoped value handlers, and effects eliminated at compile time remain available when their bodies use supported operations. Exported functions must discharge their own effect requirements.

Process launching and the built-in HTTP/WebSocket operations have no iOS implementation and are rejected at compile time. So is an unqualified `// @link:` or `// @linkdir:` directive: the archive carries no library, and the host links every import ([FFI-LINK-DIRECTIVES](0019-ForeignFunctionInterface.md#link-directives-ffi-link-directives)). The Swift host supplies platform networking and services through explicit imports. The runtime includes allocation, strings, collections, JSON, file operations, fibers/channels, ordinary effect handling, and the GPU surface on its host oracle; it carries no Metal adapter, so a device selection reports that no GPU adapter is available ([GPU-BACKEND-DEVICE](../runtime/0034-GPUComputation.md#device-backends--gpu-backend-device)). This does not authorize an effect continuation to cross the C boundary. GPU buffers and pure kernels use the existing CPU execution path; Metal execution is not implemented.

The compiler must never present an unsupported target feature as a successful static-library build and defer its discovery to the Swift linker. Missing user-supplied host symbols remain ordinary application link errors. Expanding target support requires implementation and passing target tests before removing a rejection.
