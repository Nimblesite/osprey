# Prism modular synthesis and VST hosting [PRISM-ROUTING]

The [advanced synthesis delivery plan](advanced-synthesis.md#advanced-synthesis-delivery-plan-prism-advanced-plan) covers neural, granular, physical and combined methods and their Prism interfaces.

The synth package remains UI-free. Prism becomes an editable instrument graph: add oscillators, effects, MIDI generators, modulators and VST3 host nodes from a menu, connect compatible ports, adjust parameters and hear the resulting graph. Existing audio and control work must remain usable while a replacement graph is validated. See [SYNTH-GRAPH], [SYNTH-SIGNALS], [SYNTH-RENDER] and [SYNTH-HOST-VST] in the [synthesis specification](../../specs/synth/Synth.md).

The original `codex/prism-synth-routing` work merged as PR #412. Completion continues on `codex/prism-plan-completion` in the dedicated `~/.codex/worktrees/prism-synth-routing/osprey` worktree, created from the latest local code after fetching `origin/main` on 19 November 2026. The original branch was based on `math-to-code` at `e3ebc1399`; its 03:00 Australia/Melbourne checkpoint on 16 November remains below. Historical measurements describe their recorded checkpoint, not current release readiness.

## Delivery checklist [PRISM-ROUTING-DELIVERY]

- [ ] Publish applied graph/parameter revisions and component activity through Signals without a UI dependency in the package.
- [ ] Exercise native and canvas interfaces with loaded headless and windowed journeys, screenshots and audio evidence; retain cross-platform availability with explicit platform limits.
- [ ] Run static analysis, required CI preparation and coverage checks; coordinate a single validated push and inspect its CI result.
- [ ] Record remaining limitations and remove only this task's temporary processes/artifacts.

## Timeline, track patches and bus routing [PRISM-ROUTING-TIMELINE]

- [ ] Support nested patch racks in track and bus graphs. Exercise the design walkthrough with a second send from an occupied source outlet and a refused zero-delay loop.
- [ ] Open or focus the detached Timeline from Prism, preserving the session, playhead, selected track and undo history across both windows. Compare the rendered states with the design on the FerroUI hosts.

## Acceptance evidence [PRISM-ROUTING-VERIFY]

A graph-edit test must add multiple instances, wire chains and fan-out, reject incompatible ports and illegal cycles, remove/reconnect routes, delete a connected node and verify unchanged snapshots on rejection. Audio tests must distinguish the sounds before and after a routing change, retain oscillator/filter state under parameter-only edits, and exercise block partitions and note-off ownership. UI tests must perform the edits through visible controls, not only construct the final graph directly.

Hosting means loading an existing VST3 binary, not producing a plug-in or rendering a placeholder oscillator. Plug-in discovery and loading never run on the audio callback. Scan failures, unsupported buses and absent binaries remain visible typed errors. Third-party native code is untrusted: process isolation and bounded protocol validation are part of the host design, and arbitrary plug-in safety is not implied by Osprey's guarantees.

Graph-edit verification: the added fourteenth synth journey passes under GC and ARC, including rejected edits, exact route removal, connected-node deletion and PCM comparison showing that bypassing a quarter-gain stage produces four times its output. This proves the pure editing API, not live audio-thread graph replacement or UI integration.

## Integration progress [PRISM-ROUTING-INTEGRATION]

- [ ] Replace the remaining fixed-patch coverage/implementation after keyboard, waveform and modulation interactions have migrated; integrate the continuous audio engine.
- [ ] Add movable node positions, with screenshot and windowed verification.

The current component picker offers the six already rendered reference processors. MIDI generators, modulation processors and VST nodes remain pending and are not advertised as working. The default launch entry opens the modular editor and auditions its actual graph. The VST compatibility source is the [official SDK](https://github.com/steinbergmedia/vst3sdk), including its hosting samples; the selected revision and actual loaded-binary evidence must be recorded before declaring hosting complete.

## VST worker progress [PRISM-ROUTING-VST-WORKER]

`make -C src/toolchain/targets/tests synth-vst` is the repeatable entry point. The ADelay probe measured peak 0.25 from an input impulse; mda JX10 measured peak 0.652497 from MIDI note events. These checks prove real loaded-binary audio processing, not live UI routing, arbitrary plug-in compatibility, process sandboxing or note-release correctness. The seven Prism journeys also pass under ARC. The default-app switch is complete; no full CI push has occurred yet.

VST state/automation evidence: eight worker tests now pass. The instrument regression exposed a processor/controller initialization mismatch (controller reported 0 while saved state restored 1); initial state synchronization and a zero-frame state flush fixed it. Original component bytes and controller values are verified after restoration. Automation offset delivery is checked through the state change, but exact per-sample response and separate controller state persistence remain pending.

## Restore the Prism presentation [PRISM-ROUTING-PRESENTATION]

- [ ] Restore adaptive canvas resizing in the unified editor and verify the updated native window against the original Prism design; fixed-studio tests do not validate the graph editor.
- [ ] Complete sustained audio and full repository CI gates above; these visual/interaction checks do not establish realtime playback reliability.

## Direct cable editing [PRISM-ROUTING-CABLES]

- [ ] Verify actual mouse dragging and cable deletion in the rebuilt window, and capture the windowed regression journey.

### Advanced granular playback checkpoint [PRISM-ROUTING-GRAINS]

The [advanced synthesis plan](advanced-synthesis.md) tracks the new granular implementation separately from these remaining host/release requirements. Native manual interaction added Grain Cloud, replaced the mono output with stereo output, created the actual cable and completed a device audition on 19 November 2026; its receipt reported three empty-queue callbacks. This does not satisfy the sustained zero-starvation requirement or prove speaker output quality. Redundant per-sample grain validation was subsequently moved to checked block boundaries; compilation/static review passed, with comparative runtime evidence still pending. Automated tests remain paused at the user's request.

The previously prepared native Undo/Redo journey edits are preserved in version control without execution: nine added steps cover undoing/reapplying added Delay and Mixer nodes, including an undo screenshot request. JSON syntax is valid; native execution and new screenshot evidence remain outstanding during the testing pause.

### Oscillator MIDI input [PRISM-ROUTING-MIDI-OSCILLATOR]

- [ ] Verify numerical pitch/velocity, note ownership, exact block-boundary gating and long playback when synthesis verification resumes.
- [ ] Verify rendered release endpoints, duration, repeated note-offs and retrigger behaviour; add attack/crossfades and live topology state migration before claiming click-free realtime MIDI instruments.

See [SYNTH-HOST-MIDI-OSCILLATOR]. Manual audition on 19 November 2026 reported four empty-queue callbacks. No Prism automated tests were run, and they are excluded from current work by the user's latest instruction.

Implementation checkpoint [SYNTH-HOST-MIDI-OSCILLATOR-RELEASE]: generated voice state retains release length/remaining frames. `Components.sine` exposes `releaseFrames`, defaulting to 240 frames at 48 kHz; matching note-off latches it, repeated/unrelated note-offs leave the tail intact, and note-on cancels it without resetting oscillator phase. Package strict compilation passed; rendered/audio evidence is still pending under the current testing pause.

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

The expanded app suite passes eleven journeys; the synth package passes seventeen under ARC. The MIDI source emits deterministic sample-positioned events, but the existing mono audition adapter does not yet execute these new node types.

### Hosted graph playback [PRISM-ROUTING-AUDIO-HOSTED]

The macOS windowed and headless patch journeys route a note generator through TAL-NoiseMaker and Dragonfly Room Reverb, edit the controls, verify the stereo tail, and write the captured PCM to a WAV.

### Routed LFO modulation [PRISM-ROUTING-MODULATION]

The twelve loaded app journeys pass under ARC, the seventeen package journeys pass under GC, and strict source checking passes. The current Mac remains locked, preventing windowed screenshot verification; headless native tests execute real workers and audio playback.

The windowed attempt exposed an incorrect rectangle rotation of 12 radians, caused by using the angle argument as a corner radius. A failing scene-shape assertion reproduced it; node cards now use zero rotation, and the full twelve-journey suite plus the rebuilt 42-step headless native journey pass. The windowed retry is tracked separately from this source fix.

### Hosted parameter routing [PRISM-ROUTING-MODULATION-HOSTED]

All twelve app journeys pass under ARC, including frozen graph ownership, stale replies, rejected non-automatable edits and real routing semantics. The windowed retry after the canvas rotation fix fails because no screenshot frame renders while the Mac is locked. Continuous playback, saved patches, final coverage and full CI remain outstanding.

### Package coverage verification [PRISM-ROUTING-VERIFY-COVERAGE]

- [ ] Finish repository-wide CI preparation; package coverage alone does not establish readiness to push.

Evidence is retained under `target/pr-validation/synth-coverage` and `target/pr-validation/synth-coverage-arc`. The app still provides bounded offline auditions; continuous audio and persistent hosted instances remain unfinished.

Repository-wide duplication check: Deslop 0.41.0 exits 3 at 21,235 duplicated lines out of 523,113 (reported 4.1%), above the unchanged 4.0% gate. The report is `target/deslop-report.txt`. Shared rejection assertions now replace three copied configuration-check implementations; all 22 package journeys still pass under ARC, and formatting passes. This small reduction does not establish that the repository gate passes; another complete scan is required after the remaining duplication is addressed.

The full CI, Windows, GPUI, WinUI, release, release-scope and Dependabot workflow definitions have been inspected. Release publishing and staging-branch mutations are outside this branch's validation task. Required PR checks include compiler/runtime builds, static checks, source coverage, browser acceptance, native hosts and mobile/platform suites; `make ci` has not passed. Windows-native execution cannot be proved on this macOS host, and the locked desktop still prevents window screenshots.

### Canvas layout [PRISM-ROUTING-LAYOUT]

- [ ] Verify actual window screenshots; the previously confirmed locked desktop still leaves this unproved.

The UI model is generated from `editor.td`; the synth library remains UI-free. Evidence is in `target/pr-validation/prism-drag-*`. Continuous device rendering, persistent hosted instances, saved patches, full CI and the repository duplication gate remain unfinished.

### Stateful block foundation [SYNTH-RENDER-BLOCKS]

The 22 package journeys pass under ARC. Coverage of the production change passes independently under GC and ARC at 859/926 own-source lines (92.8%, unchanged 92.2% gate); the subsequent stereo test expansion also passes under ARC. Evidence is retained under `target/pr-validation/synth-blocks-*` and `prism-blocks-*`. This is a necessary streaming foundation, not a claim that continuous playback is complete.

### Graph block scheduling [SYNTH-RENDER-BLOCKS-GRAPH]

Evidence is retained under `target/pr-validation/synth-stream-*`. Full CI, the repository duplication gate, windowed verification and continuous device playback remain unfinished.

### Prism pending-request integration [PRISM-ROUTING-AUDIO-PENDING]

Evidence is retained under `target/pr-validation/prism-pending-*`. This removes duplicated controller bookkeeping; it does not establish continuous playback or windowed screenshot verification.

### Persistent text worker sessions [SYNTH-HOST-STREAM-SESSION]

- [ ] Add application-owned session lifetimes, per-request watchdogs and explicit shutdown in the desktop host, then connect continuous graph/device processing.

Evidence is retained under `target/pr-validation/vst-session-*`. The interactive protocol is verified on macOS; Windows/Linux execution, continuous device output, full CI and the duplication gate remain unfinished.

### Desktop worker ownership [SYNTH-HOST-STREAM-SESSION-DESKTOP]

The 101 desktop host tests pass and strict Clippy passes. These are real process-lifecycle tests; the existing VST session suite independently proves retained DSP. They do not yet prove Prism owns a persistent VST instance or continuous device playback. Normal shutdown clears retained children; process isolation is not a sandbox and does not claim ownership of arbitrary plug-in descendants.

### Prism retained graph execution [PRISM-ROUTING-SESSIONS]

- [ ] Feed the retained graph into a continuous audio device queue with live graph/parameter changes.

The loaded application journey now exercises 96 effect exchanges through host completions, rejects stale replies, preserves the frozen graph after a node is deleted, and checks ownership release. All twelve journeys pass under ARC. Native evidence is under `target/pr-validation/prism-session-*`; each transaction is retained in the native journey's recording directory. This verifies actual Prism-to-worker session integration, but the device still plays a bounded rendered WAV through `afplay`.

Session-boundary hardening: a new real-process regression first failed because a NUL-containing reply left its worker registered after native slice publication failed. All transaction failures now stop and remove the worker, including publication failures. All 101 host tests and strict Clippy pass; the bounded pi audit timed out without findings or changes.

### Bounded device queue [SYNTH-DEVICE-QUEUE]

- [ ] Apply live graph edits to the running queue without restarting playback.

Measured native-session spans for the same 256 ms instrument/effect audition were approximately 518 ms at 256-frame graph transfers, 331 ms at 1024 and 274 ms at 2048, including worker startup. The selected producer transfer is 2048 frames; the VST protocol still processes sub-blocks of at most 256 frames with unchanged exact PCM and MIDI/automation offsets. These single-run measurements justify reducing transport overhead; they do not establish reliable realtime deadlines.

The device callback only publishes returned buffers and counters; decoding, allocation, waiting and logging stay on the producer thread. Four preallocated buffers bound queued memory, two transfers prefill before start, and a drain acknowledges all returned frames before cleanup. `emptyCallbacks` counts queue-empty callback observations, not a measured speaker underrun. Device support is macOS-only; other platforms return an explicit unsupported-adapter failure.

### Verified continuous native playback [PRISM-ROUTING-CONTINUOUS]

- [ ] Apply graph/parameter edits transactionally during playback; the current UI explicitly states that edits apply after restarting.
- [ ] Verify the windowed journey; the Mac was rechecked and remains locked.
- [ ] Complete root CI and resolve the root duplication gate before a coordinated push.

The twelve loaded application journeys retain their assertions and add device receipt corruption, live continuation and stop-during-open/block cases. Their test helpers reconstruct the concrete UI program to preserve the compiler's callable-effect provenance, rather than weakening its checks. The package's 22 journeys add all ASCII decoder boundary cases. Native evidence is in `target/pr-validation/prism-device-*`; the original starvation failure remains in `prism-device-live-starvation-red.log`. Full readiness is not claimed: only macOS device output is implemented, live edits require restart, and low-latency scheduling/zero-allocation production remain unfinished.

### Final diagnostics verification [PRISM-ROUTING-DIAGNOSTICS]

- [ ] Resolve root CI: both Deslop 0.42 and installed 0.41 report 4.1% against the unchanged 4.0% gate; make ci stops there.
- [ ] Verify windowed tests after the Mac is unlocked; currently unavailable.

Evidence: `target/pr-validation/prism-diagnostics-*` retains the final failing native logs and device receipt sequences; `device-diagnostics-ctest.log` records worker checks. The package previously passed all 22 journeys in GC/ARC and coverage 919/993 (92.5%, gate 92.2%); GPUI passed 102 tests and strict clippy. No push or PR was made because full CI is not green.

### Producer profile and bounded optimization [PRISM-ROUTING-THROUGHPUT]

This is a committed development checkpoint, not release readiness. Final work remains: sustained producer scheduling/throughput, transactional live edits, state persistence/latency compensation, other platform device adapters, windowed verification and the root CI duplication gate.

### Wrap-up validation [PRISM-ROUTING-WRAP-UP]

The active branch is now `codex/prism-synth-routing` in the primary checkout; the former linked worktree no longer exists. Validation below is against commit `0d0f28b3c`, not the older worktree binaries. Logs are in `target/pr-validation/wrap-up/`.

- [ ] Resolve any reproduced playback failures before claiming smooth sustained playback.

