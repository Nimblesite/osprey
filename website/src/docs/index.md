---
layout: page
title: Osprey documentation
description: Start with algebraic effects and reusable handlers, then build native, web and mobile applications with Osprey.
---

Osprey uses typed algebraic effects to separate application requests from their implementations. Start with a handler, then see the same approach in the banking and mobile applications.

## Start here

- **[Algebraic effects](/docs/effects/)** — declare an operation, call a reusable handler, replace it in a test, and choose an arithmetic policy.
- **[One model, two backends](/docs/one-model-two-backends/)** — write GPT-2 once against a static `Tensor` effect, then run it over host lists or on the GPU surface by choosing a handler.
- **[Write Osprey in the Python flavor](/docs/python-flavor/)** — use Python-style syntax and decorated effects, compiled by Osprey. Includes the GPT-2 port and explains the boundary with CPython.
- **[Python flavor for AI agents](/docs/python-flavor-for-ai/)** — commands, effect decorators, supported syntax and bug-reporting instructions for coding agents.
- **[Install Osprey](/docs/installation/)** — set up the compiler and LLVM/clang toolchain on macOS, Linux, or Windows.
- **[Build My First App](/docs/my-first-app/)** — create a native CLI that models JSON as a recursive algebraic data type, reads a JSON file, and safely writes the next state.
- **[Try the Playground](/playground/)** — run small programs in the browser before setting up a local toolchain.

## Tutorials

- **[Video tutorials](/docs/tutorials/)** — narrated videos of real Osprey apps being built, with chapters, the commands to run each app, and a transcript.
- **[Osprey and Swift interop on the Mac and iPhone](/docs/tutorials/swift-interop/)** — a three-minute video: Osprey exports in a generated C header, Swift `@_cdecl` imports, an effect handler at the boundary, and one Xcode target for every Apple SDK.

## Proofs and mathematics

- **[Proof video learning paths](/docs/proofs/)** - twelve short films, organised into six topics and two audience paths.
- **[Use the Proof Workbench](/docs/proof-workbench/)** - write an argument, inspect hypotheses and goals, and work through guided lessons.
- **[Mathlib port progress](/docs/mathlib-port/)** - real checked imports, reproducible commands and the remaining boundaries.

## Build applications

- **[Build a Mac and iPhone app](/docs/mac-and-iphone-apps/)** — follow Osprey Storage across the boundary: Osprey exports in a generated C header, Swift `@_cdecl` imports, and an effect handler at the edge. Also a [video tutorial](/docs/tutorials/swift-interop/).
- **[Build an iOS and Android app](/docs/mobile-apps/)** — handle application requests as native commands for SQLite, GitHub and reactive controls.
- **[Build a web app](/docs/web-apps/)** — follow Talon Bank from storage and audit handlers to shared web, Android and iOS clients.
- **[Explore the WebAssembly studio](/wasm/)** — compare the Default and ML flavors in a browser-hosted example.
- **[Browse working examples](https://github.com/Nimblesite/osprey/tree/main/examples)** — study native, HTTP, WebSocket, terminal, graphics, and WebAssembly programs.
- **[Document your modules](/docs/documentation/)** — generate HTML API references, add Markdown guides and custom CSS, and test documentation examples.

## Test and investigate

- **[Hunt for bugs](/docs/hunt/)** — explore symbolic inputs with KLEE, inspect source-level findings and replay them on a native build.

## Packages

- **[Package manager](/docs/package-manager/)** — folder registry, verified downloads, epoch-based resolution and offline locked builds. Includes a quick start and the planned package website workflow. The client and server are written in Osprey.
- **[Packages in VS Code](/docs/packages-in-vscode/)** — add and remove direct dependencies, review unverified-release warnings, then fetch and build locked source. Includes real editor screenshots.
- **[Package specification](/spec/0029-packagemanagement/)** — the normative contract: folder registry, manifests, identity, resolution, locks, the REST API and the command surface.

## Language

- **[Read the language specification](/spec/)** — the intended language contract; check feature status for current implementation limits.
- **[Use dense computation buffers](/spec/0034-gpucomputation/)** — typed buffers and pure kernels, run on Metal or OpenCL when `OSPREY_GPU` selects a device and on the CPU otherwise.
- **[Check feature status](/status/)** — see what works today, its current limits, and what is planned.
- **[Browse keywords](/docs/keywords/)** — declarations, bindings, matching, imports, and literals.

A flavor is a surface syntax: its concrete syntax tree lowers into one canonical AST, and everything after that is shared. Default files use `.osp`, braces, `fn`, and parenthesized calls. ML files use `.ospml`, indentation, currying by default, and whitespace application. Python-flavor files use `.py` with the `# osprey: flavor=python` marker, Python-style indentation and annotations, and decorators for effects; they are Osprey, not Python. See the [Python flavor guide](/docs/python-flavor/).

## Reference

- **[Functions](/docs/functions/)** — file I/O, strings, collections, processes, networking, JSON document queries, and concurrency.
- **[Types](/docs/types/)** — built-in value and runtime types.
- **[Operators](/docs/operators/)** — arithmetic, comparison, and pipeline operators.
- **[Keywords](/docs/keywords/)** — language syntax by keyword.
