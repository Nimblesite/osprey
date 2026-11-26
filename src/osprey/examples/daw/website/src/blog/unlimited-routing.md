---
layout: post.njk
title: "Unlimited Signal Routing: Tapping Any Outlet Into Any Bus"
description: "Why fixed channel strips choke sound design — and how Perception lets you tap internal module outlets straight into sidechained stereo buses."
date: "2026-11-24"
readTime: "4 min read"
tag: "Routing"
image: "/assets/blog-routing.jpg"
alt: "Dark indigo modular synth patch bay with glowing orange signal cables and lavender modulation arcs"
---

Most DAWs force a hard wall between your synth patch and your mixer. Once audio leaves an instrument, you get one stereo pair and a fixed stack of inserts.

Perception removes that wall. Every track is a typed signal graph, and **any internal module outlet** can tap straight into any stereo group bus while its dry cable keeps running.

## Real example: Rivet Pulse

In *Silt Relay* (138 BPM), the **Rivet pulse** track splits a MIDI-driven voice through a stereo delay, channel swap, and three parallel paths simultaneously:

- **Dry chain** → `Objects` stereo bus for crisp transient glue.
- **6% outlet tap** from the channel-swap node → `Undertow` bus, where the pressure kick keys a 4:1 sidechain compressor.
- **19% Space send** → stereo reverb return that breathes between offbeat hits.

## Graph-checked, never fragile

Every connection is validated for signal rate, channel layout, and feedback cycles before it commits. Nested stereo buses can feed other stereo buses with live cycle detection — and when you drop Perception as a **VST3 plug-in** inside another DAW, that entire internal routing graph travels with your patch.
