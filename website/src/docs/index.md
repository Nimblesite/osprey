---
layout: page
title: Osprey documentation
description: Start building with Osprey, explore application guides, understand the language, and find precise API reference pages.
---

Osprey is a practical functional language for safe, fast native programs. Start with one small application, then move into the language and API references when you need exact details.

## Start here

- **[Install Osprey](/docs/installation/)** — set up the compiler and LLVM/clang toolchain on macOS, Linux, or Windows.
- **[Build My First App](/docs/my-first-app/)** — create a native CLI that models JSON as a recursive algebraic data type, reads a JSON file, and safely writes the next state.
- **[Try the Playground](/playground/)** — run small programs in the browser before setting up a local toolchain.

## Build applications

- **[Build a web app](/docs/web-apps/)** — use an Osprey WebAssembly model/update core with a React renderer.
- **[Explore the WebAssembly studio](/wasm/)** — compare the currently available Default and ML flavors in a browser-hosted example.
- **[Browse working examples](https://github.com/Nimblesite/osprey/tree/main/examples)** — study native, HTTP, WebSocket, terminal, graphics, and WebAssembly programs.

## Language

- **[Read the language specification](/spec/)** — the precise syntax and behavior of implemented features.
- **[Check feature status](/status/)** — see what is stable, partial, experimental, or planned.
- **[Browse keywords](/docs/keywords/)** — declarations, bindings, matching, imports, and literals.

Osprey currently has two source flavors. Default files use `.osp`, braces, `fn`, and parenthesized calls. ML files use `.ospml`, indentation, currying by default, and whitespace application. Both lower to the same checked program representation.

## Reference

The reference is divided by what you are looking for. The function catalogue is deliberately kept on its own page instead of overwhelming this guide.

- **[Functions](/docs/functions/)** — file I/O, strings, collections, processes, networking, JSON document queries, and concurrency.
- **[Types](/docs/types/)** — built-in value and runtime types.
- **[Operators](/docs/operators/)** — arithmetic, comparison, and pipeline operators.
- **[Keywords](/docs/keywords/)** — language syntax by keyword.
