---
mlTwins: manual
layout: page
title: Build a Mac and iPhone app with Osprey and Swift
description: "Build Osprey Storage, a Mac and iPhone disk browser: Osprey exports in a generated C header, Swift @_cdecl imports, and one Xcode target for every Apple SDK."
permalink: /docs/mac-and-iphone-apps/
tags: [macos, ios, swift, applications, interop]
---

Osprey Storage is a native disk browser for the Mac and the iPhone. It measures a folder, hidden files included, ranks what takes the space, and opens any folder to rank what is inside. Its logic is Osprey and its platform code is Swift. This tutorial follows the boundary between them.

{% videoCard "swift-interop", "/docs/tutorials/swift-interop/" %}

Prefer to watch? The [video tutorial](/docs/tutorials/swift-interop/) shows the real app and every step below in three minutes, with chapters and a transcript.

The source is in [`examples/macos/`](https://github.com/Nimblesite/osprey/tree/main/examples/macos). You need an Apple silicon Mac with Xcode and the [Osprey toolchain](/docs/installation/).

## What you will learn

- What Osprey owns and what Swift owns in one app.
- How Osprey exports become C functions that Swift calls.
- How Swift functions become imports that Osprey calls.
- How an effect handler keeps those imports at the edge, so the logic is testable without Swift.
- How one Xcode target links the right Osprey library for the Mac, the iPhone and the simulator.

## 1. Run it

```sh
make run-mac-hd                    # rebuild the compiler, the Mac runtime and the app, then open it
examples/macos/run.sh ios-sim      # build and launch it in the iPhone simulator
```

## 2. Who does what

| Osprey | Swift |
| --- | --- |
| The browser's state and events | The window and the toolbar |
| The ranking and the hidden-file rule | The folder picker |
| Size and count formatting | The walk over the disk |
| Every screen, as a tree of native nodes | Drawing that tree with SwiftUI |

## 3. Osprey exports two functions

[`storage/src/main.ospml`](https://github.com/Nimblesite/osprey/blob/main/examples/macos/storage/src/main.ospml) exposes the app. `start` returns the first screen for a home folder; `dispatch` takes the state the last screen came with and one event, and returns the next screen:

```osprey-ml
(** The first screen for `home` on `platform` (`macos` or `ios`), with its walk requested. *)
start (home, platform) = envelope (Browse::start home platform)

(** The next screen after one host event, given the browser the last screen returned. *)
dispatch (model, event) = envelope (Browse::dispatch (Browse::decode model) event)
```

## 4. The compiler writes a C header

The Mac target compiles the project into a static library and a header beside it:

```sh
osprey examples/macos/storage --compile --target=macos -o examples/macos/build/macos/libOspreyApp.a
```

Every export gets a plain C prototype. An Osprey `string` is a `const char *`, an `int` is an `int64_t`, and a `bool` is a C `bool`:

```c
// fn storage::start(home: string, platform: string) -> string
const char *osprey_storage_start(const char *home, const char *platform);
// fn storage::dispatch(model: string, event: string) -> string
const char *osprey_storage_dispatch(const char *model, const char *event);
```

A returned string belongs to the Osprey runtime: copy it before the next call into the library.

## 5. Swift calls Osprey

`Bridge.h` includes the generated header, and the target uses it as its bridging header, so Swift sees the prototypes directly. [`StorageStore.swift`](https://github.com/Nimblesite/osprey/blob/main/examples/macos/OspreyStorage/StorageStore.swift) initializes Osprey once, asks for the first screen, and sends every event back through `dispatch`, keeping the state as opaque text:

```swift
let status = osprey_main()
guard status == 0 else { return fail("Osprey initialization failed: \(status)") }
accept(osprey_storage_start(canonical(NSHomeDirectory()), StorageStore.platform))
// ...
accept(osprey_storage_dispatch(model, json))
```

## 6. Osprey calls Swift

Osprey declares each host function it needs with `extern`:

```osprey-ml
extern storage_scan (folder : string) -> int
```

The same header lists it as an import, a function the host must define with C linkage:

```c
// extern fn storage_scan(folder: string) -> int
int64_t storage_scan(const char *folder);
```

Swift defines it with `@_cdecl`, so the linker finds exactly that C name and signature. [`StorageServices.swift`](https://github.com/Nimblesite/osprey/blob/main/examples/macos/OspreyStorage/StorageServices.swift):

```swift
@_cdecl("storage_scan")
func storageScan(_ folder: UnsafePointer<CChar>?) -> Int64 {
    listed = folder.flatMap { DiskTree.current?.listing(of: String(cString: $0)) } ?? .none
    return listed.measured ? Int64(listed.entries.count) : -1
}
```

## 7. An effect at the boundary

The browser's logic never calls those externs. Its screens are rendered through a `Disk::Facts` effect, and the handler at the boundary answers it with the Swift functions:

```osprey-ml
screen model =
    handle Disk::Facts
        volume path => Volume(name = storage_volume_name path, total = storage_volume path "total", available = storage_volume path "available", important = storage_volume path "important")
        listing folder => listed (storage_scan folder)
    View::render model
```

## 8. Test it without Swift

The domain suite answers the same effect with fixed data, so the browser and every screen run natively with no Swift and no disk:

```osprey-ml
screen model =
    handle Disk::Facts
        volume _ => ssd
        listing path => listingFor path
    View::render model
```

```sh
target/release/osprey examples/macos/storage/test --run
```

## 9. Screens are data

Each screen Osprey returns is a tree of native nodes. The Hidden Files toggle is a node with its value and the event it sends:

```osprey-ml
visibility model = Native::node "toggle" [
    Native::field "text" "Hidden Files",
    Native::field "value" (toString model.hidden),
    Native::field "symbol" (pick model.hidden "eye" "eye.slash"),
    Native::field "key" "command+shift+.",
    Native::event "hidden" "",
]
```

The shared SwiftUI renderer, the same one the iPhone apps use, draws it as a real toggle and sends the event back to Osprey:

```swift
if node.kind == "toggle" {
    Toggle(isOn: Binding(get: { node.value == "true" }, set: { on in
        var event: [String: Any] = node.event ?? [:]
        event["value"] = on ? "true" : "false"
        send(event)
    })) { Label(node.text ?? "", systemImage: node.symbol ?? "circle") }
        .toggleStyle(.button)
}
```

## 10. One target, three SDKs

[`OspreyStorage.xcconfig`](https://github.com/Nimblesite/osprey/blob/main/examples/macos/OspreyStorage.xcconfig) names the Osprey platform for each SDK and links the library built for it:

```text
OSPREY_PLATFORM[sdk=macosx*] = macos
OSPREY_PLATFORM[sdk=iphoneos*] = ios
OSPREY_PLATFORM[sdk=iphonesimulator*] = ios-sim
OSPREY_LIBRARY_DIR = $(SRCROOT)/build/$(OSPREY_PLATFORM)
HEADER_SEARCH_PATHS = $(inherited) "$(OSPREY_LIBRARY_DIR)"
LIBRARY_SEARCH_PATHS = $(inherited) "$(OSPREY_LIBRARY_DIR)"
OTHER_LDFLAGS = $(inherited) -lOspreyApp
SWIFT_OBJC_BRIDGING_HEADER = OspreyStorage/Bridge.h
```

`examples/ios/build.sh` compiles the Osprey project for the target it is given (`macos`, `ios` or `ios-sim`) before it runs `xcodebuild`, so the same Osprey code runs on the Mac and on iPhone, drawn with each platform's own controls.

## What happens underneath

The compiler lowers the Osprey project through LLVM into native code for the target, then archives it into `libOspreyApp.a` with a generated `libOspreyApp.h`. Swift links that archive like any C library. All calls into the library come from one host thread, and `osprey_main` runs first. The [iOS target specification](https://github.com/Nimblesite/osprey/blob/main/docs/specs/platforms/0038-iOSTarget.md) states the boundary contract and how it is verified.

## Recap

- Osprey exports become C prototypes in a generated header; Swift calls them through a bridging header.
- Osprey `extern` declarations become imports; Swift defines them with `@_cdecl`.
- An effect handler at the boundary is the only place those imports are called, so a test handler runs the same logic without Swift.
- One Xcode target picks the Osprey library for the SDK being built.

Osprey is actively evolving. See [iOS and Android apps](/docs/mobile-apps/) for an app that also handles SQLite and HTTP through effects.
