---
layout: page
title: "Prism hardware"
description: "Design your own synth or effects pedal in Prism and install it on a box you build yourself: open hardware, open synthesis with mathematical proofs, unlimited patching and an easy visual editor."
permalink: /docs/audio-synthesis/hardware/
---
{% from "native-capture.njk" import capture %}

## Build your own synth. Install it on the box.

**Design a synth or effects pedal in Prism, then install it on a small box you build yourself.** You decide what it sounds like and what every knob does. Prism hardware is planned and is not in production.

- **Build your own synth or FX pedal.** Your design is the instrument. Send it to the box and it plays with no computer attached.
- **Open hardware: bring your own parts.** The design is open. You source standard parts yourself, starting with a Raspberry Pi and an audio board.
- **Open synthesis, checked by mathematical proofs.** The engine on the box is the open Osprey synthesis engine, and its maths is checked against the code that runs ([see how](/docs/audio-synthesis/#fx-on-effects)).
- **Unlimited patching and routing.** Wire any module into any other. There is no fixed signal chain.
- **An easy editor for designing synths.** Drag modules in, click two sockets to connect them, and play the result straight away.

{{ capture("/assets/images/native/macos-synth.png", "Prism patch editor on macOS: an oscillator wired through a filter to the master output, an LFO modulating the filter cutoff, a scope, an inspector and a playable keyboard", "Design the synth in Prism: wire modules together, set their controls and play it. This is the design you install on the box.", 2304, 1512) }}

## What the box is

The Prism box is a small metal pedal that plays your design without a computer. It runs the same Osprey synthesis engine as Prism, so it sounds the same on the box as on the Mac. It has no screen. It works in one of two roles:

- **Synth:** play it from a MIDI keyboard and it makes the sound.
- **Effects pedal:** plug a guitar or line signal in, and your design processes it.

The knobs and switches don't have fixed jobs. Each one controls a **macro**: a single control you add to your design that moves one or more of its settings at once, such as a "Brightness" macro that opens a filter and adds drive. You choose which macro each knob, toggle and footswitch drives.

## From your design to the box

The planned workflow:

1. Build the patch in Prism's patch editor, wiring oscillators, filters and effects in any valid routing.
2. Add macros and assign them to the pedal's eight knobs, four toggles and two footswitches. Prism draws the pedal's panel so you can see where each one lands.
3. Connect the box to the computer with one USB-C cable and install your design on it. Prism first checks that the patch fits the pedal's processor budget and shows any part that doesn't.
4. Unplug and play. Plug it back in and Prism shows which patch the pedal is running.

<figure>
  <div class="prose-image-views">
    <a href="/assets/images/prism/starter-front.webp"><img src="/assets/images/prism/starter-front.webp" width="1440" height="1000" alt="Front of the charcoal Prism starter case with eight knobs, four toggles and two footswitches" decoding="async"></a>
    <a href="/assets/images/prism/starter-rear.webp"><img src="/assets/images/prism/starter-rear.webp" width="1440" height="1000" alt="Rear of the same case with USB-C, DC power, MIDI in and MIDI out" loading="lazy" decoding="async"></a>
    <a href="/assets/images/prism/starter-side.webp"><img src="/assets/images/prism/starter-side.webp" width="1440" height="1000" alt="Left side of the same case with stereo line outputs and a headphone jack" loading="lazy" decoding="async"></a>
  </div>
  <figcaption>AI-generated renders from three angles of one 3D model of the planned printed starter case.</figcaption>
</figure>

## Tested before it is built

The idea is tested on a Mac before any hardware is put together. The pedal's engine runs as its own program, Prism sends patches to it exactly as it will to the pedal, and Prism's on-screen picture of the pedal stands in for the knobs and footswitches. Real latency and the audio hardware are measured later, on a prototype.

## Connections

- **One USB-C port** does everything: connect a computer to send patches and update firmware, or plug in a USB MIDI keyboard and the pedal powers it.
- **TRS MIDI in and out** (3.5 mm, Type A): a keyboard with a 5-pin DIN socket connects through a standard Type A adapter.
- **Audio:** stereo 6.35 mm (1/4") inputs with a guitar-level setting, stereo outputs and a headphone jack.
- **Power:** a standard 9–12 V centre-negative pedal supply. When the pedal is bypassed, crashes or loses power, a relay connects each input straight to its output, so your signal is never cut.

## Fast enough to play

A sound that lags behind your hand feels wrong. Researchers put the limit at 10 ms from gesture to sound, with no more than 1 ms of variation (Wessel and Wright, NIME 2001). The pedal is designed for about half of that: under 5 ms from audio in to audio out, and under 5.5 ms from a key press to its sound.

The pedal runs standard Raspberry Pi OS (Linux) on a Raspberry Pi Compute Module 5, the same foundation other Raspberry Pi synths and pedals use. Its processor is fast enough for a stereo patch with eight voices, a delay and a reverb. Osprey's engine is not yet fast enough: making it fast enough for the pedal is ongoing work, done alongside everything else.

## Built for the stage

The first devices will sit in a simple 3D-printed box. The rugged case comes later. That eventual case is die-cast aluminium with a sealed lid. The footswitches are bolted through the metal and must hold a 100 kg standing load. Every jack and the USB-C port is fixed to the case, not only to the circuit board. Before release, a finished pedal must survive 1 m drops onto concrete, shock and vibration tests, and still play.

## Cost

The parts for one pedal are estimated at about AUD 475, under a target of AUD 500, using November 2026 prices. Raspberry Pi prices have risen twice in 2026 as memory costs went up, so this figure will change.


[Read the hardware specification](/spec/prismdevicehardware/) for the circuit, latency budget, parts list and sources.
