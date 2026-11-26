---
layout: layouts/post.njk
tags: [blog, accessibility, testing, ai-agents]
title: "Accessibilitymaxxing: Enforced Accessibility Trees Built for Screen Readers and AI Agents"
description: "Why Osprey Native enforces typed AccessRole and live labels at the framework level so screen readers, @osprey/uitest, and AI agents drive native apps by label."
excerpt: "Computer-use agents squint at screenshots and guess coordinates. Osprey Native emits a live, typed accessibility tree from UI machines so screen readers, @osprey/uitest, and AI agents operate controls by name."
date: 2026-11-20
author: "Christian Findlay"
category: "Accessibility & Testing"
badgeClass: "badge-verified"
readingTime: 6
image: /assets/images/chromecast-player.webp
imageAlt: "Sherbet Signal casting player rendered on GPUI with accessible play, volume, and receiver controls"
imageWidth: 868
imageHeight: 1114
imageCaption: "Sherbet Signal on GPUI: every control on the GPU canvas exposes a typed role, bounds, and live label to AccessKit and @osprey/uitest."
---

Accessibility is usually treated as a late audit: sprinkle ARIA attributes or semantics modifiers onto finished screens and hope nothing breaks on the next refactor. That fails screen reader users — and it also forces AI agents into brittle screenshot-clicking loops.

In Osprey Native, **accessibilitymaxxing** is built into the framework's data model (`accessibility.td`):

- Every control in a UI machine declares a typed `AccessRole` (`Pressable`, `Slidable`, `Popup`, `MenuItem`, `Unannounced`) and a template label.
- `Canvases::semantics` produces the live accessibility tree on every frame. Hidden or disabled controls are omitted, so neither a person nor an AI agent can invoke an illegal action.
- `@osprey/uitest` drives the app's real `start` and `dispatch` exports headlessly with zero window or simulator: `tap "Play"` fires a real pointer press and release at the control's bounds, and `letters "00:50"` verifies the actual text drawn on screen.
- On the GPUI host, every entry maps to AccessKit (NSAccessibility on macOS, UI Automation on Windows, AT-SPI on Linux).

Mapping the same tree across WinUI, SwiftUI, and VS Code hosts, emitting it from direct view code, and exposing an external MCP server over `semantics` and `act` remain active delivery work.
