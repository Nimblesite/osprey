---
layout: page.njk
title: "An Accessibility Tree AI Agents Can Drive: Automated UI Testing by Label"
excerpt: "Computer-use agents squint at screenshots; browser agents read the page's accessibility tree. Osprey's UI machines give every native control a role and a label, so tests and agents find the Play button by name and press it with real pointer events, with no window, browser or device."
description: "What an accessibility tree is, why AI agents and automated UI tests work better from one than from screenshots, and how Osprey's UI machines and @osprey/uitest drive a native app by label with no window, shown on a Chromecast player."
date: 2026-11-01
tags: ["blog", "ui-testing", "accessibility", "ai-agents", "osprey-native", "testing"]
author: "Christian Findlay"
readingTime: 7
image: /assets/images/blog/accessibility-tree-ui-testing.png
imageWebp: /assets/images/blog/accessibility-tree-ui-testing.webp
imageAlt: "A luminous glass tree with colorful control-shaped leaves and a golden selection arc."
---

There are two ways for software to use an interface it did not write. It can look at pixels: take a screenshot, guess where the Play button is, and click there. That is how computer-use agents operate desktop apps, and how screenshot-based UI tests work. Or it can read the **accessibility tree**: the list of controls a screen reader announces, each with a **role** (button, slider, menu item), a **label** ("Play") and a position. Browser agents built on Playwright work this way, and so do the most reliable web UI tests, because "the button labelled Play" survives a redesign that moves it.

Native desktop apps rarely make that second way easy. [Osprey](/), a functional language built for safety, elegance and performance, now builds it into its UI framework. An interface written as a [UI machine](/blog/2026-11-01-rust-ui-framework-without-rust/) produces its own accessibility tree, and anything (a test, an agent, a screen reader bridge) can find a control by label and operate it.

{% videoPlayer "ui-testing-by-label", true %}

## What the tree looks like

Every control in a UI machine declares a role and a label. Labels are templates, so they say what the control currently means. Here are the [casting player's](/blog/2026-11-01-chromecast-from-mac-to-tv/) receiver menu rows, one per TV found:

{% raw %}
```json
{
  "x": 263, "y": 790, "w": 340, "h": 56,
  "each": { "in": "receivers", "dy": 56 },
  "role": "menuitem",
  "label": "{{item.label}}",
  "on": { "set": "receiver", "value": "{{item.key}}", "fire": "chooseReceiver" },
  "when": { "state": "menu", "in": ["receivers"] }
}
```
{% endraw %}

`Templates::semantics` turns the live controls into the tree: role, filled-in label, bounds and, for a slider, its current value. While a track plays, the player's tree reads `Receiver: Living Room TV`, `Speed: 1×`, `Pause`, `Stop`, `Open file…`, `Position` and `Volume`, and the volume slider reports its level. A control that is not live, such as the menu rows while the menu is closed, is not in the tree, so nothing can press it.

`Templates::act (machine, "Play", 0.5)` operates a control by label, the way a screen reader or an agent would. A slider is set that fraction of the way across; anything else is pressed. An unknown label changes nothing.

## UI tests that read like the user's journey

[`@osprey/uitest`](https://github.com/Nimblesite/osprey/tree/main/packages/uitest) is a small kit on top of the tree. It has three modules: `Views` reads what the view and canvas draw, `Semantics` finds controls and works out pointer positions from their bounds, and `Events` builds the same event JSON a real host sends. This is part of the casting player's canvas test:

```osprey-ml
expect (labels (), "Receiver: Living Room TV|Play|Open file…")
tap "Play"
expect (heardTypes (), "")
tap "Open file…"
picked "/music/Sunroom.mp3"
expect (letters "Sunroom", true)
tap "Play"
expect (heardTypes (), "CONNECT LAUNCH ")
tapAt ("Volume", 0.5)
expect (Doc::decimal (element "Volume", "value"), 0.5)
tapAt ("Position", 0.25)
expect (letters "00:50", true)
```

Read it as a user would. Before a file is chosen, pressing Play sends nothing to the TV. After choosing one, Play connects and launches. Tapping halfway along the volume slider sets the volume to a half, and tapping a quarter along the position bar of a 200-second track shows `00:50` on the canvas.

Two details make this stricter than it looks:

- **`tap` is a real pointer press and release** at the control's bounds, not a call to its handler. If a hit area is misplaced or covered by another, the test fails, as a user's click would.
- **`letters` reads the lettering actually drawn** on the canvas. The assertion is about what is on screen, not about an internal variable.

## No window, no browser, no flakiness

The tests call the app's real exported `start` and `dispatch` functions, the same two the GPUI desktop host calls each frame. No window opens. The outside world sits behind an [algebraic effect](/docs/algebraic-effects/): the Chromecast package does all its I/O through `Wire`, and the test installs a handler that plays the TV, reading every frame the player sends and answering like a real receiver. Nothing waits on a network, a render or a timer, so every run is the same:

```text
ok 1 - a whole cast: find TVs, open a file, play, pause, seek, change volume and speed, stop and replay
ok 2 - skins change the palette, never the controls
ok 3 - no TVs found offers another search, and an unreachable TV shows why
ok 4 - the canvas: every control is reached by its accessible label, through pointer events alone
# tests=4 passed=4 failed=0 skipped=0
```

The second journey switches skins and checks that the controls are still offered by label. A screenshot comparison cannot express that: the pixels change, the meaning must not.

## Why this suits AI agents

An agent operating this app does not need vision or coordinates. It reads a short list of what can be done right now (`Pause`, `Stop`, `Volume` at 0.5), picks one by name, and acts. The labels the agent uses are the labels the tests already prove work, so an agent's view of the app is tested on every build. Disabled or hidden controls are absent from the tree, so the agent cannot try them.

## What is not done yet

- **Platform accessibility.** The tree is specified to map to each platform's accessibility API (NSAccessibility on macOS, UI Automation on Windows, AT-SPI on Linux) through AccessKit. That host mapping is not implemented yet, so VoiceOver does not read the player today. Tests and agents use the tree in process.
- **No agent server.** There is no ready-made MCP server that exposes a running app's tree to an external agent. `semantics` and `act` are the pieces such a server would call.
- **UI machines only.** The tree comes from UI machine documents. Interfaces written directly as Osprey view code do not produce it.

## Try it

From a checkout of [the repository](https://github.com/Nimblesite/osprey), after `make build`:

```bash
./target/release/osprey examples/casting/test --run
```

Then read the test, [`examples/casting/test/main.ospml`](https://github.com/Nimblesite/osprey/blob/main/examples/casting/test/main.ospml), and the [UI machines spec](https://github.com/Nimblesite/osprey/blob/main/docs/specs/platforms/UiMachines.md#accessibility-ui-machine-a11y).

Osprey is actively evolving. Try it on your projects, validate it against your workload, and assess the risks before relying on it in production.
