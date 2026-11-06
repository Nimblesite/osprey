---
layout: page
video: rust-ui-framework-without-rust
permalink: /docs/tutorials/rust-ui-framework-without-rust/
eleventyComputed:
  title: "{{ (video | video).name }}"
  description: "Watch a GPUI desktop app described as state machines, a view and skins in one JSON document, with a small Osprey adapter and no layout code."
tags: [tutorials, video, gpui, rust, ui-framework]
---

Learn to write a GPUI app in Osprey. GPUI is the GPU-rendered Rust UI framework behind the Zed editor. This tutorial walks through [GPUI's own painting example ported to Osprey](https://github.com/Nimblesite/osprey/tree/main/src/osprey/examples/gpui/painting), where Rust is only the host, using [`@osprey/native`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/native).

{% videoPlayer video %}

## What you will learn

- The two-function boundary the Rust host calls (`main.ospml`).
- The store as typed data, and `update` returning a new store (`app.ospml`, `sketch.ospml`).
- Drawing rules as pure functions, including Shift-straightened lines.
- The view: shapes as data, the header, and regions that redraw only what changed (`view.ospml`).
- The tests and the headless self-test with no window, and a dash-pattern edit that one test catches.
- A second way to write a GPUI app: one UI machine document.

## Try it

```sh
osprey test src/osprey/examples/gpui/painting/test     # no window needed
make -C src/toolchain/targets/macos painting           # open the app
```

More on UI machine documents: [A Rust UI framework without writing Rust](/blog/2026-11-01-rust-ui-framework-without-rust/). Osprey is actively evolving.

## Transcript

{% videoTranscript video %}
