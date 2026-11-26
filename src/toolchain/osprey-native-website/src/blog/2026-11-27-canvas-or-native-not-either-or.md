---
layout: layouts/post.njk
tags: [blog, architecture, cmp, ferroui]
title: "Canvas or Native Controls: Why Cross-Platform UI Doesn't Have to Be Either/Or"
description: "How Osprey Native renders genuine SwiftUI, WinUI, and Android controls or a FerroUI GPU canvas from the same AOT codebase — and where it pulls ahead of Compose Multiplatform."
excerpt: "Flutter and Compose Multiplatform lock you into canvas widgets; React Native locks you into a JS engine. Osprey Native runs both real platform controls and a FerroUI GPU canvas over one shared-memory bridge."
date: 2026-11-27
author: "Christian Findlay"
category: "Architecture"
badgeClass: "badge-native"
readingTime: 5
image: /assets/images/windows-storage.png
imageAlt: "Osprey Storage running in WinUI on Windows"
imageWidth: 1164
imageHeight: 741
imageCaption: "Storage on Windows: real WinUI controls driven by the same Osprey Native shared-memory rows."
---

Every cross-platform UI framework forces a bet on day one. Choose Flutter or Compose Multiplatform (CMP), and your shared UI paints its own widget copies onto a Skia or Impeller canvas. Choose React Native, and you get real platform views — paired with a JavaScript engine and a runtime bridge.

Osprey Native (`@osprey/native`) rejects that either/or split. Your application state, signals, and views compile ahead of time to native machine code (LLVM) and publish fixed-width rows into a lease-checked shared-memory frame:

- **Native Controls Mode:** SwiftUI on macOS and iOS, WinUI on Windows (sandboxed MSIX AppContainer), and Android's own views on Android read `ViewRow` entries in place and draw real OS controls.
- **FerroUI / GPUI Canvas Mode:** When you want identical pixels across every platform — a DAW timeline like Prism, a vector painting board, or a skinnable media player — the FerroUI or GPUI host draws `SceneShape` rows straight on the GPU.

## Where Osprey Native pulls ahead of Compose Multiplatform

1. **Real platform controls on Apple and Windows.** CMP's shared UI draws Skia widgets on iOS and hosts desktop inside a JVM/Swing window. Osprey Native gives you genuine SwiftUI and WinUI when you want native fidelity, and FerroUI GPU rendering when you want custom pixels.
2. **No JVM on desktop, no JSON bridge.** Every native target compiles ahead of time to a static library or DLL with zero JVM warm-up and zero serialization.
3. **Write in your team's syntax flavor.** Use Default Osprey, ML, Python, or TypeScript flavor today (with Dart flavor coming soon and Kotlin flavor under consideration), all sharing one type-checked AOT core.
4. **Structural accessibility and headless testing.** UI machines emit a typed `Accessible` tree tested on every build via `@osprey/uitest`, rather than relying on opt-in semantics modifiers.
