---
layout: page.njk
title: "A Rust UI Framework Without Writing Rust: GPUI Apps as State Machines"
excerpt: "GPUI is the GPU-rendered Rust UI framework behind the Zed editor. Osprey's UI machines put a whole GPUI app's interface in one data file: parallel state machines, a view, and skins drawn from an image atlas. The only code left is a small adapter."
description: "What GPUI is, how it compares with egui, Slint and Tauri, and how Osprey's UI machines describe a native GPUI app as state machines and skins in one JSON document, shown with a Chromecast player."
date: 2026-11-01
tags: ["blog", "gpui", "rust", "ui-framework", "osprey-native", "state-machines"]
author: "Christian Findlay"
readingTime: 7
image: /assets/images/blog/rust-ui-framework-without-rust.png
imageWebp: /assets/images/blog/rust-ui-framework-without-rust.webp
imageAlt: "Floating application panels above a copper state-machine sculpture."
---

**GPUI** is the UI framework the [Zed](https://zed.dev) code editor is built on. It is written in Rust, it renders everything on the GPU, and it is fast enough to hold an editor at the display's refresh rate. Like most Rust UI frameworks, it expects you to write your interface in Rust.

[Osprey](/), a functional language built for safety, elegance and performance, already [runs apps in GPUI windows](/blog/2026-10-27-gpui-desktop-apps-with-osprey-native/). This week it went further: with **UI machines**, a GPUI app's whole interface (its states, its controls, its look) is one data file. This casting player is built that way. It contains no layout code and no Rust of its own.


{% videoPlayer "rust-ui-framework-without-rust", true %}

## Where this sits among Rust UI frameworks

| | You write | Rendering | Interface state lives in |
| --- | --- | --- | --- |
| egui | Rust, redrawn every frame (immediate mode) | GPU | Your Rust structs |
| Slint | The `.slint` markup language, plus Rust, C++ or JavaScript | GPU or software | Properties in the markup |
| Tauri | HTML, CSS and JavaScript in a web view, Rust behind it | The system web view | Your front-end framework |
| GPUI | Rust views and elements | GPU | Rust entities |
| GPUI with an Osprey UI machine | One JSON document, plus a small Osprey adapter | GPU, through GPUI | Explicit state machines in the document |

The last row trades flexibility for something specific. Every state the interface can be in is written down, and every way between them is a named transition. That makes the interface checkable, testable without a window, and skinnable by someone who never touches code.

## One document: inputs, layers, view, skins

[`player.machine.json`](https://github.com/Nimblesite/osprey/blob/main/examples/casting/player.machine.json) has four parts.

**Inputs** are the interface's state besides its current states: `file`, `receiver`, `position`, `volume`, the list of `receivers` found, and triggers such as `play`.

**Layers** are state machines that run in parallel, each with exactly one current state. The player has four: `receivers` (searching, found, none), `playback` (stopped, connecting, playing, paused, ended, failed), `commands` and `menu`. Transitions name their condition:

```json
{ "from": "connecting", "to": "playing",
  "when": { "any": [ { "input": "player", "eq": "PLAYING" },
                     { "input": "player", "eq": "BUFFERING" } ] } }
```

Entering a state can ask for work by name: entering `connecting` requests `cast.start` with the chosen `file` and `receiver`. The document never says how. It holds no code, URLs or scripts.

{% raw %}**The view** is a tree of ordinary controls (`button`, `picker`, `meter`, `text` and so on) whose strings are templates: `{{position|clock}}` shows `03:20`, `{{volume|percent}}` a percentage, `{{title|fit:30}}` a title cut with an ellipsis.{% endraw %}

**Skins** change colours and motion, and may carry a **canvas**: sprites from an image atlas, lettering, rounded shapes and hit areas laid out in the designer's own pixels. The Sherbet Signal skin above is a canvas. A row repeated with `each` gives the receiver menu one entry per TV, and a `menu` layer opens and closes the dropdowns. Switching skins keeps every input and state.

## The only code is an adapter

The application code that remains answers the document's requests and feeds facts back in as inputs:

```osprey-ml
answer (store, ask) = match ask.askName
    "cast.start" => start (store, ask)
    "cast.pause" => live (store, \p => Player::pause p)
    "cast.seekTo" => live (store, \p => Player::seek (p, givenNumber (ask, "seekTo")))
    "cast.volume" => live (store, \p => Player::volume (p, givenNumber (ask, "wantVolume")))
    _ => asked (failed (store, "the machine asked for an unknown request ${ask.askName}"), [])
```

When the TV reports `PLAYING`, the adapter sets the `player` input, and the `playback` layer moves itself. The adapter never decides what the screen shows.

## Safety in the document, too

A document is validated completely when it loads. A transition to a state that does not exist is refused with its JSON path, for example `layers[0].transitions[3].to: no state named "plaing"`, and nothing is partly applied. An unknown member, kind or input is an error, not something to skip. A settle that keeps transitioning for more than 32 rounds is reported as a cycle instead of spinning. Osprey has no exceptions or panics, and the interpreter keeps to that: a refused document shows its problems in place of the player.

## Every control is accessible and testable

Each hit area declares a role and a label, so a machine produces its own accessibility tree, and tests and AI agents [operate it by label](/blog/2026-11-01-accessibility-tree-ai-agents-ui-testing/). The player's four end-to-end journeys run with no window and no TV.

## Try it

On a Mac with Rust and Homebrew's `openssl`:

```bash
git clone https://github.com/Nimblesite/osprey && cd osprey
make build && make _runtime_macos
bash examples/casting/run.sh
```

What to know first. The casting player runs in the GPUI host on macOS today. The document becomes ordinary Osprey Native views, which the WinUI, VS Code and Apple hosts also draw, but only the GPUI host has run this player. The two skins without a canvas, Midnight Mint and Poolside Pop, still need layout work, so the player opens in Sherbet Signal. Mapping the accessibility tree to the platform's screen reader is specified but not yet implemented.

The format is defined in the [UI machines spec](https://github.com/Nimblesite/osprey/blob/main/docs/specs/platforms/UiMachines.md), and [`@osprey/native`](https://github.com/Nimblesite/osprey/tree/main/packages/native) holds the interpreter. Osprey is actively evolving. Try it on your projects, validate it against your workload, and assess the risks before relying on it in production.
