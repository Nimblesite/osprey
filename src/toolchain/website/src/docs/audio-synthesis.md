---
layout: page
title: "Audio synthesis: write a DAW and synths in Osprey"
description: "Prism is a DAW written in Osprey. Patch synth voices, build FX from raw maths formulas, mix through side-chained buses and render the song to WAV."
permalink: /docs/audio-synthesis/
---

{% from "native-capture.njk" import capture %}

Prism is a DAW written entirely in Osprey, on the [`@osprey/synth`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/synth) library. In the film below, a 17-track song is edited by clicking through the real app: a note's velocity, a reverb send, a Delay patched into a voice, and a bus send. Then the edited bars play.

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

Every effect in Prism is built from raw formulas. You write the formula as a `def` in a Lean file, and Osprey generates the code from it, so you never hand-code the maths in two places. The bus compressor's soft-knee gain law (Giannoulis, Massberg and Reiss, JAES 2012) is [DynamicsMath.lean](https://github.com/Nimblesite/osprey/blob/main/src/osprey/packages/synth/src/DynamicsMath.lean):

```text
def curve (level threshold ratio knee : Float) : Float :=
  let over := level - threshold
  if 2 * over < -knee then level
  else if 2 * over <= knee && knee > 0 then
    level + (1 / ratio - 1) * (over + knee / 2) * (over + knee / 2) / (2 * knee)
  else threshold + over / ratio

def reduction (level threshold ratio knee : Float) : Float :=
  curve level threshold ratio knee - level
```

On every build, the toolchain turns that file into the `synth::DynamicsMath` module, generated into the untracked `.osprey/math/` folder. The effect itself is a fold over samples that calls those functions ([bus_mix.ospml](https://github.com/Nimblesite/osprey/blob/main/src/osprey/examples/daw/src/bus_mix.ospml)):

```ospml
reduced (c, attack, release, run, key) =
    target = synth::DynamicsMath::reduction (synth::DynamicsMath::decibels key, c.threshold, c.ratio, c.knee)
    level = synth::DynamicsMath::smoothed (run.held.level, target, attack, release)
```

{{ capture("/assets/images/native/prism-mix-bus.webp", "Prism Mix page: glow routed to the pads group bus, whose EQ and compressor take their detector from the kick, beside the Space reverb send and Master with its soft limiter", "Prism Mix on macOS: the pads bus compressor is keyed from the kick, and its gain law is the Lean curve above.", 1400, 914) }}

To build your own FX:

1. Write each formula as a `def` in a `.lean` file in your project's sources.
2. Run `osprey math <file.lean>` to print the Osprey module it generates.
3. Call the generated functions from your effect code, which keeps the state and wiring.

The same pattern gives the waveshaper its antiderivative antialiasing (Parker, Zavalishin and Le Bivic, DAFx 2016), the reverb its damping and diffusion, and the limiter its four-times-oversampled true peak, in [NonlinearMath.lean](https://github.com/Nimblesite/osprey/blob/main/src/osprey/packages/synth/src/NonlinearMath.lean) and DynamicsMath.lean. `make -C src/osprey/examples/proof math` checks that Lean and the generated Osprey compute the same bits.

The formulas and the code around them divide by zero differently, and the difference is deliberate. Generated formulas keep Lean's IEEE division, so they compute exactly what Lean computes; a zero divisor there gives an infinity. In the effect code you write around them, `/` and integer overflow raise an `Arith` algebraic effect, and the compiler rejects the program unless a handler covers it. The renderer wraps an overflow and turns a division by zero into silence:

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
- **The oscillators are Lean too.** The polyBLEP and polyBLAMP band-limiting corrections are generated from Lean the same way as the FX.
- **One project file.** Notes, patches, routing and mix live in one JSON file, defined by a typed model (`daw.td`). Every edit in the film was saved to it, so a song diffs and versions in git like code.

## Run it

```sh
make -C src/toolchain/targets/macos daw            # open Prism on a song
make -C src/toolchain/targets/macos daw-render SONG=src/osprey/examples/daw/tracks/phosphor-drift.json
make -C src/toolchain/targets/macos daw-play SONG=src/osprey/examples/daw/tracks/phosphor-drift.json
```

`daw-render` bounces a song to `target/daw/<song>/song.wav` with no window, so a mixdown can run from a script.

## Current limits

- **Offline rendering:** Play renders the selected bars offline and then plays the file; there is no real-time audio callback yet. A patch edit made while a patch is playing is heard after you stop and play again.
- **Platforms:** the full DAW runs on macOS. On Windows, only the Prism patch editor runs today.
- **Plugins:** VST3 hosting is incomplete, and a patch that holds a VST3 node can't be saved yet. CLAP and MPE input are not supported.
