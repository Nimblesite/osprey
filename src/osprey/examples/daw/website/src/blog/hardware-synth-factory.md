---
layout: post.njk
title: "From Patch to Pedal: Perception as a Hardware Synth Factory"
description: "Bind your modular patch to a typed hardware profile and run the exact same DSP engine on a dedicated Raspberry Pi 5 pedal or desktop synth."
date: "2026-11-11"
readTime: "4 min read"
tag: "Hardware"
image: "/assets/blog-hardware.svg"
alt: "Matte dark-indigo desktop synthesizer hardware box with glowing orange rotary encoders and cyan OLED waveform display"
---

Why rebuild a synth from scratch in C++ every time you want a physical instrument on your desk or pedalboard?

**Perception is the factory.** A hardware box — whether a 4-knob granular pedal or a 16-knob desktop poly synth — is just a typed **device profile** declaring its physical encoders, switches, audio I/O, MIDI ports, and CPU budget.

## One engine, three surfaces

1. **Patch in Perception** (standalone or as a VST3 in your DAW): wire waveguides, 16-voice grain clouds, spectral pitch shifters, and formula melodies.
2. **Map to a device face:** pick a hardware profile and bind macros to real physical knobs with proved monotone curves.
3. **Run untethered on Linux hardware:** the reference Raspberry Pi 5 + Perception HAT runs the exact same engine on an isolated CPU core at `SCHED_FIFO` priority, direct ALSA 64-frame periods (&lt; 10 ms bound), locked memory, and a hardware watchdog with relay bypass.

Design an instrument once. Play it in your studio, load it as a VST3, or gig it in an anodized enclosure with no laptop in sight.
