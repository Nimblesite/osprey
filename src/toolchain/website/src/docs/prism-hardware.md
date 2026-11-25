---
layout: page
title: "Prism hardware"
description: "The planned Prism pedal: a standalone synth or effects pedal that runs patches made in Prism, with knobs and footswitches mapped to the patch's macros."
permalink: /docs/audio-synthesis/hardware/
---

**Prism hardware is a design, not a product.** No firmware, board or pedal exists yet, so there is nothing to buy or build today. This page explains what is planned.

## A patch you can stomp on

The Prism pedal is a small metal box that plays a patch made in [Prism](/docs/audio-synthesis/), without a computer. It runs the same Osprey synthesis engine as Prism, so a patch sounds the same on the pedal as on the Mac. It has no screen. It works in one of two roles:

- **Synth:** play it from a MIDI keyboard and it makes the sound.
- **Effects pedal:** plug a guitar or line signal in, and the patch processes it.

The knobs and switches don't have fixed jobs. Each one controls a **macro**, a single control you add to a patch in Prism that moves one or more of its settings at once, such as a "Brightness" macro that opens a filter and adds drive. You choose which macro each knob, toggle and footswitch drives.

## From Prism to the pedal

The planned workflow:

1. Build the patch in Prism's patch editor, wiring oscillators, filters and effects in any valid routing.
2. Add macros and assign them to the pedal's eight knobs, four toggles and two footswitches. Prism draws the pedal's panel so you can see where each one lands.
3. Connect the pedal to the computer with one USB-C cable and send the patch. Prism first checks that the patch fits the pedal's processor budget and shows any part that doesn't.
4. Unplug and play. Plug it back in and Prism shows which patch the pedal is running.

<figure>
  <img src="/assets/images/prism/device-front.webp" width="1536" height="1024" alt="Top of the charcoal Prism pedal concept: two rows of four knobs, four toggles with a status light between them, and two footswitches nearest the player" decoding="async">
  <img src="/assets/images/prism/device-concept.webp" width="1536" height="1024" alt="Rear of the Prism pedal concept with one USB-C port, a DC power jack and two 3.5 mm TRS MIDI jacks" loading="lazy" decoding="async">
  <figcaption>AI-generated concept renders of the top and rear. They show the eventual metal case, not a built pedal.</figcaption>
</figure>

## Connections

- **One USB-C port** does everything: connect a computer to send patches and update firmware, or plug in a USB MIDI keyboard and the pedal powers it.
- **TRS MIDI in and out** (3.5 mm, Type A): a keyboard with a 5-pin DIN socket connects through a standard Type A adapter.
- **Audio:** stereo 6.35 mm (1/4") inputs with a guitar-level setting, stereo outputs and a headphone jack.
- **Power:** a standard 9–12 V centre-negative pedal supply. When the pedal is bypassed, crashes or loses power, a relay connects each input straight to its output, so your signal is never cut.

## Fast enough to play

A sound that lags behind your hand feels wrong. Researchers put the limit at 10 ms from gesture to sound, with no more than 1 ms of variation (Wessel and Wright, NIME 2001). The pedal is designed for about half of that: under 5 ms from audio in to audio out, and under 5.5 ms from a key press to its sound.

The pedal runs on a Raspberry Pi Compute Module 5 with a real-time Linux kernel. Its processor is fast enough for a stereo patch with eight voices, a delay and a reverb. Osprey's engine is not yet fast enough: making it fast enough for the pedal is ongoing work, done alongside everything else.

## Built for the stage

The first devices will sit in a simple 3D-printed box. The rugged case comes later. That eventual case is die-cast aluminium with a sealed lid. The footswitches are bolted through the metal and must hold a 100 kg standing load. Every jack and the USB-C port is fixed to the case, not only to the circuit board. Before release, a finished pedal must survive 1 m drops onto concrete, shock and vibration tests, and still play.

<figure>
  <a href="/assets/images/prism/enclosure.svg"><img src="/assets/images/prism/enclosure.svg" width="1300" height="980" alt="Dimensioned control and port layout with a section through the module mounting bracket" loading="lazy" decoding="async"></a>
  <figcaption>The planned layout of controls and connectors on the case.</figcaption>
</figure>

## Cost

The parts for one pedal are estimated at about AUD 475, under a target of AUD 500, using November 2026 prices. Raspberry Pi prices have risen twice in 2026 as memory costs went up, so this figure will change.

The engine's maths, including the compressor's gain curve and the waveshaper's antialiasing, is checked by proofs against its production code. See [FX on Effects](/docs/audio-synthesis/#fx-on-effects).

[Read the hardware specification](/spec/prismdevicehardware/) for the circuit, latency budget, parts list and sources.
