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

- **Swift calls Osprey.** The compiler builds the Osprey project into a static library, `libOspreyApp.a`, and writes a C header, `libOspreyApp.h`, beside it. Each Osprey `export` becomes a C prototype in that header, and Swift calls it through a bridging header.
- **Osprey calls Swift.** Osprey declares each host function it needs with `extern`. Swift defines that function with `@_cdecl`, so the linker finds exactly that C name. [`StorageServices.swift`](https://github.com/Nimblesite/osprey/blob/main/examples/macos/OspreyStorage/StorageServices.swift) holds every one of them.

The video then shows how the app stays testable. Osprey only reaches the Swift functions through an [algebraic effect](/docs/effects/) handler at the boundary. A test handler answers the same effect with fixed data, so the whole browser runs in tests with no Swift and no disk. Finally, it shows one Xcode target building the same Osprey code for the Mac, the iPhone and the iPhone simulator.

Every app shot in the video is the real app measuring a staged folder. Every line of code shown is quoted from the [`examples/macos`](https://github.com/Nimblesite/osprey/tree/main/examples/macos) folder of the repository.

## Get it working

You need:

- An Apple silicon Mac with Xcode, selected with `xcode-select`. Add the iOS simulator runtime in Xcode if you want the iPhone build.
- [Rust](https://www.rust-lang.org/tools/install), to build the Osprey compiler.
- The LLVM/clang toolchain described in [Install Osprey](/docs/installation/).

Clone the repository and run the app from its root:

```sh
git clone https://github.com/Nimblesite/osprey.git
cd osprey
make run-mac-hd                    # build the compiler, the Mac runtime and the app, then open it
examples/macos/run.sh ios-sim      # build the same app and launch it in the iPhone simulator
```

Run the checks the video mentions:

```sh
target/release/osprey examples/macos/storage/test --run   # the Osprey tests, with no Swift and no disk
examples/macos/run.sh --smoke                             # the real walk and screens on this Mac
```

The app opens on your home folder. Choose a folder to rank it, click any folder to open it, and use the eye button (⇧⌘.) to leave hidden files out of every total. Right-click a folder and choose Open in Finder to see it in a Finder window. The app never asks for disk access: it reads only folder metadata, the information Finder shows. The [example's README](https://github.com/Nimblesite/osprey/tree/main/examples/macos#permissions) explains how.

## Read the walkthrough

[Build a Mac and iPhone app with Osprey and Swift](/docs/mac-and-iphone-apps/) writes out the same steps with the full code. For an app that also handles SQLite and HTTP through effects, see [iOS and Android apps](/docs/mobile-apps/). Osprey is actively evolving.

## Transcript

{% videoTranscript video %}
