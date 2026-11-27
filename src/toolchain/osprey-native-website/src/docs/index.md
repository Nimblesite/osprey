---
layout: layouts/docs.njk
title: "Osprey Native Documentation"
lead: "Build real native apps for macOS, Windows, iOS, Android, and Web — rendering genuine platform controls or a FerroUI GPU canvas from one ahead-of-time codebase."
section: "Getting Started"
nextUrl: /docs/first-app/
nextTitle: "Build Your First App"
permalink: /docs/
---

**Osprey Native** (`@osprey/native`) compiles your application state, signals, and UI ahead of time to native machine code on desktop and mobile, and WebAssembly on the web. Native builds ship **no JavaScript engine**, no JS bundle, and no desktop JVM.

## Choose your rendering model

- **Native Platform Controls:** SwiftUI on macOS and iOS, WinUI (Windows App SDK, MSIX AppContainer) on Windows, and Android's own views on Android.
- **FerroUI / GPUI GPU Canvas:** Pixel-identical GPU-accelerated scenes for DAWs (Perception), vector tools, and skinnable UI machines.
- **Web & VS Code:** Compiles to `wasm32` and reads the exact same shared-memory row offsets via `DataView`.

## Core architecture

1. **Centralized store & signals (`Signal<T>`):** Each state slice has its own record type. `Keyed::region` tracks compiler-derived signal reads automatically, refreshing only changed regions.
2. **Pure `update` & typed algebraic effects:** Network, files, storage, and host-held slices (`Slices.fetch`) live behind typed effects so production hosts execute real OS calls while tests supply deterministic fixtures.
3. **Zero-serialization shared-memory bridge:** Every update publishes fixed-width rows (`ViewRow` 152 B, `SceneShape` 88 B, UTF-8 text run, `AskRow` 32 B) defined once in `native.td`.
4. **Multi-syntax authoring:** Write in Default Osprey (`.osp`), ML (`.ospml`), Python flavor (`# osprey: flavor=python`), or TypeScript flavor today — with Dart flavor coming soon and Kotlin flavor under consideration.

## Next steps

- [Build Your First App →](/docs/first-app/) — step-by-step guide to state, `update`, `view`, and running on macOS, Windows, iOS, and Android.
- [Shared-Memory Bridge →](/docs/bridge/) — how SwiftUI, WinUI, FerroUI, GPUI, Android, and WebAssembly lease and validate frames in place.
- [Accessibility & Automated Testing →](/docs/testing-and-accessibility/) — how UI machines emit typed `Accessible` trees for AccessKit, `@osprey/uitest`, and AI agents.
- [Osprey Language Docs →](https://www.ospreylang.dev/docs/) — language reference, Hindley-Milner types, algebraic effects, and compiler installation.
