---
layout: page
video: swift-interop
permalink: /docs/tutorials/swift-interop/
eleventyComputed:
  title: "{{ (video | video).name }}"
  description: "Watch Osprey and Swift build one app: Osprey exports become a C header Swift calls, Swift @_cdecl functions become Osprey imports, on the Mac and iPhone."
tags: [tutorials, video, macos, ios, swift, interop]
---

Osprey compiles to native code with a plain C interface ([the iOS target specification](/spec/0038-iostarget/) defines it), so a Swift app can link it like any C library. This video follows one real app, Osprey Storage, across that boundary. Osprey Storage is a disk browser for the Mac and the iPhone: it measures a folder, hidden files included, and ranks what takes up the space. Its logic is written in Osprey and its platform code in Swift.

{% videoPlayer video %}

## What the video covers

**Interop** here means two languages calling each other inside one process. The calls go both ways:

- **Swift calls Osprey.** The compiler builds the project into a static library, `libOspreyApp.a`, with a C header, `libOspreyApp.h`. Entry-source functions with supported scalar signatures become C prototypes that Swift calls through a bridging header.
- **Osprey calls Swift.** Osprey declares host functions with `extern`; Swift defines them with `@_cdecl`. Disk operations live in [`StorageServices.swift`](https://github.com/Nimblesite/osprey/blob/main/examples/macos/OspreyStorage/StorageServices.swift), and Photos operations in [`PhotoLibrary.swift`](https://github.com/Nimblesite/osprey/blob/main/examples/macos/OspreyStorage/PhotoLibrary.swift).

The video then shows how the app stays testable. Osprey only reaches the Swift functions through an [algebraic effect](/docs/effects/) handler at the boundary. A test handler answers the same effect with fixed data, so the whole browser runs in tests with no Swift and no disk. Finally, it shows one Xcode target building the same Osprey code for the Mac, the iPhone and the iPhone simulator.

The recording predates the Full Disk Access argument and Photos handler. Use the [current walkthrough](/docs/mac-and-iphone-apps/) for runnable code.

## Get it working

You need:

- An Apple silicon Mac with Xcode, selected with `xcode-select`. Add the iOS simulator runtime in Xcode if you want the iPhone build.
- [Rust](https://www.rust-lang.org/tools/install), to build the Osprey compiler.
- The LLVM/clang toolchain described in [Install Osprey](/docs/installation/).

Clone the repository and run the app from its root:

```sh
git clone https://github.com/Nimblesite/osprey.git
cd osprey
make -C apps run-mac-hd                    # build the compiler, the Mac runtime and the app, then open it
examples/macos/run.sh ios-sim      # build the same app and launch it in the iPhone simulator
```

Run the checks the video mentions:

```sh
target/release/osprey examples/macos/storage/test --run   # the Osprey tests, with no Swift and no disk
examples/macos/run.sh --smoke                             # the real walk and screens on this Mac
```

On Mac, the app starts at the whole disk. macOS can ask for access to protected folders during the walk; refused folders count as unreadable. Without Full Disk Access, other apps' data stays guarded until you choose a folder in the system picker or grant access in Settings. The [example's README](https://github.com/Nimblesite/osprey/tree/main/examples/macos#permissions) explains the permission flow.

Choose a folder to rank it, click a folder to open it, or use the eye button (⇧⌘.) to exclude hidden files from the totals. On Mac, right-click a folder to open it in Finder. The iPhone app measures its own container and, with permission, the photo library.

## Read the walkthrough

[Build a Mac and iPhone app with Osprey and Swift](/docs/mac-and-iphone-apps/) writes out the same steps with the full code. For an app that also handles SQLite and HTTP through effects, see [iOS and Android apps](/docs/mobile-apps/). Osprey is actively evolving.

## Transcript

{% videoTranscript video %}
