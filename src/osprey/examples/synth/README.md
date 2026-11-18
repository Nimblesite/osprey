# Prism synth [PRISM-ROUTING-EDITOR]

Prism opens its signal-flow studio with the original node cards, cables, inspector, meters and playable keyboard. The module palette, editable routing and VST controls use that same studio and patch.

Prism is the modular instrument editor for `@osprey/synth`. Its native component picker, port controls and parameter fields share a typed graph with the canvas. Start with an oscillator, filter, gain and output; add more components, choose an output port, connect it to an input and audition the resulting patch. Invalid connections leave the previous graph unchanged, and deleting a component removes its cables.

The menu offers grain clouds, sine oscillators, low-pass filters, gains, mixers, delays, MIDI note generators, normalized LFOs, stereo conversion, modulated stereo gain and mono/stereo outputs. Scan a VST3 bundle to add independent instrument or effect instances. Drag nodes, connect canvas ports or native port controls, and route LFOs to compatible effect parameters. The graph library validates port rates, units, channel counts and feedback cycles; parameter edits are checked against each component's descriptor.

## Grain cloud [SYNTH-ADVANCED-GRAINS-INTEGRATION]

Open **Modules → Grain cloud** to add an independent granular instrument. Its card displays the actual source waveform using Prism's existing card styling. The initial source is a built-in 1,024-frame harmonic sample; importing your own sample file is not implemented yet. The library factory accepts other checked mono sample buffers.

Connect its orange stereo output to **Stereo output**, or through **Modulated stereo gain** and compatible stereo effects. Remove the old mono output when replacing the initial patch: playback requires exactly one output. Select the cloud and open **Patch controls** to edit source position, playback rate, grain duration, per-grain gain, pan, onset interval and timing jitter. Timing fields require whole 48 kHz output-frame counts; jitter must be less than the onset interval. Zero rate holds a source position and negative rate reverses it. The default pool allows 16 overlapping grains and reports exhaustion explicitly.

The keyboard changes the source playback rate for the selected musical pitch, alongside the existing oscillators and MIDI generators. **Audition** and **Play** use the shared stereo graph/device path; each cloud retains its grain cursors, envelopes and seeded scheduling clock across blocks, including a pause for a downstream VST response. Graph edits still require stopping/restarting the captured playback graph. Strict static checks and the native build pass; manual UI inspection confirmed palette insertion, the source waveform and seven inspector controls. A manual stereo cable and audition also completed, but the device reported three empty-queue callbacks. Exact audio verification and automated UI journeys remain outstanding, and sustained playback reliability remains unfinished as described below.

## Run

```sh
make -C src/toolchain/targets/macos synth             # or: bash src/osprey/examples/synth/run.sh
make -C src/toolchain/targets/windows synth           # in its sandbox on Windows
make -C src/toolchain/targets/windows/tests synth     # its Windows self-test against goldens/windows
```

Open **Modules** and click a component to add it, or drag it onto the canvas. **Patch controls** opens the selected-module inspector: choose a **Module**, edit its **Parameters**, then use **Connections** to select an output and connect or disconnect an input. Only the chosen module's controls are shown; **Remove module** removes that module and its cables. Compact **Audition**, **Play** and **Stop** controls remain together above the inspector; **Add to patch** holds component and VST creation. The inspector journey exercises selection and routing through these visible controls [PRISM-ROUTING-EDITOR].

Drag from an output socket to a compatible input socket to draw a cable; clicking the output and then the input also works. Click an exposed cable to highlight it, then click **Delete cable ×** or press **Backspace/Delete**. **Escape** cancels a pending connection. A rejected connection preserves the existing cables. The Filter's purple **CUTOFF** input accepts normalized modulation (unplugged uses the Cutoff value); its cyan monitor socket routes the same audio output to the Scope. These interactions are covered by [PRISM-ROUTING-CABLES].

The keyboard stays docked to the bottom of the window. **Close keyboard ×** hides it; **Keyboard** in the header restores it.

**Play patch** continuously produces the captured graph until **Stop playback**. **Audition patch** uses the same device path for a 256 ms preview. On macOS the packaged worker sends checked mono/stereo 48 kHz float32 PCM into a bounded AudioQueue; each VST instance survives successive graph blocks. Built-in DSP state and sample-positioned MIDI/modulation also continue between blocks. Stop, failure and ordinary application shutdown release the workers.

The running graph is frozen at start. Stop and play again to apply editor changes. The producer allocates outside the audio callback and transfers 2048-frame blocks. The native journey checks sustained graph audio and that Stop leaves no owned workers alive. Repeated runs have exposed queue starvation, which Prism reports in the editor. Windows uses WinMM for audition and playback of built-in patches. VST3 hosting currently uses the macOS worker. Graph persistence and latency compensation remain pending. On macOS a mono audition without the packaged worker can use `afplay`. Continuous playback requires the packaged worker.

If the device queue runs empty, Prism retains a visible warning with the observed count. The backend emits structured `audio.queue_empty` diagnostics from the producer thread; the callback never logs. A warning is not an acoustic measurement, and reducing patch workload may help.

## Test

```sh
make -C src/toolchain/targets/tests synth
make -C src/toolchain/targets/tests synth-vst
make -C src/toolchain/targets/macos/tests synth-headless
make -C src/toolchain/targets/macos/tests synth
```

The application tests exercise both the existing fixed-patch interaction coverage and the new modular surface through real host events. The modular journey creates independent components, wires a chain, renders a playback request, changes parameters, rejects invalid input and removes a connected node. A device journey captures every float32 block Audition writes to the audio device through an effect and measures it: pitch from zero crossings, RMS, peak, click-free block joins, gain and low-pass attenuation. The synth package separately checks graph replacement, DSP state continuity, sample output and float32 RIFF bytes.

The VST target builds a separate worker against a pinned official SDK, scans actual ADelay and mda binaries, processes audio/MIDI and checks automation/state restoration. The native headless journey drives the menu and canvas through a real MIDI instrument, two real effects and LFO-controlled gain; it verifies 18 graph transactions across three retained workers, exact effect-chain PCM and sample-positioned MIDI/automation. Windowed screenshots, live graph replacement, graph persistence and plug-in latency compensation remain unfinished. See the [delivery checklist](../../../../docs/plans/prism-synth-routing.md) and [worker scope](../../../toolchain/hosts/synth_vst/README.md) for verified capabilities and remaining work.


VST3 bundles can now be scanned from the native **VST3 bundle** field. **Add VST3** lists the actual classes; choosing one loads its checked stereo/MIDI ports and normalized controls into an independent graph node. The worker is packaged beside Prism. Scan failures remain visible, and stale responses cannot add nodes to a newer module selection. External-node audio runs through the preview engine using persistent worker instances.

`make -C src/toolchain/targets/macos/tests synth-vst-headless` drives the native application against real SDK plug-ins and real bounded worker processes. `synth-vst` runs the same journey in a window. The headless records are accessibility trees; the windowed run records screenshots. These commands do not substitute a fake successful scan or descriptor.

Latest verification (2026-11-16): the final sustained native reruns failed the unchanged zero-empty-queue assertion, including a rerun without concurrent test compilation. Earlier successful 985088-frame evidence is not reproducible reliability. Exact finite VST-chain PCM still passes. Continuous playback still needs producer-throughput work; do not treat this branch as release-ready.
