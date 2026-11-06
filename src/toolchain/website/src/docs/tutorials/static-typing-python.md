---
layout: page
video: static-typing-python
permalink: /docs/tutorials/static-typing-python/
eleventyComputed:
  title: "{{ (video | video).name }}"
  description: "Watch Osprey's Python flavor infer every type and stop a mistyped call at compile time, on a sample and on a 1,262-line Chromecast sender ported from CPython."
tags: [tutorials, video, python-flavor, static-typing, types]
---

Learn how Osprey's Python flavor type-checks Python-style code with no type hints. Every type is inferred at compile time, so a wrong call stops the build instead of failing at run time.

{% videoPlayer video %}

## What you will learn

- Inference on a five-line sample, and the error a wrong argument gives.
- Records, lambdas, `Result` and `match` in a real package, [`@osprey/chromecast`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/chromecast).
- Effects instead of exceptions (`@effect`, `perform`).
- How the package is checked against CPython: 255 checks that must match a CPython oracle.
- One edit that mixes two return types, caught at compile time.

## Try it

```sh
osprey label.py --run                                  # the five-line sample
osprey src/osprey/packages/chromecast/tests --run      # the CPython-checked port
```

The longer write-up is [Static typing for Python-style code, without writing type hints](/blog/2026-11-01-static-typing-python-without-type-hints/). Osprey is actively evolving.

## Transcript

{% videoTranscript video %}
