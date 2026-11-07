---
layout: page
video: chromecast-from-mac
permalink: /docs/tutorials/chromecast-from-mac/
eleventyComputed:
  title: "{{ (video | video).name }}"
  description: "Watch a native Mac player written in Osprey cast an MP3 to a Chromecast or Google TV: discovery, playback controls, a spectrum visualiser, and end-to-end tests with no TV."
tags: [tutorials, video, chromecast, macos, casting]
---

Learn how Osprey handles input and output with algebraic effects. Every byte the [`@osprey/chromecast`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/chromecast) package sends or receives goes through one effect, `Wire`. The app answers it with a real handler, and the tests answer it with a fake TV. The worked example is the [casting player](https://github.com/Nimblesite/osprey/tree/main/src/osprey/examples/casting), which plays an MP3 from a Mac on a Chromecast or Google TV.

{% videoPlayer video %}

## What you will learn

- Finding devices with `dns-sd` and returning a `Result` instead of crashing (`discovery.py`, `dns.py`).
- The CAST v2 session and frame format (`session.py`, `protocol.py`).
- Serving the file and loading it on the TV (`media.py`, `player.py`).
- The `Wire` effect and its `Native` handler (`wire.py`), and the app's adapter (`link.ospml`).
- The tests' in-memory TV, eight journeys with no network, and one deleted line that two tests catch.

## Try it

```sh
osprey src/osprey/examples/casting/test --run          # no TV needed
make -C src/toolchain/targets/macos casting            # open the player
```

The longer write-up, with the player's limits, is [How to Chromecast from a Mac to a TV](/blog/2026-11-01-chromecast-from-mac-to-tv/). Osprey is actively evolving.

## Transcript

{% videoTranscript video %}
