# Osprey on iPhone

This example runs Osprey application logic inside a SwiftUI iPhone app. Osprey decides how the counter changes and formats its messages. Swift owns the screen and the host callback that writes to the iOS log. The compiler produces a native static library and a C header that Swift imports directly.

## Build and launch

Use an Apple silicon Mac with full Xcode selected by `xcode-select`, the iOS SDKs, and an installed iPhone simulator runtime. The repository's normal Rust and LLVM build prerequisites also apply. Both targets require iOS 15 or later.

From the repository root:

```sh
make ios                       # Build unsigned iPhone and simulator apps
examples/ios/run.sh             # Build, install, and launch on an iPhone simulator
make ios-test                  # Check C ABI, language output, and the SwiftUI app
```

`run.sh` selects a booted iPhone when available, otherwise boots an installed iPhone simulator. Set `OSPREY_SIMULATOR_UDID` to choose one explicitly. Set `OSPREY_BIN` to use another compiler binary. `OSPREY_IOS_SKIP_BUILD=1 examples/ios/run.sh` launches the existing simulator build.

The products are `build/ios/products/OspreyCounter.app` and `build/ios-sim/products/OspreyCounter.app`. `make ios` leaves device builds unsigned.

To build and run on a physical iPhone, run `make ios` once to prepare the compiler and runtime, connect and unlock your phone, trust the Mac, and enable Developer Mode. Sign in to your Apple developer account in Xcode, then run:

```sh
OSPREY_DEVELOPMENT_TEAM=<your-team-id> examples/ios/run-device.sh
```

The script selects the only connected iPhone, recompiles `app.osp`, signs the app using Xcode, installs it, and opens it on the phone. With multiple phones, set `OSPREY_DEVICE_ID` to the identifier shown by `xcrun devicectl list devices`. The signed product is `build/ios-device/products/OspreyCounter.app`; signing and provisioning errors stop the script. You can also open `OspreyCounter.xcodeproj`, select your signing team and phone, and run after `make ios`.

After changing `app.osp`, rerun `examples/ios/build.sh ios-sim` or `examples/ios/build.sh ios` for the relevant platform. Xcode builds Swift sources against the already generated Osprey archive.

## The boundary

```sh
target/release/osprey examples/ios/app.osp --compile --target=ios-sim \
    -o examples/ios/build/ios-sim/libOspreyApp.a
```

This emits `libOspreyApp.a`, containing application code and the matching runtime, and `libOspreyApp.h`. `OspreyCounter/Bridge.h` includes that generated header. The app calls `osprey_main()` once before calling functions such as `osprey_increment(count)` and `osprey_summary(count)`.

The call back into Swift is declared in Osprey:

```osprey
extern fn ios_host_log(message: string) -> int
fn notify(count) = ios_host_log(summary(count))
```

`iosHostLog` implements that C symbol using Swift's `@_cdecl`, copies the incoming UTF-8 string, and uses Foundation's `NSLog`. The screen displays the copied message after every counter change. A platform service can follow the same pattern: expose a small scalar C function, implement it in Swift, and call it from Osprey.

Calls are synchronous and run on the main thread in this sample. Host strings are borrowed for the duration of the call; Swift copies returned strings immediately with `String(cString:)`. Do not free Osprey-owned strings or keep Swift temporary string pointers in Osprey. The first iOS target uses the default memory backend, which retains general allocations for the process lifetime; it does not yet provide a host release API for returned strings.

## Verification and supported scope

`examples/ios/run.sh --smoke` launches the actual app with assertions for initialization exactly once, scalar and UTF-8 string returns, checked integer overflow, decrement bounds, and the Swift callback. The app writes `OSPREY_IOS_SMOKE_OK` into its sandbox only if every assertion passes. The script removes any previous result before launch and fails on an assertion failure or missing result.

The boundary supports `int`, `float`, `bool`, `string`, and `Unit` returns. Keep records, collections, closures, and effect handlers inside Osprey and expose scalar wrapper functions. Unsupported target features, including explicit resumable effects and unavailable process/network APIs, produce compiler errors before LLVM or linking. Swift supplies platform services through the C boundary. See the [iOS target specification](../../docs/specs/0038-iOSTarget.md) for the exact contract and limitations.

Apple documents [importing C declarations into Swift](https://developer.apple.com/documentation/swift/imported-c-and-objective-c-apis) and [Xcode's command-line tools](https://developer.apple.com/documentation/xcode/xcode-command-line-tool-reference). No third-party Xcode project generator or package dependency is required.
