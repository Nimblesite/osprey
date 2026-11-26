# Prism routing, instruments and macros [PRISM-ROUTING] [PRISM-INSTRUMENT-PLAN]

Prism becomes an editable instrument graph: add oscillators, effects, MIDI generators, modulators and VST3 host nodes, connect compatible ports, adjust parameters and hear the resulting graph. The `@osprey/synth` package remains UI-free. Existing audio and control work must remain usable while a replacement graph is validated. Instrument Play/Edit views, macros and device faces build on the same patch.

The governing contracts are [SYNTH-GRAPH], [SYNTH-SIGNALS], [SYNTH-RENDER] and [SYNTH-HOST-VST] in the [synthesis specification](../../specs/synth/Synth.md), plus the [instrument builder specification](../../specs/synth/Perception.md#instrument-builder-prism-instrument). The [advanced synthesis plan](advanced-synthesis.md) covers synthesis methods and their Prism interfaces. The [instrument designs](../../designs/prism-timeline/README.md#instrument-builder-prism-instrument) show Play, Edit and MIDI learn. Every instrument-builder item lands with loaded headless and windowed journeys and screenshot goldens.

## Delivery and release verification [PRISM-ROUTING-DELIVERY] [PRISM-ROUTING-VERIFY-COVERAGE] [PRISM-ROUTING-DIAGNOSTICS] [PRISM-ROUTING-WRAP-UP]

- [ ] Publish applied graph/parameter revisions and component activity through Signals without a UI dependency in the package.
- [ ] Exercise native and canvas interfaces with loaded headless and windowed journeys, screenshots and audio evidence; retain cross-platform availability with explicit platform limits.
- [ ] Run static analysis, required CI preparation, package coverage, root CI and the duplication gate; coordinate a single validated push and inspect its CI result.
- [ ] Verify actual window screenshots and windowed journeys on an unlocked desktop.
- [ ] Resolve reproduced playback failures before claiming smooth sustained playback.
- [ ] Record remaining limitations and remove only this task's temporary processes/artifacts.

## Acceptance evidence [PRISM-ROUTING-VERIFY]

A graph-edit test must add multiple instances, wire chains and fan-out, reject incompatible ports and illegal cycles, remove/reconnect routes, delete a connected node and verify unchanged snapshots on rejection. Audio tests must distinguish the sounds before and after a routing change, retain oscillator/filter state under parameter-only edits, and exercise block partitions and note-off ownership. UI tests must perform the edits through visible controls, not only construct the final graph directly.

Hosting means loading an existing VST3 binary, not producing a plug-in or rendering a placeholder oscillator. Plug-in discovery and loading never run on the audio callback. Scan failures, unsupported buses and absent binaries remain visible typed errors. Third-party native code is untrusted: process isolation and bounded protocol validation are part of the host design, and arbitrary plug-in safety is not implied by Osprey's guarantees. The compatibility source is the [official VST3 SDK](https://github.com/steinbergmedia/vst3sdk), including its hosting samples; record the selected revision and loaded-binary evidence before declaring hosting complete.

## Timeline, track patches and bus routing [PRISM-ROUTING-TIMELINE]

- [ ] Support nested patch racks in track and bus graphs. Exercise the design walkthrough with a second send from an occupied source outlet and a refused zero-delay loop.
- [ ] Open or focus the detached Timeline from Prism, preserving the session, playhead, selected track and undo history across both windows. Compare the rendered states with the design on the FerroUI hosts.

## Graph editor integration [PRISM-ROUTING-INTEGRATION]

- [ ] Replace the remaining fixed-patch coverage/implementation after keyboard, waveform and modulation interactions have migrated; integrate the continuous audio engine.
- [ ] Add movable node positions, with screenshot and windowed verification.

The component picker must not advertise MIDI generators, modulation processors or VST nodes until they work. The default launch entry must audition the actual graph rather than a fixed demonstration.

## Prism presentation and direct cable editing [PRISM-ROUTING-PRESENTATION] [PRISM-ROUTING-CABLES]

- [ ] Restore adaptive canvas resizing in the unified editor and verify the updated native window against the original Prism design; fixed-studio tests do not validate the graph editor.
- [ ] Verify actual mouse dragging and cable deletion in the rebuilt window, and capture the windowed regression journey.

### Granular playback [PRISM-ROUTING-GRAINS]

Keep granular integration separate from the host/release gates. Sustained zero-starvation playback, speaker output quality and comparative runtime evidence remain open. Complete native execution and screenshot evidence for the Undo/Redo journey that removes and reapplies Delay and Mixer nodes.

### Oscillator MIDI input [PRISM-ROUTING-MIDI-OSCILLATOR]

- [ ] Verify numerical pitch/velocity, note ownership, exact block-boundary gating and long playback under [SYNTH-HOST-MIDI-OSCILLATOR].
- [ ] Verify rendered release endpoints, duration, repeated note-offs and retrigger behaviour; add attack/crossfades and live topology state migration before claiming click-free realtime MIDI instruments.

Release handling under [SYNTH-HOST-MIDI-OSCILLATOR-RELEASE] must retain release length and remaining frames, latch release only on the matching note-off, preserve the tail through repeated or unrelated note-offs, and cancel release on retrigger without resetting oscillator phase. Rendered/audio evidence remains required.

## VST worker and hosted graph boundaries [PRISM-ROUTING-VST-WORKER]

`make -C src/toolchain/targets/tests synth-vst` is the repeatable worker entry point. Loaded-binary audio processing does not by itself prove live UI routing, arbitrary plug-in compatibility, process sandboxing or note-release correctness. Exact per-sample automation response and separate controller-state persistence remain open.

### Preview and native boundary [PRISM-ROUTING-AUDIO]

Generic host capability completions and offline PCM/WAV playback are not proof of sound, continuous playback or visual correctness. The real device adapter, explicit playback-failure handling and rebuilt native host still require the windowed and sustained-audio gates above.

### MIDI and stereo components [PRISM-ROUTING-MIDI]

The deterministic sample-positioned MIDI source and stereo components must execute through the audition adapter; a mono adapter that ignores the new node types is insufficient.

### Hosted graph playback [PRISM-ROUTING-AUDIO-HOSTED]

Hosted patch journeys must route a note generator through real instrument/effect binaries, edit controls, verify the stereo tail and capture PCM. This offline evidence does not replace continuous hosted-instance verification.

### Routed LFO and hosted parameters [PRISM-ROUTING-MODULATION] [PRISM-ROUTING-MODULATION-HOSTED]

Routed modulation and hosted parameter edits must preserve frozen graph ownership, reject stale replies and non-automatable edits, and demonstrate real routing semantics. Continuous playback, saved patches and windowed screenshots remain open.

### Canvas layout [PRISM-ROUTING-LAYOUT]

The UI model is generated from `editor.td`; the synth library remains UI-free. Verify node geometry, pointer interaction, accessibility bounds and actual window screenshots alongside the movable-node work above.

### Stateful blocks and graph scheduling [SYNTH-RENDER-BLOCKS] [SYNTH-RENDER-BLOCKS-GRAPH]

Stateful block processing and graph block scheduling are foundations for streaming, not evidence of continuous playback. Retain partition and PCM comparisons while connecting the graph to the device queue.

### Pending requests and persistent workers [PRISM-ROUTING-AUDIO-PENDING] [SYNTH-HOST-STREAM-SESSION] [SYNTH-HOST-STREAM-SESSION-DESKTOP]

- [ ] Add application-owned session lifetimes, per-request watchdogs and explicit shutdown in the desktop host, then connect continuous graph/device processing.

Desktop process ownership must reject malformed/nonzero receipts, bound input and output, clear retained children on shutdown and remove workers after every transaction failure. Process isolation is not a sandbox and does not claim ownership of arbitrary plug-in descendants. Windows/Linux execution remains unverified.

### Retained graph execution and device queue [PRISM-ROUTING-SESSIONS] [SYNTH-DEVICE-QUEUE]

- [ ] Feed the retained graph into a continuous audio device queue with live graph/parameter changes.
- [ ] Apply live graph edits to the running queue without restarting playback.

The device callback only publishes returned buffers and counters; decoding, allocation, waiting and logging stay on the producer thread. Four preallocated buffers bound queued memory, two transfers prefill before start, and a drain acknowledges all returned frames before cleanup. `emptyCallbacks` counts queue-empty observations, not a measured speaker underrun. Device support is macOS-only; other platforms require explicit unsupported-adapter failures until implemented.

### Continuous playback and throughput [PRISM-ROUTING-CONTINUOUS] [PRISM-ROUTING-THROUGHPUT]

- [ ] Apply graph/parameter edits transactionally during playback; the UI must not require a restart once this is complete.

Sustained producer scheduling/throughput, low-latency scheduling, zero-allocation production, state persistence, latency compensation and non-macOS device adapters remain open. Do not claim release readiness from bounded offline auditions.

## Macros in the patch [PRISM-INSTRUMENT-PATCH]

Macro controls build on the synth package's `Macros`, [DAW-CONTROLS-DIAL], [DAW-MIDI-LEARN] and [DAW-OSC]. They must work in Prism before device faces depend on them.

- [ ] **Amount in the patch ([PRISM-INSTRUMENT-AMOUNT]).** Add `amount` to `Macro` in the synth package's `graph.td`, regenerate, and save and load it with `PatchDocument`. Package journey: many macros, many amounts, save and reload, and `Macros.apply` gives the same graph.
- [ ] **Smoothing ([PRISM-INSTRUMENT-SMOOTH]).** Macro moves reach the audio through `Smoothers` at 20 ms. Journey: a step in the amount gives no sample step in the output above the smoothed bound.
- [ ] **Proofs ([PRISM-INSTRUMENT-PROOF]).** Lean proofs for `FitMath.macroValue` (monotone, in range, exact ends) and switch positions, derived from the shipped source, each failing on a source mutation, in CI.
- [ ] **Detach on direct edit ([PRISM-INSTRUMENT-AMOUNT]).** Turning a module dial that a macro drives detaches that target with a notice. Journey: detach, undo, re-attach.

## Instrument face [PRISM-INSTRUMENT-FACE]

- [ ] **Model ([PRISM-INSTRUMENT-MACRO]).** Declare the instrument in `daw.td` (controls with id, label, kind, position, size; macro targets with node, parameter, range, curve; MIDI and OSC bindings), generate the code, and store it with the patch in the project file. Journey: save, reopen, compare every field; a deleted module removes its targets with a notice; a second macro on one parameter is refused with the reason.
- [ ] **Views and modes ([PRISM-INSTRUMENT-MODES]).** Patch view and Instrument view on one track patch; Play and Edit mode commands with keys in all three keymaps. Journey: switching renders identical Master PCM before and after.
- [ ] **Play mode ([PRISM-INSTRUMENT-PLAY]).** Only controls drawn; a turn moves every target through `Macros` as one undo step. Journey: turn each control across its range and assert each target's value and the rendered Master audio change (cutoff measured from attenuation), undo restores them.
- [ ] **Edit mode ([PRISM-INSTRUMENT-EDIT]).** Add, drag, resize by handles, grid snap, rename, change kind, delete; one undo step each; fixed control size across window sizes. Journey: many controls, many moves and resizes, screenshots per step, undo and redo of each.
- [ ] **MIDI learn and OSC ([PRISM-INSTRUMENT-REMOTE]).** Learn binds a controller to a control; `/track/<n>/instrument/<control>` address with checked edits and feedback. Needs [DAW-MIDI] input and the [DAW-OSC] listener first. Journey: CC and OSC messages move the on-screen control and the targets; out-of-range messages are refused with the reason.
- [ ] **Accessibility ([DAW-ACCESS]).** Name, value, range, kind, focus order and arrow-key steps reported and tested for every control.

## Device faces [PRISM-INSTRUMENT-DEVICE]

These need the device profile model from the [device plan](device.md#device-profiles-and-the-virtual-device-device-engine-virtual).

- [ ] **Design screenshots first.** The device face with four assigned knobs and a footswitch, assigning a macro to a three-way switch, the Devices panel with a device playing an unknown patch, and a deploy refused for budget, in the style of the [Prism timeline designs](../../designs/prism-timeline/README.md).
- [ ] **Device face ([PRISM-INSTRUMENT-DEVICE]).** Choose a profile in Edit mode; assign macros to fixed controls; refuse kind mismatches with the reason; switch faces with identical Master PCM.
- [ ] **Panel overlay ([PRISM-INSTRUMENT-DEVICE-LABELS]).** Export the SVG overlay; the journey compares it with a golden.
