---
layout: layouts/docs.njk
title: "The Shared-Memory Bridge"
lead: "How one compiled Osprey Native library reaches SwiftUI, WinUI, FerroUI, GPUI, Android, and WebAssembly with zero JSON serialization."
section: "Architecture"
prevUrl: /docs/first-app/
prevTitle: "Build Your First App"
nextUrl: /docs/testing-and-accessibility/
nextTitle: "Accessibility & Testing"
permalink: /docs/bridge/
---

An Osprey Native application compiles ahead of time to a static C-ABI archive (macOS, iOS, Android), a DLL (Windows), or a WebAssembly module (`wasm32`). Every app exports three calls: `osprey_app_argument`, `osprey_app_open`, and `osprey_app_event`.

## One frame, four sealed runs

Each call writes one immutable frame into in-process shared memory defined by a single `typeDiagram` schema (`native.td`):

| Run | Row type | Bytes per row | Contents |
| --- | --- | --- | --- |
| Scene rows | `SceneShape` | 88 | GPU canvas primitives, sprites, lettering, and accessible elements |
| View rows | `ViewRow` | 152 | Native controls, layout metrics, typography, and binding ordinals |
| Text | — | — | Contiguous UTF-8 bytes referenced by offset and length |
| Ask rows | `AskRow` | 32 | Typed capability requests, host-held slice writes, and diagnostics |

## Lease safety and host validation

1. Every host checks the `native.td` ABI fingerprint at startup so layout mismatches fail immediately.
2. Hosts read frames through generation-checked leases; memory is freed only when the last lease is released.
3. Before drawing anything, hosts validate every row kind, parent index, finite float, and UTF-8 text boundary. A malformed frame is refused whole and the last good frame stays on screen.
4. Control events return by binding ordinal, never by arbitrary host-constructed actions.

Read the full normative contract in [Spec 0047](https://github.com/Nimblesite/osprey/blob/main/docs/specs/applications/0047-OspreyNativeHosts.md).
