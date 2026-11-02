---
layout: page
video: static-typing-python
permalink: /docs/tutorials/static-typing-python/
eleventyComputed:
  title: "{{ (video | video).name }}"
  description: "Watch Osprey's Python flavor infer every type and stop a mistyped call at compile time, on a sample and on a 1,262-line Chromecast sender ported from CPython."
tags: [tutorials, video, python-flavor, static-typing, types]
---

Static typing checks every value's type before a program runs. This video shows Osprey's Python flavor doing that with no type hints, first on a five-line sample, then on a real port of a CPython program.

{% videoPlayer video %}

The full write-up, with the code and its limits, is [Static typing for Python-style code, without writing type hints](/blog/2026-11-01-static-typing-python-without-type-hints/).

Osprey is actively evolving.

## Transcript

{% videoTranscript video %}
