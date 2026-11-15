# Prism synth [PRISM-ROUTING-EDITOR]

Prism is the modular instrument editor for `@osprey/synth`. Its native component picker, port controls and parameter fields share a typed graph with the canvas. Start with an oscillator, filter, gain and output; add more components, choose an output port, connect it to an input and audition the resulting patch. Invalid connections leave the previous graph unchanged, and deleting a component removes its cables.

The menu offers sine oscillators, low-pass filters, gains, mixers, delays, MIDI note generators, normalized LFOs, stereo conversion, modulated stereo gain and mono/stereo outputs. Scan a VST3 bundle to add independent instrument or effect instances. Drag nodes, connect canvas ports or native port controls, and route LFOs to compatible effect parameters. The graph library validates port rates, units, channel counts and feedback cycles; parameter edits are checked against each component's descriptor.

## Run

```sh
make -C src/toolchain/targets/macos synth
```

**Play patch** continuously produces the captured graph until **Stop playback**. **Audition patch** uses the same device path for a 256 ms preview. On macOS the packaged worker sends checked mono/stereo 48 kHz float32 PCM into a bounded AudioQueue; each VST instance survives successive graph blocks. Built-in DSP state and sample-positioned MIDI/modulation also continue between blocks. Stop, failure and ordinary application shutdown release the workers.

The current running graph is frozen at start: stop and play again to apply editor changes. The producer allocates outside the audio callback and transfers 2048-frame blocks; this is not a low-latency, allocation-free synthesis engine. The native journey verifies over twenty seconds of actual graph/device processing without an empty-queue callback, then verifies that Stop leaves no owned workers alive. This is measured fixture evidence, not a guarantee for arbitrary third-party plug-ins or workloads. Windows and other device adapters, graph persistence and latency compensation remain pending. Without the packaged worker, a mono audition can still use the existing temporary-WAV `afplay` adapter; continuous playback reports that the worker is required.


## Test

```sh
make -C src/toolchain/targets/tests synth
make -C src/toolchain/targets/tests synth-vst
```

The application tests exercise both the existing fixed-patch interaction coverage and the new modular surface through real host events. The modular journey creates independent components, wires a chain, renders a playback request, changes parameters, rejects invalid input and removes a connected node. The synth package separately checks graph replacement, DSP state continuity, sample output and float32 RIFF bytes.

The VST target builds a separate worker against a pinned official SDK, scans actual ADelay and mda binaries, processes audio/MIDI and checks automation/state restoration. The native headless journey drives the menu and canvas through a real MIDI instrument, two real effects and LFO-controlled gain; it verifies 18 graph transactions across three retained workers, exact effect-chain PCM and sample-positioned MIDI/automation. Windowed screenshots, live graph replacement, graph persistence and plug-in latency compensation remain unfinished. See the [delivery checklist](../../../../docs/plans/prism-synth-routing.md) and [worker scope](../../../toolchain/hosts/synth_vst/README.md) for verified capabilities and remaining work.


VST3 bundles can now be scanned from the native **VST3 bundle** field. **Add VST3** lists the actual classes; choosing one loads its checked stereo/MIDI ports and normalized controls into an independent graph node. The worker is packaged beside Prism. Scan failures remain visible, and stale responses cannot add nodes to a newer module selection. External-node audio is not yet connected to the preview engine.

`make -C src/toolchain/targets/macos/tests synth-vst-headless` drives the native application against real SDK plug-ins and real bounded worker processes. `synth-vst` runs the same journey in a window. The headless records are accessibility trees; the windowed run records screenshots. These commands do not substitute a fake successful scan or descriptor.
