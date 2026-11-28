---
layout: page
title: Osprey documentation
description: "Install Osprey, follow a guide, or watch real apps built with it: Mac, Windows, iPhone and Android apps, web servers and AI models on the GPU."
---

{% from "native-capture.njk" import capture %}

Osprey is uncompromising: safe, elegant and fast, with no trade between them. It compiles to native code through LLVM, failures are `Result` values you must handle, and types are inferred. The film below profiles a running Osprey program in VS Code.

{% videoPlayer "profiler", true %}

## Start here

- **[Install Osprey](/docs/installation/)** — the compiler and LLVM/clang on macOS, Linux or Windows.
- **[Try the Playground](/playground/)** — run programs in the browser, no install.
- **[Build My First App](/docs/my-first-app/)** — a native CLI that reads, validates and updates a JSON file.
- **[Algebraic effects](/docs/effects/)** — declare an operation, handle it, swap the handler in a test.
- **[Run the same model on CPU or GPU](/docs/cpu-or-gpu/)** — GPT-2 runs on the CPU or the GPU by changing one handler.
- **[Write Osprey in the Python flavor](/docs/python/flavor/)** — compile Python-style Osprey code, declare effects and call CPython through interop.
- **[Call Python from Osprey, and Osprey from Python](/docs/python/interop/)** — the `Python` effect, `import osprey`, and test doubles for Python calls.
- **[Python flavor for AI agents](/docs/python/for-ai/)** — commands, effect decorators and supported syntax, written for coding agents.
- **[Write Osprey in the TypeScript flavor](/docs/typescript/flavor/)** — serve the Express README example natively, and see it measured against Express on Node.js.

## Tutorials

- **[Video tutorials](/docs/tutorials/)** — real apps on film, with the commands to run them.
- **[Osprey and Swift interop on the Mac and iPhone](/docs/tutorials/swift-interop/)** — Storage calls Swift through a generated C interface.

## Proofs and mathematics

Osprey checks proofs with its own kernel and uses [Lean as an oracle](/docs/proofs/#lean-as-an-oracle) while it is built.

- **[Proof films](/docs/proofs/)** — twelve short films on six topics, one cut for programmers and one for mathematicians.
- **[Proof Workbench](/docs/proof-workbench/)** — write an argument and watch its hypotheses and goals.
- **[Mathlib port progress](/docs/mathlib-port/)** — which imports check today, and the commands to recheck them.

## Build applications

- **[Audio synthesis](/docs/audio-synthesis/)** — Prism, a DAW written in Osprey: patch voices, mix through side-chained buses, render to WAV.
- **[Osprey Native](/docs/osprey-native/)** — the app framework: one store, signals and typed screens, drawn by SwiftUI, VS Code or GPU hosts.
- **[Build a Mac and iPhone app](/docs/mac-and-iphone-apps/)** — Storage, with its logic in Osprey and its screens in SwiftUI.
- **[Build an iOS and Android app](/docs/mobile-apps/)** — native commands for SQLite, GitHub and reactive controls.
- **[Build a web app](/docs/web-apps/)** — Talon Bank: one Osprey Native app compiled to WebAssembly, the same source on Android and iOS.
- **[WebAssembly studio](/wasm/)** — the Default and ML flavors side by side in the browser.
- **[Examples](https://github.com/Nimblesite/osprey/tree/main/src/osprey/examples/)** — native, HTTP, WebSocket, terminal, graphics and WebAssembly programs.
- **[Document your modules](/docs/documentation/)** — HTML API references, Markdown guides and tested examples.

{{ capture("/assets/images/native/prism-mix-bus.webp", "Prism Mix page on macOS: tracks routed to group buses, a kick-keyed compressor, the Space reverb and Master", "Prism, the DAW written in Osprey, on its Mix page.", 1400, 914) }}

## Test and investigate

- **[Hunt for bugs](/docs/hunt/)** — KLEE explores symbolic inputs; replay each finding on a native build.

## Packages

- **[Package manager](/docs/package-manager/)** — publish to a folder registry, resolve and lock dependencies, build from verified source. Hosted publishing is planned.
- **[Packages in VS Code](/docs/packages-in-vscode/)** — add and remove dependencies, see unverified-release warnings, fetch and build.
- **[Package specification](https://github.com/Nimblesite/osprey-packages/blob/main/docs/specs/ecosystem/0029-PackageManagement.md)** — registry, manifests, resolution, locks, REST API and commands.

## Language

- **[Language specification](/spec/)** — the language as designed; feature status says what is built.
- **[GPU computation](/spec/0034-gpucomputation/)** — typed buffers and pure kernels on Metal, OpenCL or the CPU, picked by `OSPREY_GPU`.
- **[Feature status](/status/)** — what works today, its limits, and what is planned.

Osprey has four **syntax flavors** with one compiler, type system and effect system: Default (`.osp`, braces), ML (`.ospml`, indentation), [Python](/docs/python/flavor/) (`.py` marked `# osprey: flavor=python`) and [TypeScript](/docs/typescript/flavor/) (`.ts` files a project lists). Python and TypeScript files compile as Osprey, not CPython or JavaScript.

## Reference

- **[Functions](/docs/functions/)** — files, strings, collections, processes, networking, JSON and concurrency.
- **[Types](/docs/types/)** — built-in value and runtime types.
- **[Operators](/docs/operators/)** — arithmetic, comparison, and pipeline operators.
- **[Keywords](/docs/keywords/)** — language syntax by keyword.
