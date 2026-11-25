---
layout: page
title: "Prism hardware"
description: "The planned Prism pedal: a standalone synth or effects pedal that runs patches made in Prism, with knobs and footswitches mapped to the patch's macros."
permalink: /docs/audio-synthesis/hardware/
---

**Prism hardware is planned and is not in production.**

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

The engine's maths, including the compressor's gain curve and the waveshaper's antialiasing, is checked by proofs against its production code. See [FX on Effects](/docs/audio-synthesis/#fx-on-effects).

[Read the hardware specification](/spec/prismdevicehardware/) for the circuit, latency budget, parts list and sources.
