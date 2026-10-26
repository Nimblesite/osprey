# iOS Application Target [IOS-TARGET]

Osprey application logic compiles through LLVM into an ARM64 iOS static library with a generated C header. A Swift application links that library, owns the iOS lifecycle and platform APIs, and calls Osprey through the C ABI. This target covers iPhone devices, the Apple silicon iOS Simulator and, through the [Mac slice](#mac-host-ios-macos-host), Apple silicon Macs. [Android](0039-AndroidTarget.md) has a separate target contract and shares the [reactive mobile application](../applications/0040-ReactiveMobileApplications.md) boundary.

The test layers, what hosted CI executes, and the state of signed-device evidence are in [Verification](#verification-ios-verification).

Compiler and language rules are separate from integration tooling. See [iOS code generation](../compiler/iOSCodegen.md) for the extracted contract.

## Swift application [IOS-SWIFT-HOST]

[`examples/ios/`](../../../examples/ios) contains Osprey counter logic, a checked-in SwiftUI Xcode project, a bridging header that imports the generated header, and build/launch scripts. Swift owns UI state and calls Osprey for increments, decrements, resets, thresholds, and message formatting. Osprey invokes the Swift host callback when the count changes.

The project links separate device/simulator archives through SDK-specific build settings. `build.sh ios` and `build.sh ios-sim` generate each archive before running `xcodebuild`; unsigned device output can be opened in Xcode for signing and installation.

## Mac host [IOS-MACOS-HOST]

`--target=macos` compiles the same boundary for a Swift macOS host ([IOS-TARGET-MACOS](../compiler/iOSCodegen.md#the-mac-slice-ios-target-macos)), so Osprey on a Mac calls Swift libraries the way an iPhone app does: Swift wraps an Apple API in a `@_cdecl` function over the boundary types, and Osprey declares it with `extern`. A SwiftUI app builds through [`examples/ios/build.sh`](../../../examples/ios/build.sh) exactly as the iPhone apps do, with `macos` as its target: one Xcode target lists `macosx`, `iphoneos` and `iphonesimulator`, and its xcconfig picks the archive for the SDK being built. A Swift host without a bundle, such as the graphics scenes, builds through [`scripts/macos-host.sh`](../../../scripts/macos-host.sh). Both compile the project to `libOspreyApp.a` and `libOspreyApp.h` and link the Swift sources through a bridging header.

[`examples/macos/`](../../../examples/macos) is a storage browser for the Mac and iPhone. Osprey owns its state, events, ranking, hidden-file rules, formatting and native UI tree; Swift renders that tree and answers a `Disk::Facts` effect using Foundation volume capacities and a filesystem walk. The walk records sizes with and without hidden items and counts unreadable folders. The current Mac walk enters Desktop, Documents, Downloads and iCloud Drive, waiting for any system permission response; refusal contributes to the unreadable count. Without Full Disk Access, Osprey guards other apps' container folders; choosing one in the system picker requests access, measures it and adds it to the existing totals. The app also offers the Full Disk Access settings route. Folder context menus request Finder reveal only within the measured tree. A fixed handler exercises the same browser in the native domain suite. See the [application guide](../../../examples/macos/README.md) for the host permission flow.

[`examples/graphics/`](../../../examples/graphics) runs three real-time scenes this way. Each scene compiles with `--entry-only`, because its host calls only `osprey_main`, which runs the scene's own frame loop; a Swift Metal host defines the six `osp_gfx_*` imports, exchanging the window as a `Ptr`.

`make macos-test` runs the C ABI fixture above natively, then the whole corpus through the Mac slice against the native goldens and [`MOBILE_UNPORTABLE.txt`](../../../tests/MOBILE_UNPORTABLE.txt), the storage domain suite, and the storage app's smoke assertions, which drive the browser through the events the app sends. The smoke measures a folder with known contents, a hidden `.git`, a hidden `.cache` and an unreadable folder among them. Its prerequisite `make macos` builds every graphics host; `make graphics-smoke` renders each scene, which needs a desktop session and a Metal GPU. Hosted CI runs `make macos-test` beside `make ios-test` on the `macos-15` runner.

## Verification [IOS-VERIFICATION]

`make ios-test` builds both platform variants and runs three layers of checks.

The C ABI fixture in [`scripts/mobile-abi.osp`](../../../scripts/mobile-abi.osp) and its C host link for the device and execute in the simulator. Its assertions cover what a header cannot state: integers at both 64-bit extremes, overflow recovered by an explicit `Arith` handler, doubles round-tripping, C booleans passed through registers under both bool ABIs, UTF-8 strings crossing intact in both directions, an empty and a 4096-byte string, an Osprey-allocated string reaching a host import, and a borrowed host buffer being copied rather than aliased. Android runs the same fixture, so one file states the boundary contract once.

`make _test_ios_goldens` then runs the WHOLE `tests/` corpus through this target: each accepted program is built as an entry-only library, linked into a C host and executed in an iPhone simulator, then held to the byte-exact stdout the native backend produces. Programs the compiler rejects are pinned by name and reason in [`MOBILE_UNPORTABLE.txt`](../../../tests/MOBILE_UNPORTABLE.txt), which iOS and Android share because they share one boundary implementation; a new, removed or changed entry fails the harness, and an unexpected clang, linker or simulator failure is a failure rather than a skip. Seven hand-picked programs used to stand in for this and could not have noticed a boundary that truncated a string or miscompiled arithmetic inside an archive.

Finally the actual SwiftUI app's smoke assertions run. They cover initialization exactly once, scalar and allocated string returns, checked arithmetic, and calls back into Swift. The launch script removes old output and requires a fresh success marker from the app sandbox.

Compiler unit tests pin target selection, options, inferred header types, symbol collision rejection, boolean/Unit adaptation, initialization, and target capability diagnostics. Unsupported effect resumption and unavailable APIs have negative tests that run before SDK discovery, so target legality remains verifiable on machines without Xcode.

Hosted CI is configured to run `make ios-test mobile-ios-test` on an ARM64 `macos-15` runner for every pull request. It builds the device archive and links its C host, then executes the fixture, the corpus and both application smokes in an iPhone simulator, ending in `OSPREY_IOS_SMOKE_OK` and `OSPREY_INBOX_SMOKE_OK`. The runner and a phone share one instruction set, so what a device adds is signing, provisioning and device-only OS behaviour rather than different code generation.

A signed build on real hardware is outside what a hosted runner can do, so it is a manual check. `OSPREY_DEVELOPMENT_TEAM=<team-id> examples/mobile/ios/run.sh ios-device` builds, signs, installs and launches the [Issue Inbox](../applications/0040-ReactiveMobileApplications.md) on a connected, unlocked iPhone; launching the installed application with `--inbox-smoke` runs the assertions the simulator runs and writes the same marker to its sandbox. The signed application passed that smoke and displayed live GitHub issues on an iPhone 16. That run predates [issue-description Markdown](../applications/0040-ReactiveMobileApplications.md#issue-description-markdown-mobile-markdown) and the callable handler migration. A pre-migration build compiled, signed and installed on the same phone on 5 October 2026, but iOS refused to launch it on a locked phone, so that device smoke did not execute. Neither that installation nor earlier simulator evidence verifies the callable handler migration.

Simulator, device, and hosted CI execution evidence predates the callable handler migration. Current application validation and commands are recorded in [the application README](../../../examples/mobile/README.md#validation); platform execution must be refreshed before claiming current acceptance.

## Host build integration

`make _runtime_ios` creates `libosprey_runtime_ios.a`; `make _runtime_ios_sim` creates `libosprey_runtime_ios_sim.a`. `make ios` builds both runtimes, the compiler, and both SwiftUI sample apps. Signing and deployment are performed by Xcode, outside the Osprey compiler.

## Host callbacks

The sample implements `ios_host_log(const char *) -> int64_t` in Swift with `@_cdecl`, copies the incoming string, and calls Foundation's `NSLog`. Other iOS APIs belong behind the same host boundary. C/Swift interoperability remains outside Osprey's memory-safety guarantee.
