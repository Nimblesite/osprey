# Prism modular synthesis and VST hosting [PRISM-ROUTING]

The synth package remains UI-free. Prism becomes an editable instrument graph: add oscillators, effects, MIDI generators, modulators and VST3 host nodes from a menu, connect compatible ports, adjust parameters and hear the resulting graph. Existing audio and control work must remain usable while a replacement graph is validated. See [SYNTH-GRAPH], [SYNTH-SIGNALS], [SYNTH-RENDER] and [SYNTH-HOST-VST] in the [synthesis specification](../specs/runtime/Synthesis.md).

Work is isolated on `codex/prism-synth-routing`, based on `math-to-code` at `e3ebc1399`. The requested stopping time is 03:00 Australia/Melbourne on 16 November 2026; stop with an accurate handoff even if verification remains incomplete.

## Delivery checklist [PRISM-ROUTING-DELIVERY]

- [x] Create and attach the requested worktree without moving unrelated Inbox edits.
- [x] Establish the synth package baseline: thirteen existing loaded journeys pass.
- [x] Implement transactional graph editing, cable removal, component removal and checked reconnection; rejected edits preserve the previous playable graph.
- [ ] Expand typed component descriptors and rendering for MIDI generators, modulators and effects; declare new models in typeDiagram and derive their representations.
- [ ] Replace Prism's fixed patch mapping with the synth graph, with a component menu, selectable nodes, compatible cable endpoints and parameter inspection.
- [ ] Publish applied graph/parameter revisions and component activity through Signals without a UI dependency in the package.
- [ ] Implement actual VST3 discovery, out-of-process scanning, activation, processing, MIDI/automation, state save/restore and disposal as an external graph node.
- [ ] Verify an SDK instrument and effect with real audio, MIDI, parameter and state round trips; surface unavailable or failed plug-ins explicitly.
- [ ] Exercise native and canvas interfaces with loaded headless and windowed journeys, screenshots and audio evidence; retain cross-platform availability with explicit platform limits.
- [ ] Run static analysis, required CI preparation and coverage checks; coordinate a single validated push and inspect its CI result.
- [ ] Record remaining limitations and remove only this task's temporary processes/artifacts.

## Acceptance evidence [PRISM-ROUTING-VERIFY]

A graph-edit test must add multiple instances, wire chains and fan-out, reject incompatible ports and illegal cycles, remove/reconnect routes, delete a connected node and verify unchanged snapshots on rejection. Audio tests must distinguish the sounds before and after a routing change, retain oscillator/filter state under parameter-only edits, and exercise block partitions and note-off ownership. UI tests must perform the edits through visible controls, not only construct the final graph directly.

Hosting means loading an existing VST3 binary, not producing a plug-in or rendering a placeholder oscillator. Plug-in discovery and loading never run on the audio callback. Scan failures, unsupported buses and absent binaries remain visible typed errors. Third-party native code is untrusted: process isolation and bounded protocol validation are part of the host design, and arbitrary plug-in safety is not implied by Osprey's guarantees.

Graph-edit verification: the added fourteenth synth journey passes under GC and ARC, including rejected edits, exact route removal, connected-node deletion and PCM comparison showing that bypassing a quarter-gain stage produces four times its output. This proves the pure editing API, not live audio-thread graph replacement or UI integration.

## Integration progress [PRISM-ROUTING-INTEGRATION]

- [x] Reference graph replacement preserves compatible DSP state, parameters and sample position while advancing the applied revision; fifteen synth journeys pass on GC and ARC.
- [x] Declare editor state and actions in `editor.td` and generate the Osprey model with the upstream converter.
- [x] Build a native component picker, per-port cable controls and parameter fields alongside a canvas drawn from the same graph Signal.
- [x] Drive the editor through real host events: create oscillator/gain/output nodes, connect their ports, change frequency, reject invalid text and remove a connected component. All seven Prism journeys pass on GC, including the five existing playback/skin journeys.
- [x] Make the modular surface Prism's default and audition its actual graph through a checked float32 preview.
- [ ] Replace the remaining fixed-patch coverage/implementation after keyboard, waveform and modulation interactions have migrated; integrate the continuous audio engine.
- [x] Add pointer-based canvas port routing and node selection using the same checked editor commands, with accessible port bounds and cycle-rejection regression coverage.
- [ ] Add movable node positions, with screenshot and windowed verification.

The current component picker offers the six already rendered reference processors. MIDI generators, modulation processors and VST nodes remain pending and are not advertised as working. The default launch entry opens the modular editor and auditions its actual graph. The VST compatibility source is the [official SDK](https://github.com/steinbergmedia/vst3sdk), including its hosting samples; the selected revision and actual loaded-binary evidence must be recorded before declaring hosting complete.

## VST worker progress [PRISM-ROUTING-VST-WORKER]

- [x] Pin and build the official SDK and a separate native scanning/processing worker without installing user plug-ins.
- [x] Scan real ADelay and mda modules by stable VST3 class IDs.
- [x] Initialize, activate and process a real effect and instrument, including note-on/off delivery, finite-output checks, teardown and unload; six bounded worker tests pass on macOS arm64.
- [ ] Add parameter automation, controller/component state round trips and restart notifications.
- [ ] Add bounded streaming transport and connect external processor instances to synth graph ports and parameter Signals.
- [ ] Drive VST discovery/creation and actual playback from Prism's component menu.

`make -C src/toolchain/targets/tests synth-vst` is the repeatable entry point. The ADelay probe measured peak 0.25 from an input impulse; mda JX10 measured peak 0.652497 from MIDI note events. These checks prove real loaded-binary audio processing, not live UI routing, arbitrary plug-in compatibility, process sandboxing or note-release correctness. The seven Prism journeys also pass under ARC. The default-app switch is complete; no full CI push has occurred yet.

VST state/automation evidence: eight worker tests now pass. The instrument regression exposed a processor/controller initialization mismatch (controller reported 0 while saved state restored 1); initial state synchronization and a zero-frame state flush fixed it. Original component bytes and controller values are verified after restoration. Automation offset delivery is checked through the state change, but exact per-sample response and separate controller state persistence remain pending.

## Preview and native boundary evidence [PRISM-ROUTING-AUDIO]

The default entry now opens the modular graph. The native archive builds with warnings treated as errors, the desktop host builds, and the eleven-step headless native-boundary journey passes. That host smoke driver supplies generic capability completions; its success is not used as proof of sound. Separately, the exact Osprey-produced playback request was executed successfully against the real device adapter and emitted `audio.preview` for 12,288 frames. An independent RIFF decoder verified mono float32, 48 kHz, finite samples, peak 0.249982357 and RMS 0.176679798 for the starting graph. Sixteen package journeys pass under GC/ARC and seven app journeys pass, including explicit playback-failure handling.

The real-window test was attempted but could not render a frame: computer-use inventory confirmed the Mac is locked. Its owned process exited; screenshots and layout verification remain outstanding. Do not treat the native smoke result as visual verification. The macOS bundle uses the existing Prism icon; the worktree's LFS icon was materialized before bundling.

Canvas interaction evidence: the loaded application journey first failed because pointer events were ignored, then caught fractional row placement in the existing grid. Ports and cable endpoints now share geometry; host pointer events select nodes, connect ports and reject feedback without changing the previous patch. Accessibility bounds are checked against that geometry.

Persistent worker transport now retains one actual plug-in across bounded variable-size blocks, receives MIDI and automation, and returns interleaved PCM and latency. Nine worker journeys pass, including independent exact PCM across an effect block boundary, held/released instrument controls and malformed input rejection. This is offline worker transport; graph integration, realtime device scheduling, host watchdogs, controller restart notifications and cross-platform execution remain outstanding.

Descriptor evidence: `describe` activates the actual module and reports checked channel counts and normalized controller values. The library's generated `HostDescriptor` becomes an ordinary external graph node with audio/MIDI ports and independently identified parameters. Seventeen package journeys pass under GC and ARC; the loaded-worker journey also checks ADelay's actual stereo layout and Delay parameter. The descriptor does not imply those external nodes are connected to Prism playback yet.

Desktop process receipts distinguish delivery success from child exit status. A regression using the GPUI host's actual `{status, stderr}` receipt first exposed a false playback success; Prism now rejects nonzero/malformed receipts and shares that decoder with forthcoming worker commands. Seven app journeys pass on GC/ARC and the rebuilt native host passes the eleven-step headless journey using correctly shaped simulated receipts. The descriptor's channel-count fields have explicit names (`inputChannels`, `outputChannels`); both package and complete app compile/test paths are verified after adding the new models.


VST menu evidence: eight loaded application journeys pass under GC/ARC. They scan through a native path field, create independently identified plug-in instances, edit normalized parameters, wire stereo ports, remove instances and reject stale/failed/malformed completions. A rebuilt native headless host also completes a 17-step journey using actual ADelay worker processes (`--live-processes`), recording the routed and removed accessibility states. The worker and default path configuration are packaged beside the app. These checks prove discovery and UI graph construction; external-node playback remains pending.

Audio transport preparation: the GPUI desktop now supports bounded `process.input` requests using host-held stdin slices. Tests exercise payloads larger than pipe capacity, input limits, stdout capture and a child that closes output pipes before hanging. That last case exposed and fixed a real deadline escape. The VST worker's bounded `stream-text` mode has exact-PCM parity with binary streaming; it uses the same processing implementation. These are transport checks, not evidence that Prism's external graph audio is integrated yet.


The UI-free package now owns checked worker packet encoding/decoding and interleaved float32 WAV output. Independent byte-oracle tests cover negative samples, stereo frame counts, MIDI crossing a 256-frame boundary, malformed/trailing replies and invalid input shapes. Seventeen package journeys and nine application journeys pass under ARC; the real worker suite also verifies its 1,024-point parameter limit. External-node graph rendering is the next integration step.

### MIDI and stereo components [PRISM-ROUTING-MIDI]

- [x] Add a native-menu MIDI note generator with pitch, velocity and exact gate-frame parameters, explicit mono-to-stereo conversion, and a stereo output.
- [x] Verify note-on/off ownership at frame 256, whole-clip versus partitioned events, invalid fractional pitch/gate rejection, and incompatible MIDI/audio routing through the UI.
- [x] Connect these sources to the graph audio execution path and verify an audible VST instrument/effect chain.

The expanded app suite passes eleven journeys; the synth package passes seventeen under ARC. The MIDI source emits deterministic sample-positioned events, but the existing mono audition adapter does not yet execute these new node types.

### Hosted graph playback [PRISM-ROUTING-AUDIO-HOSTED]

- [x] Execute MIDI → real mda JX10 → real ADelay → real ADelay → stereo playback through native UI controls and bounded workers.
- [x] Compare captured effect PCM exactly against its upstream buffer with the SDK delay's documented one-sample minimum; confirm note-on at frame zero and note-off at frame 6000.
- [x] Share built-in kernels with the mono reference renderer and verify exact PCM equality, malformed-cache rejection, frozen graph/binding ownership and stale completion handling.
- [ ] Implement continuous device rendering, persistent hosted instances, live modulation, graph persistence and latency compensation.

The native headless journey now runs 36 steps with real processes and playback. The host-free app suite has twelve loaded journeys. The graph clip renderer is deliberately bounded and offline; feedback stays in the reference renderer, and nonzero plug-in latency is reported explicitly. Windowed screenshot evidence and full CI remain outstanding.

### Routed LFO modulation [PRISM-ROUTING-MODULATION]

- [x] Add an independent normalized sine LFO and stereo controlled-gain component to the native menu and canvas graph.
- [x] Share oscillator kernels, author transfer formulas in Lean, and broadcast control samples without implicitly changing audio channel widths.
- [x] Verify exact stereo PCM for constant control, changing PCM for moving control, missing-input rejection and invalid-control rejection.
- [x] Extend the real native VST journey to 42 steps including an LFO-controlled stereo gain before playback; retain exact upstream VST PCM checks.
- [x] Route normalized modulators into automatable hosted parameters.
- [ ] Implement continuous rendering.

The twelve loaded app journeys pass under ARC, the seventeen package journeys pass under GC, and strict source checking passes. The current Mac remains locked, preventing windowed screenshot verification; headless native tests execute real workers and audio playback.

The windowed attempt exposed an incorrect rectangle rotation of 12 radians, caused by using the angle argument as a corner radius. A failing scene-shape assertion reproduced it; node cards now use zero rotation, and the full twelve-journey suite plus the rebuilt 42-step headless native journey pass. The windowed retry is tracked separately from this source fix.

### Hosted parameter routing [PRISM-ROUTING-MODULATION-HOSTED]

- [x] Add native parameter modulation actions that create ordinary typed canvas cables, with incompatible-source rejection and disconnect-to-default behavior.
- [x] Encode checked automation points at exact 128-frame intervals; cap clip automation and reject unavailable controls explicitly.
- [x] Verify the real JX10 filter receives 96 modulation points, with changed PCM compared to a stationary-control replay; the complete native headless journey now has 43 steps.

All twelve app journeys pass under ARC, including frozen graph ownership, stale replies, rejected non-automatable edits and real routing semantics. The windowed retry after the canvas rotation fix fails because no screenshot frame renders while the Mac is locked. Continuous playback, saved patches, final coverage and full CI remain outstanding.

### Package coverage verification [PRISM-ROUTING-VERIFY-COVERAGE]

- [x] Expand the existing package suite to 22 loaded journeys, covering stereo kernel chains, MIDI window boundaries, hosted modulation, native packet round trips, malformed buffers and invalid public graph records.
- [x] Run coverage independently under GC and ARC: both report 848/913 own source lines (92.9%), exceeding the unchanged 92.2% package gate.
- [x] Check formatting and compile the expanded suite with warnings treated as errors.
- [ ] Resolve the expanded single test file's 655-line size while retaining its assertions and useful comments; this conflicts with the instruction to expand existing test files rather than create new ones.
- [ ] Finish repository-wide CI preparation; package coverage alone does not establish readiness to push.

Evidence is retained under `target/pr-validation/synth-coverage` and `target/pr-validation/synth-coverage-arc`. The app still provides bounded offline auditions; continuous audio and persistent hosted instances remain unfinished.

Repository-wide duplication check: Deslop 0.41.0 exits 3 at 21,235 duplicated lines out of 523,113 (reported 4.1%), above the unchanged 4.0% gate. The report is `target/deslop-report.txt`. Shared rejection assertions now replace three copied configuration-check implementations; all 22 package journeys still pass under ARC, and formatting passes. This small reduction does not establish that the repository gate passes; another complete scan is required after the remaining duplication is addressed.

The full CI, Windows, GPUI, WinUI, release, release-scope and Dependabot workflow definitions have been inspected. Release publishing and staging-branch mutations are outside this branch's validation task. Required PR checks include compiler/runtime builds, static checks, source coverage, browser acceptance, native hosts and mobile/platform suites; `make ci` has not passed. Windows-native execution cannot be proved on this macOS host, and the locked desktop still prevents window screenshots.
