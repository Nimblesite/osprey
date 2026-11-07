---
layout: page
video: ui-testing-by-label
permalink: /docs/tutorials/ui-testing-by-label/
eleventyComputed:
  title: "{{ (video | video).name }}"
  description: "Watch Osprey UI machines expose an accessibility tree that tests and AI agents use to find native controls by label and press them, with no window or browser."
tags: [tutorials, video, ui-testing, accessibility, ai-agents]
---

Learn to test an Osprey app the way a screen reader or an AI agent uses it: by the role and label of each control. This tutorial reads [`@osprey/uitest`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/uitest) and [`@osprey/native`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/native), then the [casting player's tests](https://github.com/Nimblesite/osprey/tree/main/src/osprey/examples/casting/test).

{% videoPlayer video %}

## What you will learn

- Where a control's role and label come from (the machine document).
- How `@osprey/native` turns them into a typed accessibility tree.
- Finding controls by label, reading roles and slider values, and the outline an agent reads (`semantics.ospml`).
- Driving the app with real pointer events, and checks that explain themselves when they fail (`drive.ospml`).
- A whole user journey, the real run, and one renamed label that three tests catch.

## Try it

```sh
osprey src/osprey/examples/casting/test --run          # no window, no TV
```

The longer write-up is [An accessibility tree AI agents can drive](/blog/2026-11-01-accessibility-tree-ai-agents-ui-testing/). Osprey is actively evolving.

## Transcript

{% videoTranscript video %}
