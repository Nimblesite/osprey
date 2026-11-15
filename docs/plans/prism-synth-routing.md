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
- [ ] Resolve the expanded single test file's 771-line size while retaining its assertions and useful comments; this conflicts with the instruction to expand existing test files rather than create new ones.
- [ ] Finish repository-wide CI preparation; package coverage alone does not establish readiness to push.

Evidence is retained under `target/pr-validation/synth-coverage` and `target/pr-validation/synth-coverage-arc`. The app still provides bounded offline auditions; continuous audio and persistent hosted instances remain unfinished.

Repository-wide duplication check: Deslop 0.41.0 exits 3 at 21,235 duplicated lines out of 523,113 (reported 4.1%), above the unchanged 4.0% gate. The report is `target/deslop-report.txt`. Shared rejection assertions now replace three copied configuration-check implementations; all 22 package journeys still pass under ARC, and formatting passes. This small reduction does not establish that the repository gate passes; another complete scan is required after the remaining duplication is addressed.

The full CI, Windows, GPUI, WinUI, release, release-scope and Dependabot workflow definitions have been inspected. Release publishing and staging-branch mutations are outside this branch's validation task. Required PR checks include compiler/runtime builds, static checks, source coverage, browser acceptance, native hosts and mobile/platform suites; `make ci` has not passed. Windows-native execution cannot be proved on this macOS host, and the locked desktop still prevents window screenshots.


### Canvas layout [PRISM-ROUTING-LAYOUT]

- [x] Reproduce missing drag behavior and deletion-induced position shifts through the existing loaded UI journey.
- [x] Store typed per-node positions separately from the audio graph, preserving grab offsets and moving cards, cables and accessible port bounds together.
- [x] Keep layout-only edits out of the audio revision; clamp dragged cards and port targets to the canvas, reject nonfinite coordinates and release pointer capture.
- [x] Verify all twelve app journeys under GC and ARC, and the rebuilt native host's 44-step real VST/audio journey including a card drag.
- [x] Author placement mathematics in Lean and compare 544 values against generated Osprey under both memory backends.
- [ ] Verify actual window screenshots; the previously confirmed locked desktop still leaves this unproved.

The UI model is generated from `editor.td`; the synth library remains UI-free. Evidence is in `target/pr-validation/prism-drag-*`. Continuous device rendering, persistent hosted instances, saved patches, full CI and the repository duplication gate remain unfinished.


### Stateful block foundation [SYNTH-RENDER-BLOCKS]

- [x] Expose multichannel kernel continuation without introducing parallel DSP implementations; whole-clip rendering delegates to the same path.
- [x] Compare 256-frame execution against 31 + 225 frames for oscillators, LFOs, filters, delays and gain, including a stereo filter with different channel inputs.
- [x] Reject wrong state identities, missing/excess channels, invalid capacities and components that differ from their prepared graph entry; the last case was reproduced as a failing regression before its fix.
- [x] Retain the rebuilt native host's 44-step actual VST/audio journey and strict source checks.
- [ ] Connect retained built-in states and persistent VST workers to graph-wide streaming and the device queue.

The 22 package journeys pass under ARC. Coverage of the production change passes independently under GC and ARC at 859/926 own-source lines (92.8%, unchanged 92.2% gate); the subsequent stereo test expansion also passes under ARC. Evidence is retained under `target/pr-validation/synth-blocks-*` and `prism-blocks-*`. This is a necessary streaming foundation, not a claim that continuous playback is complete.


### Graph block scheduling [SYNTH-RENDER-BLOCKS-GRAPH]

- [x] Share one scheduler between whole clips and stateful graph blocks, retaining independent channel kernels and one absolute sample clock.
- [x] Declare graph state and pending external requests in typeDiagram; a pending request carries its frozen graph, state, exact node and block shape.
- [x] Compare a complete oscillator/filter/gain/delay/mixer/stereo/LFO graph across 31 + 225 frames against an uninterrupted 256-frame reference.
- [x] Verify hosted 64 + 192-frame blocks, note-off at local offset 64, automation at the absolute 128-frame boundary, malformed replies and lost/duplicate state rejection.
- [x] Keep the original clip-window bound while allowing timeline events beyond frame 65,536 with explicit integer-overflow checks.
- [x] Pass all 22 package journeys under ARC and collect GC source coverage: 899/973 lines (92.4%, unchanged 92.2% gate); formatting and strict app source checks pass.
- [ ] Adopt the pending-request state directly in Prism and connect a persistent worker/device transport; current auditions remain bounded.

Evidence is retained under `target/pr-validation/synth-stream-*`. Full CI, the repository duplication gate, windowed verification and continuous device playback remain unfinished.

### Prism pending-request integration [PRISM-ROUTING-AUDIO-PENDING]

- [x] Replace separate render graph, clip-cache and node fields with the library's typed pending request, generated into the editor model through typeDiagram.
- [x] Start graph blocks through `GraphClips.start` and return worker responses through `GraphClips.replied`, preserving the frozen graph and retained kernels across hosted nodes.
- [x] Retain deletion-during-audition and stale-reply assertions, and reject missing/duplicate pending requests explicitly while clearing the render ticket.
- [x] Pass all twelve app journeys under ARC, then the expanded suite under GC; strict checks and formatting pass.
- [x] Rebuild the native host and pass its 44-step real VST/audio journey with exact effect PCM checks.
- [ ] Retain workers across blocks and feed a continuous audio device queue; this change still uses bounded auditions.

Evidence is retained under `target/pr-validation/prism-pending-*`. This removes duplicated controller bookkeeping; it does not establish continuous playback or windowed screenshot verification.

### Persistent text worker sessions [SYNTH-HOST-STREAM-SESSION]

- [x] Add newline-framed bounded text transactions using the existing packet processor and one retained VST instance.
- [x] Apply audio limits per transaction while retaining the plug-in sample clock and DSP state across transactions.
- [x] Require replies before stdin closes; compare exact ADelay continuation and 162 JX10 transactions against a single binary stream, including held-note audio and release to silence.
- [x] Reject blank, malformed, odd-length, oversized and unterminated transactions without publishing a partial response.
- [x] Pass all nine worker checks and the existing 44-step native Prism/VST/audio journey after rebuilding with fatal compiler warnings.
- [ ] Add application-owned session lifetimes, per-request watchdogs and explicit shutdown in the desktop host, then connect continuous graph/device processing.

Evidence is retained under `target/pr-validation/vst-session-*`. The interactive protocol is verified on macOS; Windows/Linux execution, continuous device output, full CI and the duplication gate remain unfinished.


### Desktop worker ownership [SYNTH-HOST-STREAM-SESSION-DESKTOP]

- [x] Add bounded desktop session open/exchange/close capabilities with sixteen owned slots, serial transactions, concurrent pipe draining, deadlines and direct-child cleanup.
- [x] Test real repeated 700 kB transfers, unchanged process identity, input rejection, output overflow, blocked stdin, closed pipes, noisy stderr, public handles, capacity exhaustion and released slots.
- [ ] Adopt these sessions in Prism's audio execution and feed the continuous device queue.

The 101 desktop host tests pass and strict Clippy passes. These are real process-lifecycle tests; the existing VST session suite independently proves retained DSP. They do not yet prove Prism owns a persistent VST instance or continuous device playback. Normal shutdown clears retained children; process isolation is not a sandbox and does not claim ownership of arbitrary plug-in descendants.


### Prism retained graph execution [PRISM-ROUTING-SESSIONS]

- [x] Replace one-shot worker invocation with typed application-owned session handles and 256-frame graph blocks.
- [x] Preserve independent built-in channel states, absolute MIDI/modulation timing, frozen graph bindings and one live VST instance per node for the complete audition.
- [x] Close retained sessions on completion/failure and keep cleanup errors visible without hiding the original failure.
- [x] Run the 44-step native headless journey against real instrument/effect binaries: three distinct worker sessions each process 48 requests, with exact chained PCM and modulation checks.
- [ ] Feed the retained graph into a continuous audio device queue with live graph/parameter changes.

The loaded application journey now exercises 96 effect exchanges through host completions, rejects stale replies, preserves the frozen graph after a node is deleted, and checks ownership release. All twelve journeys pass under ARC. Native evidence is under `target/pr-validation/prism-session-*`; each transaction is retained in the native journey's recording directory. This verifies actual Prism-to-worker session integration, but the device still plays a bounded rendered WAV through `afplay`.

Session-boundary hardening: a new real-process regression first failed because a NUL-containing reply left its worker registered after native slice publication failed. All transaction failures now stop and remove the worker, including publication failures. All 101 host tests and strict Clippy pass; the bounded pi audit timed out without findings or changes.


### Bounded device queue [SYNTH-DEVICE-QUEUE]

- [x] Add a macOS AudioQueue worker with four preallocated buffers, atomic callback ownership, bounded producer waits and explicit drain.
- [x] Validate canonical 48 kHz mono/stereo float32 WAV blocks, exact lengths, finite samples and output amplitudes within -1..1 before enqueueing.
- [x] Exercise the actual device with ordered buffers and drain; reject malformed, over-capacity, nonfinite, excessive-amplitude and unsupported-layout packets.
- [ ] Connect Prism to the queue and verify continuous rendering, stop ownership and live edits.

Measured native-session spans for the same 256 ms instrument/effect audition were approximately 518 ms at 256-frame graph transfers, 331 ms at 1024 and 274 ms at 2048, including worker startup. The selected producer transfer is 2048 frames; the VST protocol still processes sub-blocks of at most 256 frames with unchanged exact PCM and MIDI/automation offsets. These single-run measurements justify reducing transport overhead; they do not establish reliable realtime deadlines.

The device callback only publishes returned buffers and counters; decoding, allocation, waiting and logging stay on the producer thread. Four preallocated buffers bound queued memory, two transfers prefill before start, and a drain acknowledges all returned frames before cleanup. `emptyCallbacks` counts queue-empty callback observations, not a measured speaker underrun. Device support is macOS-only; other platforms return an explicit unsupported-adapter failure.


### Verified continuous native playback [PRISM-ROUTING-CONTINUOUS]

- [x] Add Play/Stop, typed pending device state, checked queue acknowledgments and reusable bounded audio slices.
- [x] Preserve real VST and built-in DSP instances across the audition boundary; drain auditions and dispose handles on Stop/failure/shutdown, including late open completions.
- [x] Extend the headless driver with bounded deferred completion batches so continuous work cannot prevent the Stop event.
- [x] Keep the sustained queue-starvation test red until production throughput improves: disable per-character stdout flushing and replace per-digit PCM string allocation/search with direct ASCII decoding and static encoding literals.
- [x] Pass real native 44-step audition and 43-step sustained journeys: 985088 frames, zero empty-queue callbacks, exact VST-chain PCM/MIDI/modulation, and no owned worker left alive after Stop.
- [ ] Apply graph/parameter edits transactionally during playback; the current UI explicitly states that edits apply after restarting.
- [ ] Verify the windowed journey; the Mac was rechecked and remains locked.
- [ ] Complete root CI and resolve the root duplication gate before a coordinated push.

The twelve loaded application journeys retain their assertions and add device receipt corruption, live continuation and stop-during-open/block cases. Their test helpers reconstruct the concrete UI program to preserve the compiler's callable-effect provenance, rather than weakening its checks. The package's 22 journeys add all ASCII decoder boundary cases. Native evidence is in `target/pr-validation/prism-device-*`; the original starvation failure remains in `prism-device-live-starvation-red.log`. Full readiness is not claimed: only macOS device output is implemented, live edits require restart, and low-latency scheduling/zero-allocation production remain unfinished.
