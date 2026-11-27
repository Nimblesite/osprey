---
layout: layouts/post.njk
tags: [blog, signals, bridge, performance]
title: "Signals from the Ground Up and One Shared-Memory Frame per Update"
description: "How Osprey Native combines compiler-derived Signal<T> keyed regions with a zero-serialization shared-memory bridge generated from native.td."
excerpt: "No dependency arrays, no virtual DOM diffing, and no JSON bridge: compiler-derived signals refresh only changed regions and publish fixed-width 152-byte rows in shared memory."
date: 2026-11-12
author: "Christian Findlay"
category: "Signals & Runtime"
badgeClass: "badge-canvas"
readingTime: 5
image: /assets/images/prism-spectral.webp
imageAlt: "Perception DAW spectral processor patch rendered on FerroUI GPU canvas"
imageWidth: 2304
imageHeight: 1512
imageCaption: "Perception on FerroUI: compiler-derived signals update only changed regions while streaming fixed-width rows across the shared-memory bridge."
---

Most UI frameworks bolt reactivity onto a general-purpose runtime with hook rules, dependency arrays, or virtual DOM reconciliation. Osprey Native builds reactivity into the compiler and pairs it with a zero-serialization binary boundary.

## Compiler-derived signals (`Signal<T>`)

Each application state slice is a typed record read through `Signal<T>`. Keyed regions (`Keyed::region`) snapshot their compiler-derived read dependencies automatically. When `update` returns a new store, only regions that read a changed slice rebuild; unchanged regions retain their revision, focus, scroll anchor, and IME composition state.

## One frame, four sealed runs

Instead of formatting JSON strings across a bridge, `Views::shared` appends fixed-width, 8-byte-aligned rows generated from `native.td`:

1. **Scene rows (`SceneShape`, 88 bytes):** GPU primitives, sprites, lettering, and accessible elements.
2. **View rows (`ViewRow`, 152 bytes):** native controls, typography, layout, and binding ordinals.
3. **Text run:** contiguous UTF-8 bytes referenced by offset and length.
4. **Ask rows (`AskRow`, 32 bytes):** typed capability requests and host-held slice writes.

Every host (SwiftUI, WinUI, FerroUI, GPUI, Android JNI, and WebAssembly) verifies the ABI fingerprint at startup, leases the frame in place, validates every row, and refuses malformed frames whole while keeping the last good frame on screen.
