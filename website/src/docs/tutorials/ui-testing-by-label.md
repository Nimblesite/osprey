---
layout: page
video: ui-testing-by-label
permalink: /docs/tutorials/ui-testing-by-label/
eleventyComputed:
  title: "{{ (video | video).name }}"
  description: "Watch Osprey UI machines expose an accessibility tree that tests and AI agents use to find native controls by label and press them, with no window or browser."
tags: [tutorials, video, ui-testing, accessibility, ai-agents]
---

Every control in an Osprey UI machine has a role and a label. This video shows the accessibility tree that produces, and end-to-end UI tests that tap controls by label with real pointer events.

{% videoPlayer video %}

The full write-up, with the code and its limits, is [An accessibility tree AI agents can drive](/blog/2026-11-01-accessibility-tree-ai-agents-ui-testing/).

Osprey is actively evolving.

## Transcript

{% videoTranscript video %}
