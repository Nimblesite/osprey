# Prism synth [PRISM-ROUTING-EDITOR]

Prism is the modular instrument editor for `@osprey/synth`. Its native component picker, port controls and parameter fields share a typed graph with the canvas. Start with an oscillator, filter, gain and output; add more components, choose an output port, connect it to an input and audition the resulting patch. Invalid connections leave the previous graph unchanged, and deleting a component removes its cables.

The current menu offers sine oscillators, low-pass filters, gains, mixers, explicit delays and outputs. The graph library validates port rates, units, channel counts and feedback cycles. Parameter edits are checked against each component's descriptor. This remains work in progress: draggable canvas routing, MIDI generators, modulators and plug-in nodes are being integrated.

## Run

```sh
make -C src/toolchain/targets/macos synth
```

**Audition patch** renders 256 ms of the actual connected mono graph in Osprey at 48 kHz and exports IEEE float32 WAV audio. The macOS host plays it through `afplay`; an exclusively created temporary file is removed after playback. This bounded offline preview is not yet the continuous audio engine, and preview device adapters for Windows and other platforms remain pending. Incomplete audio routes and host playback failures are shown explicitly.

## Test

```sh
make -C src/toolchain/targets/tests synth
make -C src/toolchain/targets/tests synth-vst
```

The application tests exercise both the existing fixed-patch interaction coverage and the new modular surface through real host events. The modular journey creates independent components, wires a chain, renders a playback request, changes parameters, rejects invalid input and removes a connected node. The synth package separately checks graph replacement, DSP state continuity, sample output and float32 RIFF bytes.

The VST target builds a separate worker against a pinned official SDK, scans actual ADelay and mda binaries, processes audio/MIDI and checks automation/state restoration. That worker is not yet connected to Prism's component menu or its graph audio transport. See the [delivery checklist](../../../../docs/plans/prism-synth-routing.md) and [worker scope](../../../toolchain/hosts/synth_vst/README.md) for verified capabilities and remaining work.
