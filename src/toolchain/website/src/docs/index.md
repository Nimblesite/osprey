---
layout: page
title: Osprey documentation
description: Start with algebraic effects and reusable handlers, then build native, web and mobile applications with Osprey.
---

{% from "native-capture.njk" import capture %}

Osprey is built for **safety, elegance and performance**: typed effects and `Result` for handling failures, inferred types for concise code, and LLVM native compilation. Start with a storage handler, then use the same pattern in web and mobile apps.

{% videoPlayer "profiler", true %}

## Start here

- **[Algebraic effects](/docs/effects/)** — declare an operation, call a reusable handler, replace it in a test, and choose an arithmetic policy.
- **[Run the same model on CPU or GPU](/docs/cpu-or-gpu/)**: GPT-2 runs on the CPU or the GPU by changing one handler.
- **[Write Osprey in the Python flavor](/docs/python/flavor/)** — compile Python-style Osprey code, declare effects and call CPython through interop.
- **[Call Python from Osprey, and Osprey from Python](/docs/python/interop/)** — the `Python` effect, `import osprey`, and test doubles for Python calls.
- **[Python flavor for AI agents](/docs/python/for-ai/)** — commands, effect decorators, supported syntax and bug-reporting instructions for coding agents.
- **[Install Osprey](/docs/installation/)** — set up the compiler and LLVM/clang toolchain on macOS, Linux, or Windows.
- **[Build My First App](/docs/my-first-app/)** — build a native CLI that reads, validates and updates a JSON file.
- **[Try the Playground](/playground/)** — run small programs in the browser before setting up a local toolchain.

## Tutorials

- **[Video tutorials](/docs/tutorials/)** — watch real apps, then run them with the accompanying commands.
- **[Osprey and Swift interop on the Mac and iPhone](/docs/tutorials/swift-interop/)** — watch Storage call Swift through a generated C interface.

## Proofs and mathematics

Lean provides the [development oracle](/docs/proofs/#lean-as-an-oracle) for compatible proof checking while Osprey builds its independent proof technology and replaceable disciplines.

- **[Proof video learning paths](/docs/proofs/)** - twelve short films, organised into six topics and two audience paths.
- **[Use the Proof Workbench](/docs/proof-workbench/)** - write an argument, inspect hypotheses and goals, and work through guided lessons.
- **[Mathlib port progress](/docs/mathlib-port/)** - real checked imports, reproducible commands and the remaining boundaries.

## Build applications

- **[Osprey Native](/docs/osprey-native/)** — the application framework: one store, signals, typed screens and capabilities; SwiftUI, VS Code and GPU hosts draw them.
- **[Build a Mac and iPhone app](/docs/mac-and-iphone-apps/)** — build Storage and connect its logic to SwiftUI and platform services.
- **[Build an iOS and Android app](/docs/mobile-apps/)** — handle application requests as native commands for SQLite, GitHub and reactive controls.
- **[Build a web app](/docs/web-apps/)** — follow Talon Bank from storage and audit handlers to shared web, Android and iOS clients.
- **[Explore the WebAssembly studio](/wasm/)** — compare the Default and ML flavors in a browser-hosted example.
- **[Browse working examples](https://github.com/Nimblesite/osprey/tree/main/src/osprey/examples/)** — study native, HTTP, WebSocket, terminal, graphics, and WebAssembly programs.
- **[Document your modules](/docs/documentation/)** — generate HTML API references, add Markdown guides and custom CSS, and test documentation examples.

{{ capture("/assets/images/native/macos-synth.png", "Prism Synth · connect a patch and play it in a native macOS window.", "Prism Synth · connect a patch and play it in a native macOS window.", 2304, 1512) }}

## Test and investigate

- **[Hunt for bugs](/docs/hunt/)** — explore symbolic inputs with KLEE, inspect source-level findings and replay them on a native build.

## Packages

- **[Package manager](/docs/package-manager/)** — publish to a folder registry, resolve dependencies and build from verified, locked source. Hosted publishing remains planned.
- **[Packages in VS Code](/docs/packages-in-vscode/)** — add and remove direct dependencies, review unverified-release warnings, then fetch and build locked source. Includes real editor screenshots.
- **[Package specification](/spec/0029-packagemanagement/)** — the normative contract: folder registry, manifests, identity, resolution, locks, the REST API and the command surface.

## Language

- **[Read the language specification](/spec/)** — the intended language contract; check feature status for current implementation limits.
- **[Use dense computation buffers](/spec/0034-gpucomputation/)** — typed buffers and pure kernels, run on Metal or OpenCL when `OSPREY_GPU` selects a device and on the CPU otherwise.
- **[Check feature status](/status/)** — see what works today, its current limits, and what is planned.
- **[Browse keywords](/docs/keywords/)** — declarations, bindings, matching, imports, and literals.

A **syntax flavor** is a way to write Osprey; all three share its compiler, types and effects. Default uses `.osp` and braces; ML uses `.ospml` and indentation; [Python flavor](/docs/python/flavor/) uses `.py` with `# osprey: flavor=python`. Python-flavor files compile as Osprey, not CPython.

## Reference

- **[Functions](/docs/functions/)** — file I/O, strings, collections, processes, networking, JSON document queries, and concurrency.
- **[Types](/docs/types/)** — built-in value and runtime types.
- **[Operators](/docs/operators/)** — arithmetic, comparison, and pipeline operators.
- **[Keywords](/docs/keywords/)** — language syntax by keyword.
