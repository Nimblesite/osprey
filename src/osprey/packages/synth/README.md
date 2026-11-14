# @osprey/synth

Pure synthesis components for Osprey, extracted from Prism. The library contains no UI or device playback code. Prism's editor and playback adapter remain in the example; Light Orchestra will use this package as a separate consumer.

The initial `synth::Voice` module supplies pitch conversion, periodic wave shapes, LFO rates, patch sampling and WAV encoding. `synth::Patch` is generated from [patch.td](patch.td). This initial renderer retains Prism's 11,025 Hz/8-bit preview behaviour and approximate filtering; it is not yet a real-time, bandlimited production engine.

The [library contract](../../../../docs/specs/runtime/Synthesis.md) defines modular typed graphs, routable/chained components, stateful audio processing, UI-independent Signals and future VST3 hosting. The [research collection](../../../../docs/research/synthesis/README.md) links fundamentals and modern papers to concrete experiments. These are delivery requirements; unimplemented features are marked as such.

Regenerate a model with the upstream typeDiagram emitter:

```sh
node scripts/make/osprey-schema.mjs /path/to/typediagram/dist src/osprey/packages/synth/patch.td src/osprey/packages/synth/src/model.ospml synth
```

`Graphs` validates typed ports and compiles a deterministic schedule, rejecting cycles without explicit delay. `Parameters` checks edits against descriptors and applied revisions; `Observations` reads component values through `Signal<Observation>` without a UI dependency. `Oscillators`, `Envelopes` and `Filters` provide explicit-state scalar reference processing. These record/list-based paths have **not** been verified as allocation-free audio callbacks.

Run `make -C src/osprey/packages synth` for the independent package journeys and the coverage gate. The suite covers routing, delay feedback, corrupt graph records, same-kind component Signals, stale edits, waveform/state continuity, ADSR and filter responses across sample rates. The current shared renderer is exercised by `make -C src/toolchain/targets/tests synth`, which drives Prism's complete editor and checks the samples and host playback requests.
