---
layout: page.njk
title: "A Synthesizer With Proofs: How Osprey Checks Its DSP Maths"
excerpt: "@osprey/synth is a modular synthesis library, and Prism is the DAW built on it. Their DSP formulas are written in Osprey, and Lean proofs about that exact code run in CI, so a change to the maths cannot slip through unnoticed."
description: "What @osprey/synth and the Prism DAW do, and why mathematical proofs about audio code matter: equal-power panning, overlap-add, the FFT and antialiased waveshaping, each checked against the shipped Osprey source."
date: 2026-11-24
tags: ["blog", "audio", "synthesizer", "dsp", "daw", "formal-verification", "lean"]
author: "Christian Findlay"
readingTime: 7
---

Audio code fails quietly. A wrong sign in an FFT, a pan law that is slightly off, or a window that does not quite overlap still produces sound, and it usually sounds almost right. You find the problem weeks later, as a stereo image that leans, a mix that gets louder as you pan, or a faint ripple in a spectral effect. Unit tests catch some of this, but they only check the inputs you thought to try.

[Osprey](/) is safe, elegant and fast. Its synthesis library and the DAW built on it take a stronger approach to audio maths: the formulas live in ordinary Osprey code, and Lean proofs about that exact code run in CI.

{% videoPlayer "prism-daw", true %}

## @osprey/synth

`@osprey/synth` is a modular synthesis library with no UI and no audio device code. You build a typed graph of components, connect their ports, and render it. The graph checks that connected ports agree on rate, unit and channel count. It rejects a feedback loop unless that loop goes through an explicit delay.

The library includes:

- **Oscillators and wavetables:** sine, additive partials, and wavetables that are bandlimited at each octave level.
- **FM and phase modulation:** a phase operator that follows Smart's Wave Pulse Phase Modulation equations from DAFx 2025.
- **Physical modelling:** modal banks for strings, bars, membranes and bells, plus digital waveguides with loss filters and dispersion.
- **Granular synthesis:** grain clouds from a WAV file or a live capture, with jitter, spreads and reproducible seeds.
- **Spectral processing:** freeze, band filtering, pitch shift, smear and morph, built on its own FFT.
- **Effects:** waveshaping with antialiasing (ADAA) and oversampling, a feedback-delay-network reverb, a frequency shifter, filters, envelopes, delays and mixers.
- **Fitting:** an offline optimiser that tunes a patch's parameters to match a reference recording's spectrum.

Every component follows the same pattern: `configure` rejects invalid settings with a typed error, `initial` creates explicit state, and `next` computes one sample. There are no hidden globals and no panics.

## Prism

Prism is the working title for the DAW built on `@osprey/synth`. It has a timeline, a piano roll, a mixer with buses and sends, and a patch editor where every voice is a graph of synth components. It can also host VST3 instruments and effects, each in a separate worker process.

Prism has one rule above the others: **every module that changes the signal is visible.** There is no hidden limiter, clipper, normaliser or automatic gain anywhere, including on Master. If you want a limiter on Master, you add one and you can see it.

## Why proofs matter for audio

That rule is about trust. You should be able to see what touches your signal. Proofs apply the same idea to the maths inside each module. A proof does not say a module sounds good. It says the code computes the formula it claims to compute, for every input, not only the inputs a test tried.

Here is an example. Prism pans a track with the MIDI RP-036 equal-power law. This is the shipped code from the DAW's arrangement module:

```osprey-ml
(** Equal-power pan gains for pan −1 (left) to 1 (right). *)
export panLeft pan = cos (0.7853981633974483 * (pan + 1.0))

(** Right-channel equal-power pan gain. *)
export panRight pan = sin (0.7853981633974483 * (pan + 1.0))
```

Equal power means the two channel powers always sum to one, so a sound keeps the same loudness as it moves across the stereo field. The Lean theorem `panEqualPower` states exactly that about these two functions:

```lean
theorem panEqualPower (ops : Ops α) (pan : α)
    (pythagoras : ∀ θ, ops.plus (ops.times (ops.cos θ) (ops.cos θ))
                         (ops.times (ops.sin θ) (ops.sin θ)) = ops.literal 1) :
    let θ := ops.times (ops.binary64 0x3fe921fb54442d18) (ops.plus pan (ops.literal 1))
    ArrangeMath.panLeft ops pan = ops.cos θ ∧ ArrangeMath.panRight ops pan = ops.sin θ ∧
      ops.plus (ops.times (ArrangeMath.panLeft ops pan) (ArrangeMath.panLeft ops pan))
        (ops.times (ArrangeMath.panRight ops pan) (ArrangeMath.panRight ops pan)) = ops.literal 1
```

The `ArrangeMath.panLeft` in this theorem is not a copy of the code. The proof build parses the shipped `.ospml` file with Osprey's own frontend and turns its syntax tree into the definition the theorem talks about. If someone edits the pan formula, the definition changes, and the proof fails.

## Proofs that fail when the code is wrong

A proof that still passes after the code breaks proves nothing. So CI also breaks the code on purpose. It applies fifty-one deliberate edits to the production source, such as flipping a sign, swapping two arguments, or changing a denominator or a cosine coefficient. For each edit it regenerates the definitions and checks that the matching theorem now fails.

Osprey's own proof kernel checks every certificate in three builds: native code with tracing GC, native code with ARC, and WebAssembly. CI also replays an old FFT certificate against an environment where a subtraction was replaced with an addition, and all three kernels must reject it.

## What is proved today

<<<<<<< HEAD
The [synthesis specification](/spec/synthesis/) lists each theorem, the source it checks and the paper it comes from. Among them:
=======
The [synthesis specification](/spec/synthdsp/) lists each theorem, the source it checks and the paper it comes from. Among them:
>>>>>>> origin/fixes

- **Mixing:** sending x at gain g and y at gain h onto a bus gives g·x + h·y in either order, and the power of the mix follows Puckette's power-of-a-sum equation.
- **Overlap-add:** the spectral engine's accumulator rebuilds an unchanged signal exactly when its window products sum to one, and only then (Smith's equation 9.44, in both directions). The periodic Hann window's squared values sum to 3/2 at a hop of N/4 and to 3 at a hop of N/8, which sets the normalising gains the code uses.
- **The FFT:** the production radix-2 butterfly computes the full transform's bins from its two halves, real input gives conjugate-symmetric bins, and the transform is linear.
- **Waveshaping:** the antialiased shaper's divided difference is the average of the curve over the interval, by the fundamental theorem of calculus (Parker, Zavalishin and Le Bivic, DAFx 2016). The clippers hold their rails, and opposite inputs cancel.
- **Filters and delays:** the allpass recurrence has unit gain at steady state (Smith 5.2), and a delay line delivers every sample in arrival order.
- **Grains and envelopes:** grain windows are exactly zero at both ends, so grains start and stop without clicks.

## What the proofs do not claim

These proofs work over exact arithmetic, plus laws for the scalar operations that are stated in each theorem. They show that the code follows the right sequence of operations. They do not yet bound the floating-point rounding error of a whole render. A compiled FFT test separately compares every bin against the direct Fourier sum to within 1e-12, but an end-to-end numerical error bound is still open work.

The audio engine has limits too. Prism renders offline and then plays the result; there is no real-time audio callback yet. VST3 hosting is incomplete. The full DAW runs on macOS, and Windows runs the patch editor.

## Try it

Read the [audio synthesis guide](/docs/audio-synthesis/), or browse the [synth package source](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/synth). Osprey is actively evolving. Try it on your projects, validate it against your workload, and assess the risks before relying on it in production.
