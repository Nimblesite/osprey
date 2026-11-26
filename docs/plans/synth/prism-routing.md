# Prism modular synthesis and VST hosting [PRISM-ROUTING]

The [advanced synthesis delivery plan](prism-advanced.md#advanced-synthesis-delivery-plan-prism-advanced-plan) covers neural, granular, physical and combined methods and their Prism interfaces.

The synth package remains UI-free. Prism becomes an editable instrument graph: add oscillators, effects, MIDI generators, modulators and VST3 host nodes from a menu, connect compatible ports, adjust parameters and hear the resulting graph. Existing audio and control work must remain usable while a replacement graph is validated. See [SYNTH-GRAPH], [SYNTH-SIGNALS], [SYNTH-RENDER] and [SYNTH-HOST-VST] in the [synthesis specification](../../specs/synth/Synthesis.md).

The original `codex/prism-synth-routing` work merged as PR #412. Completion continues on `codex/prism-plan-completion` in the dedicated `~/.codex/worktrees/prism-synth-routing/osprey` worktree, created from the latest local code after fetching `origin/main` on 19 November 2026. The original branch was based on `math-to-code` at `e3ebc1399`; its 03:00 Australia/Melbourne checkpoint on 16 November remains below. Historical measurements describe their recorded checkpoint, not current release readiness.

## Delivery checklist [PRISM-ROUTING-DELIVERY]

- [x] Create and attach the requested worktree without moving unrelated Inbox edits.
- [x] Establish the synth package baseline: thirteen existing loaded journeys pass.
- [x] Implement transactional graph editing, cable removal, component removal and checked reconnection; rejected edits preserve the previous playable graph.
- [x] Expand typed component descriptors and rendering for MIDI generators, modulators and effects; declare new models in typeDiagram and derive their representations.
- [x] Replace Prism's fixed patch mapping with the synth graph, with a component menu, selectable nodes, compatible cable endpoints and parameter inspection.
- [ ] Publish applied graph/parameter revisions and component activity through Signals without a UI dependency in the package.
- [x] Implement actual VST3 discovery, out-of-process scanning, activation, processing, MIDI/automation, state save/restore and disposal as an external graph node. The worker session can snapshot both states and restore them in a fresh process by class ID.
- [x] Verify an SDK instrument and effect with real audio, MIDI, parameter and state round trips; surface unavailable or failed plug-ins explicitly. ADelay and mda JX10 pass independent-process restore tests, malformed-state rejection and real processing.
- [ ] Exercise native and canvas interfaces with loaded headless and windowed journeys, screenshots and audio evidence; retain cross-platform availability with explicit platform limits.
- [ ] Run static analysis, required CI preparation and coverage checks; coordinate a single validated push and inspect its CI result.
- [ ] Record remaining limitations and remove only this task's temporary processes/artifacts.

## Timeline, track patches and bus routing [PRISM-ROUTING-TIMELINE]

- [x] Register the [six design screenshots and interaction contract](../../designs/prism-timeline/README.md) and the [typed routing model](../../designs/prism-timeline/routing.td) for the combined Prism DAW.
- [x] Open a timeline track in Prism with its notes, gain, pan, output and Space send in the same arrangement. The track-opening journey exercises built-in and patched voices, and the project JSON journey reopens their mix and timeline.
- [x] Create, cancel, edit and remove independent sends from internal mono and stereo outlets to named groups through the native Routing menu. The host journey verifies the complete saved project, and the audio journey sums two pre-fader taps from one note evaluation. [DAW-MIX-BUS-TAP]
- [x] Browse every internal send from the Mix graph or Routing menu. The native host opens and edits the fourth of four sends on a single outlet, verifies both pages, cancels a draft and checks that removing one tap retains all siblings. [DAW-MIX-BUS-TAP]
- [x] Route group outputs through other groups to Master. Validate missing destinations and cycles before saving. Both declaration orders produce the same stereo samples, and muting the downstream group silences the path. [DAW-MIX-BUS-OUTPUT]
- [x] Remove the per-patch component and cable counts. The synth schedules 2,052 components and 2,051 cables under GC and ARC, and a saved project with 256 unity gains renders sample-for-sample like a direct connection. Dependent feedback and per-block memory limits remain explicit. [SYNTH-GRAPH]
- [ ] Support nested patch racks in track and bus graphs. Exercise the design walkthrough with a second send from an occupied source outlet and a refused zero-delay loop.
- [ ] Open or focus the detached Timeline from Prism, preserving the session, playhead, selected track and undo history across both windows. Compare the rendered states with the design on the FerroUI hosts.

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
- [x] Add parameter automation, controller/component state round trips and restart notifications. Changing the JX10 program triggers a real controller restart notification. Unsupported topology changes stop the worker with an explicit diagnostic.
- [x] Add bounded streaming transport and connect external processor instances to synth graph ports and parameter Signals. The synth and native Prism journeys check the worker packets, modulation points and device output.
- [x] Drive VST discovery/creation and actual playback from Prism's component menu.

`make -C src/toolchain/targets/tests synth-vst` is the repeatable entry point. The ADelay probe measured peak 0.25 from an input impulse; mda JX10 measured peak 0.652497 from MIDI note events. These checks prove real loaded-binary audio processing, not live UI routing, arbitrary plug-in compatibility, process sandboxing or note-release correctness. The seven Prism journeys also pass under ARC. The default-app switch is complete; no full CI push has occurred yet.

VST state/automation evidence: eight worker tests now pass. The instrument regression exposed a processor/controller initialization mismatch (controller reported 0 while saved state restored 1); initial state synchronization and a zero-frame state flush fixed it. Original component bytes and controller values are verified after restoration. Automation offset delivery is checked through the state change, but exact per-sample response and separate controller state persistence remain pending.

## Restore the Prism presentation [PRISM-ROUTING-PRESENTATION]

- [x] Replace the inspector's repeated module forms with a Module picker, selected parameters/connections, compact transport and destructive removal. The new inspector regression failed before implementation and passes unchanged; all fourteen synth journeys pass after explicit-selection adaptation (2026-11-16). The final scoped spacing and routing layout were verified in the rebuilt native window.
- [x] Replace GPUI picker chips with a labelled dropdown and preserve selected-key dispatch; use real themed buttons so focus, accessibility and icons survive styling. The host release check passes; empty menus are disabled and empty Parameters sections omitted.
- [x] Exercise the packaged inspector through native input: Module dropdown changes LFO to Filter parameters; Modules adds Mixer; keyboard closes and reopens; direct pointer clicks on Oscillator output and Mixer input create a visible cable, and Disconnect first removes it (2026-11-16).
- [x] Accept the final stacked routing actions and omitted empty section footers visually: rebuilt and relaunched Prism on 2026-11-16, opened Filter inspector, and verified both complete routing labels fit inside the panel with section spacing and no unknown-symbol diagnostic. Decorative canvas ports and further cable gestures remain a separate active canvas task.


- [x] Restore the original signal-flow studio as the normal app launch, including node cards, inspector and keyboard.
- [x] Preserve the modular editor and VST implementation behind an explicit routing launch; do not discard that work.
- [x] Verify the normal launch through the compiled boundary and capture the restored window: the four-step launch regression and the fourteen-step headless/windowed interaction journey pass; captures are under `target/pr-validation/wrap-up/restored-window/`. The explicit routing launch also passes 41 real-worker scan, parameter, cable, drag and removal steps (audio timing excluded from this UI check).
- [x] Replace the temporary two-workspace launch with one editable graph rendered using the original Prism atlas and machine layout; remove the separate routing target.
- [x] Preserve module insertion by palette drag, independent node movement, typed cable editing, VST discovery/instances and native parameter controls.
- [x] Bind original cutoff/master sliders, oscillator waveform, enable switches, LFO rate and keyboard to the graph. The loaded regression compares actual PCM for waveform changes and bypass silence.
- [x] Pass 13 application journeys, the 19-step original-controls/module-drag journey in headless and windowed modes, and the 42-step real VST editing journey in both modes. Evidence: `target/pr-validation/wrap-up/main-art-tests.log`, `integrated-controls-*`, `integrated-routing-*`.
- [x] Preserve the exact main-branch Prism artwork and logo; inspect `design/prism-audio-modulation.png` and captures of the packaged app, including newly added module cards.
- [x] Restore graph undo/redo in the unified editor using the original Prism header controls. Typed snapshots retain graph, positions, selection, note and plug-in bindings; node identities remain monotonic, new edits discard redo, and one drag is one edit. The new loaded graph-history journey failed before the fix and passes under ARC and GC; all 17 app journeys and strict static analysis pass (2026-11-19). Evidence: `target/pr-validation/plan-completion/history-{red,gc,final-suite,final-check}.log`.
- [x] Align original Undo/Redo hit areas and custom inspector content with the expanding canvas. The graph resize journey first reproduced x=1108 instead of 1408 for Undo and x=1264 instead of 1564 for rendered connection text, then passed; all 18 app journeys and strict static analysis pass (2026-11-19). Evidence: `target/pr-validation/plan-completion/resize-{red,inspector-red-detail,suite,final-check}.log`.
- [ ] Restore adaptive canvas resizing in the unified editor and verify the updated native window against the original Prism design; fixed-studio tests do not validate the graph editor.
- [ ] Complete sustained audio and full repository CI gates above; these visual/interaction checks do not establish realtime playback reliability.

## Direct cable editing [PRISM-ROUTING-CABLES]

- [x] Add output-to-input pointer capture, preview, typed validation and release-to-connect while retaining click-to-connect.
- [x] Share cable geometry between painting, selection hit testing and accessible actions; expose a contextual Delete cable control and Backspace/Delete/Escape handling.
- [x] Back the original Filter cutoff socket with an optional normalized input and its monitor socket with the actual audio output, preserving the original artwork.
- [x] Pass strict application static analysis and the 55-step compiled native headless journey, including module insertion, routing, cable deletion and keyboard docking.
- [x] Pass the expanded application/audio regressions: synth package 22/22 and app 15/15 (2026-11-16). GPUI host clippy still runs only in CI.
- [x] Measure the audio the app actually writes to the device: a `Speaker` effect captures every float32 WAV block from Audition, then the test checks the header, pitch from zero crossings (110 Hz–7 kHz within 0.05 Hz), RMS, peak, block-boundary continuity, 0.5 gain and low-pass attenuation (4 kHz through a 400 Hz cutoff vs. 19 kHz). A 1% pitch error fails it.
- [ ] Verify actual mouse dragging and cable deletion in the rebuilt window, and capture the windowed regression journey.

### Advanced granular playback checkpoint [PRISM-ROUTING-GRAINS]

The [advanced synthesis plan](advanced-synthesis.md) tracks the new granular implementation separately from these remaining host/release requirements. Native manual interaction added Grain Cloud, replaced the mono output with stereo output, created the actual cable and completed a device audition on 19 November 2026; its receipt reported three empty-queue callbacks. This does not satisfy the sustained zero-starvation requirement or prove speaker output quality. Redundant per-sample grain validation was subsequently moved to checked block boundaries; compilation/static review passed, with comparative runtime evidence still pending. Automated tests remain paused at the user's request.

The previously prepared native Undo/Redo journey edits are preserved in version control without execution: nine added steps cover undoing/reapplying added Delay and Mixer nodes, including an undo screenshot request. JSON syntax is valid; native execution and new screenshot evidence remain outstanding during the testing pause.

### Oscillator MIDI input [PRISM-ROUTING-MIDI-OSCILLATOR]

- [x] Declare a typed optional MIDI event input on the built-in oscillator.
- [x] Apply sample-offset note-on/off, pitch and velocity in the shared renderer with retained monophonic note ownership.
- [x] Expose and manually connect the input in the original Prism card design; compile the native application and complete a device audition.
- [ ] Verify numerical pitch/velocity, note ownership, exact block-boundary gating and long playback when synthesis verification resumes.
- [x] Add configurable MIDI note-off release shaping with retained phase, exact endpoints and note ownership across blocks.
- [ ] Verify rendered release endpoints, duration, repeated note-offs and retrigger behaviour; add attack/crossfades and live topology state migration before claiming click-free realtime MIDI instruments.

See [SYNTH-HOST-MIDI-OSCILLATOR]. Manual audition on 19 November 2026 reported four empty-queue callbacks. No Prism automated tests were run, and they are excluded from current work by the user's latest instruction.

Implementation checkpoint [SYNTH-HOST-MIDI-OSCILLATOR-RELEASE]: generated voice state retains release length/remaining frames. `Components.sine` exposes `releaseFrames`, defaulting to 240 frames at 48 kHz; matching note-off latches it, repeated/unrelated note-offs leave the tail intact, and note-on cancels it without resetting oscillator phase. Package strict compilation passed; rendered/audio evidence is still pending under the current testing pause.
