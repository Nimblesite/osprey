# Synthesis and Perception

[All specification subjects](../README.md)

Two specifications, one for each thing that is built:

- [The synth package [SYNTH]](Synth.md): `@osprey/synth`, a UI-free synthesis library. Components, routing, signals, rendering, DSP quality and proofs, spectral processing, VST3 hosting, the advanced methods and their sources.
- [Perception](Perception.md): the applications built on the package. Perception Studio (the daw), Perception Modular (the VST3 plug-in), the instrument builder, the Perception device with its firmware and hardware, and Pocket Orchestra.

Perception depends on the synth package; the package never depends on Perception. The synthesis engine must get much faster for both, and most of all on the device: see [DEVICE-PERF](Perception.md).

The device is compiled with the [embedded targets](../compiler/EmbeddedTargets.md). Delivery plans are in [docs/plans/synth](../../plans/synth/README.md).
