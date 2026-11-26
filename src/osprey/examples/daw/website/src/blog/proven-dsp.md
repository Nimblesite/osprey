---
layout: post.njk
title: "Mathematically Proven DSP: Machine-Checked Waveguides & FFTs"
description: "From allpass unit magnitude to weighted overlap-add and antiderivative anti-aliasing: every core formula carries a Lean proof that fails if mutated."
date: "2026-11-18"
readTime: "5 min read"
tag: "DSP Proofs"
image: "/assets/blog-proofs.jpg"
alt: "Precision dark indigo oscilloscope grid with cyan harmonic spectra and glowing orange traveling wave curves"
---

Subtle DSP bugs hide for years: a 1-sample window offset in an overlap-add loop, an allpass coefficient that drifts off unit magnitude, or a waveshaper that folds aliases back into the audible band.

In Perception, core synthesis mathematics is **formally proved in Lean** with zero axioms and replayed against the shipped formulas in CI.

## What is proved today

- **Physical modeling:** `travelingWavePropagation` verifies waveguide delay behavior; `allpassMagnitude` and `allpassRecurrence` prove unit-magnitude phase dispersion.
- **Spectral & granular:** `weightedOverlapAdd`, `overlapNormalizationNecessary`, `hannEndpoints`, and `radixTwoSplit` prove exact STFT/FFT reconstruction and window energy.
- **Anti-aliasing:** `antiderivativeAverage` and `antiderivativeOffset` verify our nonlinear waveshaper's antiderivative anti-aliasing formulas.
- **Macro curves:** Hardware and UI macro exponents (0.25–4.0) are proved monotone and strictly bounded inside target parameter ranges.

If anyone introduces a regression into a DSP formula, the proof kernel rejects it before a single audio sample is ever shipped.
