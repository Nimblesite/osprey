# @osprey/synth

Pure synthesis components for Osprey, extracted from Prism. The library contains no UI or device playback code. Prism's editor and playback adapter remain in the example; Light Orchestra will use this package as a separate consumer.

The initial `synth::Voice` module supplies pitch conversion, periodic wave shapes, LFO rates, patch sampling and WAV encoding. `synth::Patch` is generated from [patch.td](patch.td). This initial renderer retains Prism's 11,025 Hz/8-bit preview behaviour and approximate filtering; it is not yet a real-time, bandlimited production engine.

The [library contract](../../../../docs/specs/runtime/Synthesis.md) defines modular typed graphs, routable/chained components, stateful audio processing, UI-independent Signals and future VST3 hosting. The [research collection](../../../../docs/research/synthesis/README.md) links fundamentals and modern papers to concrete experiments. These are delivery requirements; unimplemented features are marked as such.

Regenerate a model with the upstream typeDiagram emitter:

```sh
node scripts/make/osprey-schema.mjs /path/to/typediagram/dist src/osprey/packages/synth/patch.td src/osprey/packages/synth/src/model.ospml synth
```

The current shared renderer is exercised by `make -C src/toolchain/targets/tests synth`, which drives Prism's complete editor and checks the samples and host playback requests.
