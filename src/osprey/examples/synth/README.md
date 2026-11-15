# Prism synth [PRISM-ROUTING-EDITOR]

Prism is the modular instrument editor for `@osprey/synth`. Its native component picker, port controls and parameter fields share a typed graph with the canvas. Start with an oscillator, filter, gain and output; add more components, choose an output port, connect it to an input and audition the resulting patch. Invalid connections leave the previous graph unchanged, and deleting a component removes its cables.

The menu offers sine oscillators, low-pass filters, gains, mixers, delays, MIDI note generators, normalized LFOs, stereo conversion, modulated stereo gain and mono/stereo outputs. Scan a VST3 bundle to add independent instrument or effect instances. Drag nodes, connect canvas ports or native port controls, and route LFOs to compatible effect parameters. The graph library validates port rates, units, channel counts and feedback cycles; parameter edits are checked against each component's descriptor.

## Run

```sh
make -C src/toolchain/targets/macos synth
```

**Audition patch** renders 256 ms of the actual connected mono or stereo graph in Osprey at 48 kHz and exports IEEE float32 WAV audio. The macOS host plays it through `afplay`; an exclusively created temporary file is removed after playback. Hosted graphs transfer 2048-frame graph blocks, split into VST processing blocks of at most 256 frames; built-in DSP state and one worker per VST instance survive every block. Workers close when the audition finishes or fails. This bounded offline preview is not yet the continuous audio engine, and preview device adapters for Windows and other platforms remain pending. Incomplete audio routes and host playback failures are shown explicitly.

## Test

```sh
make -C src/toolchain/targets/tests synth
make -C src/toolchain/targets/tests synth-vst
```

The application tests exercise both the existing fixed-patch interaction coverage and the new modular surface through real host events. The modular journey creates independent components, wires a chain, renders a playback request, changes parameters, rejects invalid input and removes a connected node. The synth package separately checks graph replacement, DSP state continuity, sample output and float32 RIFF bytes.

The VST target builds a separate worker against a pinned official SDK, scans actual ADelay and mda binaries, processes audio/MIDI and checks automation/state restoration. The native headless journey drives the menu and canvas through a real MIDI instrument, two real effects and LFO-controlled gain; it verifies 18 graph transactions across three retained workers, exact effect-chain PCM and sample-positioned MIDI/automation. Windowed screenshots, continuous device output, graph persistence and plug-in latency compensation remain unfinished. See the [delivery checklist](../../../../docs/plans/prism-synth-routing.md) and [worker scope](../../../toolchain/hosts/synth_vst/README.md) for verified capabilities and remaining work.


VST3 bundles can now be scanned from the native **VST3 bundle** field. **Add VST3** lists the actual classes; choosing one loads its checked stereo/MIDI ports and normalized controls into an independent graph node. The worker is packaged beside Prism. Scan failures remain visible, and stale responses cannot add nodes to a newer module selection. External-node audio is not yet connected to the preview engine.

`make -C src/toolchain/targets/macos/tests synth-vst-headless` drives the native application against real SDK plug-ins and real bounded worker processes. `synth-vst` runs the same journey in a window. The headless records are accessibility trees; the windowed run records screenshots. These commands do not substitute a fake successful scan or descriptor.
