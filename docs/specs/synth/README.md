# Synthesis, Prism and devices

## 🚨 DIRECTIVE: PERFORMANCE OPTIMISATION NECESSARY, ESPECIALLY ON EMBEDDED DEVICES 🚨

The synthesis engine must get much faster: it runs in Prism and on the [Prism device](PrismDevice.md), whose CPU is slower and whose deadline is hard. See [DEVICE-PERF].


The synthesis toolkit, the Prism daw, Pocket Orchestra, instruments and macros, and the hardware Prism device.

[All specification subjects](../README.md)

`@osprey/synth` is a UI-free synthesis library. Prism and Pocket Orchestra are applications built on it. An **instrument** is a Prism patch with a face of macro controls, and a **Prism device** is a box that plays that patch with no computer attached, its knobs turning the same macros.

## The synthesis toolkit

- [Synthesis toolkit [SYNTH]](Synthesis.md): components, routing, channels, signals, modulation and verification.
- [Rendering and audio devices](SynthRendering.md): the real-time contract, stateful blocks and continuous device playback.
- [DSP, spectral processing and proofs](SynthDsp.md): DSP quality, Lean proofs of the shipped formulas, spectral processing and formula melodies.
- [Hosting: VST3, MIDI and workers](SynthHosting.md): plug-in hosting, MIDI input and generators, worker transport and hosted modulation.
- [Advanced synthesis](SynthAdvanced.md): granular, physical, neural and combined methods with their Prism interfaces.

## Applications

- [The daw [DAW]](Daw.md): Prism, the modular daw.
- [Pocket Orchestra [ORCHESTRA]](PocketOrchestra.md): phones as instruments around one host.

## Instruments and devices

- [Instrument builder [PRISM-INSTRUMENT]](PrismInstrument.md): instrument faces, macros, MIDI learn, OSC and device faces.
- [Prism device [DEVICE]](PrismDevice.md): firmware, engine, controls, the link to Prism and the factory.
- [Prism device hardware [DEVICE-HW]](PrismDeviceHardware.md): the Compute Module 5 board, latency budget, compute, enclosure constraints and the AUD 500 budget.

The device is compiled with the [embedded targets](../compiler/EmbeddedTargets.md). Delivery plans are in [docs/plans/synth](../../plans/synth/README.md); research is in [docs/research/synthesis](../../research/synthesis/README.md).
