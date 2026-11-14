# @osprey/synth

Pure synthesis components for Osprey, extracted from Prism. The library contains no UI or device playback code. Prism's editor and playback adapter remain in the example; Light Orchestra uses this package as a separate consumer.

The initial `synth::Voice` module supplies pitch conversion, periodic wave shapes, LFO rates, patch sampling and WAV encoding. `synth::Patch` is generated from [patch.td](patch.td). This initial renderer retains Prism's 11,025 Hz/8-bit preview behaviour and approximate filtering; it is not yet a real-time, bandlimited production engine.

The [library contract](../../../../docs/specs/runtime/Synthesis.md) defines modular typed graphs, routable/chained components, stateful audio processing, UI-independent Signals and future VST3 hosting. The [research collection](../../../../docs/research/synthesis/README.md) links fundamentals and modern papers to concrete experiments. These are delivery requirements; unimplemented features are marked as such.

Regenerate a model with the upstream typeDiagram emitter:

```sh
node scripts/make/osprey-schema.mjs /path/to/typediagram/dist src/osprey/packages/synth/patch.td src/osprey/packages/synth/src/model.ospml synth
```

`Graphs` validates typed ports and compiles a deterministic schedule, rejecting cycles without explicit delay. `Parameters` checks edits against descriptors and applied revisions; `Observations` reads component values through `Signal<Observation>` without a UI dependency. `Oscillators`, `Envelopes` and `Filters` provide explicit-state scalar reference processing. These record/list-based paths have **not** been verified as allocation-free audio callbacks.

Run `make -C src/osprey/packages synth` for the independent package journeys and the coverage gate. The suite covers routing, delay feedback, corrupt graph records, same-kind component Signals, stale edits, waveform/state continuity, ADSR and filter responses across sample rates. The current shared renderer is exercised by `make -C src/toolchain/targets/tests synth`, which drives Prism's complete editor and checks the samples and host playback requests.

`Components` supplies mono oscillator, filter, gain, mixer, delay and output nodes. `Renderer.start` validates/allocates an offline reference graph, `Renderer.block` carries state across blocks, and `Renderer.automated` applies ordered parameter changes at exact sample positions. `Renderer.observed` produces the next typed meter/parameter snapshot for Signal handlers. The current reference graph supports sine sources and mono routing; other oscillator shapes are available through `Oscillators` directly. VST3, real-time callback execution, multichannel graph rendering and concurrent Signal publication remain pending.

`Performance.wave` provides UI-free scalar Glass, Bass, additive Pulse and FM Orbit timbres, with harmonic tone and metallic ring-modulation controls. Light Orchestra calls it from its producer thread and keeps delay storage and audio devices in its host. Echo and Schroeder-style reverb currently belong to that host adapter. They are pending extraction into reusable graph processors. Oversampling and measured alias rejection remain outstanding.
