---
mlTwins: manual
layout: page
title: "Osprey Native: shared logic, native apps"
description: Share Osprey logic across macOS, Windows, iOS, Android and web apps. See native controls, AOT compilation and typed effects in running applications.
permalink: /docs/osprey-native/
tags: [native, applications, ui, gpui, vscode]
---

{% from "mobile-gallery.njk" import gallery %}
{% from "native-capture.njk" import capture %}

**Safety, elegance and performance:** typed effects for platform services, inferred types for concise code, and ahead-of-time LLVM compilation for native execution.

**macOS · Windows · iOS · Android · Web**

{% videoPlayer "agent-native", true %}

[Watch the agent walkthrough with chapters and transcript →](/docs/osprey-native/agent/)

**Osprey Agent is not sandboxed:** it can run commands and change files your account can.

## Native builds: no JavaScript engine to ship.

Osprey application logic compiles **ahead of time to native machine code** on desktop and mobile. Native builds ship no JavaScript engine or JavaScript application bundle; Osprey code needs no JIT warm-up. On the web, Osprey compiles to **WebAssembly**; its browser host can use JavaScript.

### Real platform controls

SwiftUI on Apple platforms. WinUI on Windows. Android widgets on Android. Native text fields, buttons and scrolling come from the platform. Choose GPUI for custom GPU-rendered scenes.

{{ gallery(mobile.screenshots) }}

**Issue Inbox:** search GitHub issues, save an issue, open its description and add a local note. Real iPhone simulator and Pixel emulator captures. [Build it →](/docs/mobile-apps/)

### The casting player on phones

{{ gallery(mobile.casting) }}

The mobile UI runs through real simulator/emulator journeys. Full playback integration, screenshot goldens and physical-device verification remain delivery work; [see the verified scope](https://github.com/Nimblesite/osprey/blob/main/src/osprey/examples/casting/mobile.md).

## See what you can build

### Windows: real WinUI, shared Osprey logic

{{ capture("/assets/images/native/windows-storage.png", "Osprey Storage in WinUI showing folder sizes, native buttons and disk usage", "Storage · Windows WinUI · browse folders, reveal hidden items and find what takes up space.", 1164, 741) }}

{{ capture("/assets/images/native/windows-flock.png", "Flock boids simulation running in the Windows host", "Flock · Windows · steer a live flock, pause it and resume.", 1168, 745) }}

{{ capture("/assets/images/native/windows-painting.png", "Painting on Windows showing coloured paths and native controls", "Painting · Windows · draw freehand, switch stroke styles and clear the canvas.", 1164, 741) }}

{{ capture("/assets/images/native/windows-casting.png", "Casting player on Windows with receiver and playback controls", "Casting · Windows · discover receivers and control playback.", 434, 557) }}

### macOS: tools, instruments and custom scenes

{{ capture("/assets/images/native/macos-synth.png", "Prism Synth native macOS app with a signal flow editor and piano keyboard", "Prism Synth · macOS · connect a patch, change its controls and play the keyboard.", 2304, 1512) }}

Light Orchestra turns any phone on your network into an instrument for your Mac: the synthesiser is Osprey, and you hear exactly what it played.

{% videoPlayer "light-orchestra", true %}

{{ capture("/assets/images/blog/chromecast-from-mac-player.webp", "Casting player on macOS with receiver selection, transport and volume", "Casting · macOS GPUI · choose audio and a receiver, then press Play.", 868, 1114) }}

{% videoPlayer "chromecast-from-mac", true %}

{{ capture("/assets/images/native/macos-painting.png", "Osprey Painting on macOS showing shapes, gradients and a freehand stroke", "Painting · macOS GPUI · GPU-rendered paths and gradients from an Osprey application.", 3128, 2122) }}

{% videoPlayer "rust-ui-framework-without-rust", true %}

### Web: WebAssembly logic, real browser UI

{{ capture("/assets/images/native/web-bank.png", "Talon Bank browser dashboard showing balances, accounts and activity", "Talon Bank · Chromium · Osprey WebAssembly owns the state and rules; React renders the browser UI.", 1440, 960) }}

[Run the bank →](/docs/web-apps/) · [Explore Proof Workbench →](/docs/proof-workbench/)

### VS Code: application UI inside your editor

{{ capture("/assets/images/packages/workspace-details.jpg", "Osprey package details inside VS Code. Audit unverified.", "Package workspace · VS Code · inspect a release and its trust information before adding it. Capture status: audit unverified.", 2400, 1600) }}

{% videoPlayer "package-manager", true %}

### Proof Workbench: mathematics as an application

{{ capture("/assets/images/guides/mathlib-library.png", "Proof Workbench showing a checked imported theorem and its propext assumption", "Proof Workbench · recorded tutorial frame · an imported theorem is checked under its visible propext assumption.", 1920, 1080) }}

Independent Mathlib operation remains delivery work; [inspect the current evidence](/docs/mathlib-port/).

{% videoPlayer "proof-workbench", true %}

## Compare the application models

| Requirement | How Osprey Native handles it |
| --- | --- |
| Native execution and native controls | For native builds: AOT Osprey code plus SwiftUI, WinUI or Android widgets, with coverage depending on the host and app. React Native also uses native components, but runs JavaScript through an engine such as [Hermes](https://reactnative.dev/docs/hermes). |
| Platform identity and custom graphics | Native widget hosts preserve platform controls; GPUI handles custom scenes. Compose Multiplatform's shared UI uses [Skia](https://kotlinlang.org/docs/multiplatform/kotlin-multiplatform-react-native.html); it also supports native UI integration. |
| Test the rules you share | Pure updates describe state changes. Typed effects request files, networking and storage; test handlers supply controlled responses through each application's own boundary. |
| Less code to maintain | Inferred types, exhaustive matching and shared transitions keep rules in Osprey instead of repeating them per host. |

The framework's hosts all draw the same frame of rows in shared memory; [see how the bridge works →](/docs/osprey-native/bridge/). Host coverage varies: the mobile Issue Inbox and bank examples still use their own boundaries, and common host conformance remains delivery work. Osprey is actively evolving; validate it against your workload.

## Build your first app

[Install →](/docs/installation/) · [Build a counter →](/docs/osprey-native/first-app/) · [First CLI app →](/docs/my-first-app/) · [Watch Mac and iPhone interop →](/docs/tutorials/swift-interop/)

[Framework source](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/native) · [Host contract](/spec/0047-ospreynativehosts/) · [Capture sources](https://github.com/Nimblesite/osprey/blob/main/src/toolchain/website/src/assets/images/native/README.md)
