---
layout: page
title: "Audio synthesis: write a DAW and synths in Osprey"
description: "Prism is a DAW written in Osprey. Patch synth voices, build FX from raw maths formulas, mix through side-chained buses and render the song to WAV."
permalink: /docs/audio-synthesis/
---

{% from "native-capture.njk" import capture %}

Prism is a DAW written entirely in Osprey, on the [`@osprey/synth`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/synth) library. In the film below, a 17-track song is edited by clicking through the real app: a note's velocity, a reverb send, a Delay patched into a voice, and a bus send. Then the edited bars play.

See the [Prism hardware plan](/docs/audio-synthesis/hardware/) for a standalone synth or effects pedal you can build yourself.

{% videoPlayer "prism-daw-editing", true %}

The clicks are recorded by `make -C src/toolchain/targets/macos/tests daw-video`, which fails if any click leaves the screen unchanged. The soundtrack is the audio Osprey rendered for those two bars.

## What you can patch

- **Synthesis types:** additive partials, FM phase operators, modal resonator banks, physical models (an exciter striking a comb filter, a waveguide string, a loss filter), and granular clouds.
- **Modules:** sine oscillator, low-pass filter, delay, gain, mixer, LFO, MIDI note generator, mono-to-stereo and stereo output. Drag one in, then click an output socket and an input socket to wire them.
- **Typed graph:** every port has a declared rate and channel count. A feedback loop is rejected unless it goes through an explicit delay, so a patch can't make an unintended cycle.

## Mixing and effects

- Each track has gain, pan, an output bus and a send to Space, the shared reverb (size, decay, damping).
- Group buses carry an EQ and a compressor whose detector can be keyed from any track. In the song, the kick ducks the pads.
- Master sums the buses through a soft limiter.

## FX on Effects

The bus compressor computes its soft-knee gain using the Giannoulis, Massberg and Reiss curve (JAES 2012). Its native Osprey implementation lives in [dynamics_math.ospml](https://github.com/Nimblesite/osprey/blob/main/src/osprey/packages/synth/src/dynamics_math.ospml).

```ospml
export curve (level, threshold, ratio, knee) =
    over = level - threshold
    match 2.0 * over < (-1.0) * knee
        true => level
        false => match 2.0 * over <= knee && knee > 0.0
            true => level + floatDiv ((floatDiv (1.0, ratio) - 1.0) * (over + knee / 2.0) * (over + knee / 2.0), 2.0 * knee)
            false => threshold + floatDiv (over, ratio)

(** Gain reduction the curve asks for, in decibels (never positive). *)
export reduction (level, threshold, ratio, knee) = curve (level, threshold, ratio, knee) - level
```

The bus applies the curve and smooths the result for each sample. [Bus mixer](https://github.com/Nimblesite/osprey/blob/main/src/osprey/examples/daw/src/bus_mix.ospml)

```ospml
reduced (c, attack, release, run, key) =
    target = synth::DynamicsMath::reduction (synth::DynamicsMath::decibels key, c.threshold, c.ratio, c.knee)
    level = synth::DynamicsMath::smoothed (run.held.level, target, attack, release)
```

{{ capture("/assets/images/native/prism-mix-bus.webp", "Prism Mix page: glow routed to the pads group bus, whose EQ and compressor take their detector from the kick, beside the Space reverb send and Master with its soft limiter", "Prism Mix on macOS: the pads bus compressor is keyed from the kick, and its gain law is the Osprey curve above.", 1400, 914) }}

The waveshaper’s antiderivative antialiasing and the reverb’s damping and diffusion live in [nonlinear_math.ospml](https://github.com/Nimblesite/osprey/blob/main/src/osprey/packages/synth/src/nonlinear_math.ospml). The synthesis proof suite reads the production Osprey functions named in the synthesis specification, and checks the resulting proofs on GC, ARC and WebAssembly. Run `make _test_synthesis_proofs` to check them locally.

Where a formula calls `floatDiv`, it uses IEEE floating-point division. The `/` operator raises an `Arith` effect on division by zero. The renderer wraps an overflow and turns a division by zero into silence:

```ospml
main () =
    handle Arith
        overflow _ _ _ wrapped => wrapped
        divideByZero _ _ => 0.0
        remainderByZero _ => 0
    started (request (args ()))
```

The policy is one handler you can read and replace, not a crash buried in a plugin.

## Why write it in Osprey

- **No collector pauses.** Prism and its renderer build with `--memory=arc`, which uses reference counting, so memory is freed at known points rather than by a garbage collector that stops the program.
- **Failures are values.** A malformed patch, an out-of-range parameter or an unreadable sample returns a typed error. Osprey code never panics or aborts.
- **The oscillators share one implementation.** Native Osprey functions in `waves.ospml` supply the polyBLEP and polyBLAMP corrections.
- **One project file.** Notes, patches, routing and mix live in one JSON file, defined by a typed model (`daw.td`). Every edit in the film was saved to it, so a song diffs and versions in git like code.

## Run it

```sh
make -C src/toolchain/targets/macos prism          # install Prism for Spotlight
make -C src/toolchain/targets/macos daw-render SONG=src/osprey/examples/daw/tracks/phosphor-drift.json
make -C src/toolchain/targets/macos daw-play SONG=src/osprey/examples/daw/tracks/phosphor-drift.json
```

`daw-render` bounces a song to `target/daw/<song>/song.wav` with no window, so a mixdown can run from a script.

## Current limits

- **Offline rendering:** Play renders the selected bars offline and then plays the file; there is no real-time audio callback yet. A patch edit made while a patch is playing is heard after you stop and play again.
- **Platforms:** the full DAW runs on macOS. On Windows, only the Prism patch editor runs today.
- **Plugins:** VST3 hosting is incomplete, and a patch that holds a VST3 node can't be saved yet. CLAP and MPE input are not supported.
