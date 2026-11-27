---
layout: layouts/docs.njk
title: "Accessibility & Automated Testing"
lead: "Enforced framework-level accessibility trees designed for screen readers, headless @osprey/uitest journeys, and AI agents."
section: "Quality & AI"
prevUrl: /docs/bridge/
prevTitle: "Shared-Memory Bridge"
permalink: /docs/testing-and-accessibility/
---

Instead of bolting accessibility onto finished screens, Osprey Native's UI machines declare a typed `AccessRole` (`Pressable`, `Slidable`, `Popup`, `MenuItem`, `Unannounced`) and a live template label for every control (`accessibility.td`).

## One tree for screen readers, tests, and AI agents

- **`Canvases::semantics`** emits the live accessibility tree on every frame: role, resolved label, bounds, and slider value. Hidden or disabled controls are omitted so neither a user nor an AI agent can trigger an illegal action.
- **AccessKit integration:** The GPUI host maps each entry to AccessKit (NSAccessibility on macOS, UI Automation on Windows, AT-SPI on Linux).
- **Headless `@osprey/uitest`:** Drives the app's real `start` and `dispatch` exports without opening a window, browser, or simulator.

```ospml
expect (labels (), "Receiver: Living Room TV|Play|Open file…")
tap "Open file…"
picked "/music/Sunroom.mp3"
expect (letters "Sunroom", true)
tap "Play"
tapAt ("Volume", 0.5)
expect (valueOf "Volume", 0.5)
```

`tap` dispatches a real pointer press and release at the control's bounds (catching overlapped hit targets), and `letters` asserts what is actually drawn on screen.

## Current scope

The GPUI host maps the UI-machine accessibility tree to AccessKit today. Mapping the same tree on WinUI, SwiftUI, and VS Code hosts, emitting it from direct view code, and shipping an external MCP server over `semantics`/`act` remain active delivery work.
