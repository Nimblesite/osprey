---
mlTwins: manual
layout: page
title: Build a Mac and iPhone app with Osprey and Swift
description: "Build Osprey Storage, a Mac and iPhone disk browser: Osprey exports in a generated C header, Swift @_cdecl imports, and one Xcode target for every Apple SDK."
permalink: /docs/mac-and-iphone-apps/
tags: [macos, ios, swift, applications, interop]
---

Osprey Storage is a native disk browser for the Mac and iPhone. The Mac app starts at the whole disk, with hidden files included. On iPhone it measures its own container and, with permission, the photo library. Osprey owns the state, ranking and screens; Swift draws them and calls the platform APIs.

{% videoCard "swift-interop", "/docs/tutorials/swift-interop/" %}

The source is in [`examples/macos/`](https://github.com/Nimblesite/osprey/tree/main/examples/macos). You need an Apple silicon Mac with Xcode and the [Osprey toolchain](/docs/installation/).

## 1. Run it

```sh
make -C apps run-mac-hd                    # rebuild the compiler, the Mac runtime and the app, then open it
examples/macos/run.sh ios-sim      # build and launch it in the iPhone simulator
```

## 2. Who does what

| Osprey | Swift |
| --- | --- |
| The browser's state and events | The window and the toolbar |
| The ranking and the hidden-file rule | The folder picker |
| Size and count formatting | The disk walk and PhotoKit measurements |
| Every screen, as a tree of native nodes | Drawing that tree with SwiftUI |

## 3. Export the app entry points

[`storage/src/main.ospml`](https://github.com/Nimblesite/osprey/blob/main/examples/macos/storage/src/main.ospml) exports `start` and `dispatch` for the browser, plus the formatting helpers `formatBytes`, `formatCount` and `percent`. `start` returns the first screen and requests a disk walk; `dispatch` takes the previous state and one event, then returns the next screen:

```osprey-ml
start (home, platform, full) = envelope (Browse::start home platform full)

(** The next screen after one host event, given the browser the last screen returned. *)
dispatch (model, event) = envelope (Browse::dispatch (Browse::decode model) event)
```

`platform` is `"macos"` or `"ios"`. `full` reports whether the Mac has Full Disk Access; the iPhone host passes `false`.

## 4. The compiler writes a C header

The Mac target compiles the project into a static library and a header beside it:

```sh
osprey examples/macos/storage --compile --target=macos --memory=arc -o examples/macos/build/macos/libOspreyApp.a
```

Every export gets a plain C prototype. An Osprey `string` is a `const char *`, an `int` is an `int64_t`, and a `bool` is a C `bool`:

```c
// fn storage::start(home: string, platform: string, full: bool) -> string
const char *osprey_storage_start(const char *home, const char *platform, bool full);
// fn storage::dispatch(model: string, event: string) -> string
const char *osprey_storage_dispatch(const char *model, const char *event);
```

A returned string carries an Osprey reference valid until `osprey_release_string` releases it. `StorageStore.accept` copies the result into Swift values and releases that reference, including when JSON decoding fails. Host string inputs need remain valid only through their synchronous call; the export adapter copies any string Osprey retains or returns.

## 5. Swift calls Osprey

`Bridge.h` includes the generated header, and the target uses it as its bridging header, so Swift sees the prototypes directly. [`StorageStore.swift`](https://github.com/Nimblesite/osprey/blob/main/examples/macos/OspreyStorage/StorageStore.swift) initializes Osprey once, asks for the first screen, and sends every event back through `dispatch`, keeping the state as opaque text:

```swift
let status = osprey_main()
guard status == 0 else { return fail("Osprey initialization failed: \(status)") }
accept(osprey_storage_start(canonical(NSHomeDirectory()), StorageStore.platform, fullDiskAccess()))
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
        photos => Photos(status = storage_photos_status (), items = storage_photos_items (), bytes = storage_photos_bytes (), cloud = storage_photos_cloud (), kinds = photoKinds ())
    View::render model
```

## 8. Test it without Swift

The domain suite answers the same effect with fixed data, so the browser and every screen run natively with no Swift and no disk:

```osprey-ml
screen model =
    handle Disk::Facts
        volume _ => ssd
        listing path => listingFor path
        photos => storage::Photos(status = "unavailable", items = 0, bytes = 0, cloud = 0, kinds = [])
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

Osprey is actively evolving. See [iOS and Android apps](/docs/mobile-apps/) for an app that also handles SQLite and HTTP through effects.
