# The daw: delivery plan [DAW-PLAN]

Perception Studio must play a project in real time at 1.5× or better, lay its pages out to any window without resizing its controls, and carry the Pocket Orchestra ensemble app beside it. Real-time play is the first blocker: the user's verdict is that without it the app is useless, and it comes before every other item here. The daw runs on the FerroUI host (`make -C src/toolchain/targets/macos prism`); its GPUI fallback is deleted below and no GPUI work is planned.

The [daw specification](../../specs/synth/Perception.md#perception-studio-the-daw-daw) and [Pocket Orchestra](../../specs/synth/Perception.md#pocket-orchestra-orchestra) define the requirements within the [scope](../../specs/synth/Perception.md#scope-daw-scope); the [Timeline designs](../../designs/prism-timeline/README.md) are the visual reference. Checks: `make -C src/toolchain/targets/macos/tests daw-playback-rate`, `daw-profile`, `daw-events-headless`, `daw-video`; `make -C src/toolchain/targets/tests synth`, `synth-vst`, `pocket-orchestra`.

## Real-time play [DAW-TRANSPORT-LIVE]

- [ ] **Keep Elise Unbound supplied through the first 16 beats**: `make -C src/toolchain/targets/macos/tests daw-playback-rate DAW_RATE_SONG=src/osprey/examples/daw/tracks/elise-unbound.json DAW_RATE_BEATS=16` reports zero deadline misses and at least 1.5× on ARC and GC (today 0.71× and 0.22×; 909 of 1,538 main-thread samples sit under `SpectralKernels::streamed`) [DAW-TRANSPORT-LIVE-SPEED].
- [ ] **Keep the device queue supplied through 48 beats of Elise**, counting empty callbacks as well as returned frames (`daw-playback-rate DAW_RATE_BEATS=48`) [DAW-TRANSPORT-LIVE-SPEED].
- [ ] **Engine at 1.5× real time or better on the codex song on one core** (today 0.37× under load, mostly reference counting and allocation; owners: synth kernels and delay lines, stream encoding, block loop, code generation) [DAW-TRANSPORT-LIVE-SPEED].
- [ ] **No cold Elise dropouts or underruns, and a shorter cold start**: beats 8–24 with no late note or empty callback (today two Throat morph notes 159 and 319 ms late, and two empty callbacks after a 4.27 s cold start; none after 89.5 ms warm) [DAW-TRANSPORT-LIVE-SPEED].
- [ ] **The 1.5× single-core deadline met on cold codex, whole song** (today 0.58×, longest packet gap 1.24 s) [DAW-TRANSPORT-LIVE-SPEED].
- [ ] **Whole-song and spectral deadline gaps closed**: `DAW_RATE_BEATS=161` codex at 1.5× (today 0.44×, longest gap 2.95 s), Elise at beat 13.89 (today 0.27×), and the four-note-maker two-beat simulation keeps a simulated 48 kHz queue supplied for codex, Elise Motif and Silt Relay [DAW-TRANSPORT-LIVE-SPEED].
- [ ] **Start latency of 5 to 10 ms**: wall-clock time from Play to first sound, including project and device startup, measured (today 320 buffered frames and the first packet after 2.4 s) [DAW-TRANSPORT-LIVE-LATENCY].
- [ ] **A cold song never starts rough while its notes are still made** (one maker per three cores at the lowest priority, ended with the Play or the next song; `pkill -f "daw .* warm"` ends stale ones); heard by the user on each song [DAW-TRANSPORT-LIVE-SPEED].
- [ ] **MIDI clock sync**: timing clock, Start, Continue, Stop and Song Position Pointer from the audio sample clock, following an external clock when external sync is on; MIDI ports in the host (CoreMIDI, Windows MIDI Services), the clock source and ports in `daw.td`, scheduling against the device's played frames. Nothing is built [DAW-TRANSPORT-MIDI-CLOCK].
- [ ] A binary block replaces the hex text between the renderer and the device (a byte-writing output in the language or the runtime) [DAW-TRANSPORT-LIVE].
- [ ] **A disconnected patch branch stays playable**: reproduce the edited codex `sub` track with its mixer-to-gain cable removed, verify silence on that path and audio from the other tracks, reconnect and hear the bass [DAW-PATCH-HEARD].
- [ ] Find how the sub patch lost its Mixer→Gain cable during the user's drag, and add the regression [DAW-PATCH-HEARD].
- [ ] Loop toggles and the end marker moves while the song plays (today both say "Stop first") [DAW-TRANSPORT-LOOP] [DAW-TRANSPORT-RANGE].
- [ ] **Seen by the user in a window with steady audio**: Play starts at the cursor and plays to the end [DAW-TRANSPORT-RANGE]; the timeline follows the playback [DAW-TRANSPORT-FOLLOW]; Loop plays the range for ever, with L in all three keymaps [DAW-TRANSPORT-LOOP]; the start and end markers drag independently (journey test owed) [DAW-TRANSPORT-RANGE]; piano-roll keys sound through the track's patch (journey test owed) [DAW-ROLL-KEYS]; Play's refusals name the missing renderer or song while the project is kept ([issue 425](https://github.com/Nimblesite/osprey/issues/425)) [DAW-TRANSPORT-REFUSAL].
- [ ] Playhead in beats, saved in the project file [DAW-TRANSPORT-TIMELINE].
- [ ] The renderer natively verified on the FerroUI host with the codex project [DAW-RENDER].

## One graph [DAW-GRAPH]

The project is one graph of cables and a patch is a subgraph of it: tracks, buses, Space and Master are nodes, and Play and render run the whole graph in one block loop into Master. No item may add a per-track render path, a separate mix stage or a second routing model.

- [ ] One block loop runs every track, bus, send, pad and the Space into Master through `daw::Playback`; patch modules stop rendering a whole note when it starts and run in the same block loop [DAW-RENDER-PLAY] [DAW-GRAPH-SUBGRAPH].
- [ ] A patch's audio and MIDI inputs take cables from other nodes in the project graph; a patch ends in an Output routed to Master by default, the skin's "Master" card is renamed and no module is called master [DAW-MODULAR].

## Controls while playing [DAW-MIX-DRAG]

- [ ] **Hear each knob move while the song plays**: during a drag the dial, its value and the playing track follow one accepted parameter Signal; the next block of a held note, later notes, mixer and bus controls, Undo and a looping song all follow; pointer-to-queue time measured on the codex song (today 47.9–49.5 ms) and the acoustic latency measured [DAW-TRANSPORT-LIVE-CONTROLS].
- [ ] **A commit does not stall the interface**: release-to-repaint under 16 ms on the codex song, with the project encode, its decode check, the file write and the lane-trace rebuild off the interface thread or deferred (today 50.27 ms on release) [DAW-MIX-DRAG].
- [ ] The same check for the Space send, bus faders, the Space decay and the patch dials [DAW-MIX-DRAG].

## The FerroUI host [NATIVE-FERROUI-RETARGET]

- [ ] Delete the daw's GPUI fallback: the `daw-gpui` target in `src/toolchain/targets/macos/Makefile`, `src/osprey/examples/daw/run.sh` and its line in `src/osprey/examples/daw/PRISM.md`, and the daw's entries in `_gpui-apps`, `_gpui-smoke` and `_gpui-test` in `scripts/make/apps.mk`, once `_prism_ui_test` and `_prism_vst_ui_test` (the `synth` and `synth-vst` targets) run on FerroUI; `make -C src/toolchain/targets/macos prism` opens it on FerroUI and `make -C src/toolchain/targets/tests synth`, `synth-vst` and `make -C src/toolchain/targets/macos/tests daw-events-headless` run there [NATIVE-FERROUI-RETARGET].

## Window layout [DAW-LAYOUT-FILL]

- [ ] **Hard rule ([DAW-LAYOUT-FIXED-SCALE]): resizing the window never resizes the controls**; confirmed in a rebuilt app at small, wide and tall window sizes, every control, label, card and cable keeping its size.
- [ ] **Resizing never changes the zoom**: one scene unit is one point at 100% zoom on every page, the scene drawn unscaled by the host (or the region's size in points given to the app) and every page laid out from the region's width and height instead of 1440 × 940; only the user's zoom commands change the scale.
- [ ] **Song pages use the whole window**: the Timeline and Bus routing pages lay out to the region's real width and height (more lanes, a taller roll and routing table), replacing the fixed widths and heights in `timeline_view.ospml`, `header_view.ospml`, `timeline_geometry.ospml` and `mix_view.ospml` with values measured from the viewport; a window smaller than the page scrolls instead of cutting it off.
- [ ] **The patch canvas pans and zooms**: drag on empty canvas or two-finger scroll pans, pinch or Cmd-wheel zooms, Zoom to Fit shows the whole patch; needs the wheel and pinch events the host does not send yet.
- [ ] **Lists and lanes scroll**: lanes one row at a time with the wheel (the scrollbar already does), and the track list, lanes and routing table scroll with the wheel when they hold more rows than fit.
- [ ] **Timeline zoom**: pinch, Cmd-wheel, Zoom to Selection, dragging the song strip's frame, row height with Alt, the piano roll's own zoom, zoom kept per project (Zoom In, Out and Fit with keys in all three keymaps are built) [DAW-TIMELINE-ZOOM].
- [ ] A journey test at three window shapes (wide, square, tall) asserts each page's body reaches the bottom of its region and a module dragged to the bottom edge lands there.
- [ ] **Windows docked patch panel**: the toolbar band spans the pane the panel leaves and a docked panel narrows or overlays the canvas instead of hiding its right side in a window narrower than 1536 points; the panel stops following the runner's light theme beside the dark canvas (the Windows goldens of 2026-11-23 record both defects) [DAW-INSPECTOR].
- [ ] Separate Timeline and Mix windows where the host supports them [DAW-MENU] [DAW-HEADER].
- [ ] Screenshot goldens compared automatically in the daw's app test targets on macOS; the Windows goldens under `examples/daw/goldens/windows` rewritten for the new header and inspector with `make -C src/toolchain/targets/windows/tests synth-update` (`routed-delay.png` is the one reported) [DAW-PAGES].
- [ ] Real mouse drag on sockets and the menu bar rechecked in the live app [DAW-PAGES].

## Patch windows and routing interactions [DAW-PAGES-PATCH-WINDOW]

- [ ] **Focused Close and ongoing playback verified in the shipping desktop shell** on an unlocked display (Core Video error -6661 blocked it on the locked Mac): closing a patch window by its close control or the focused Close command leaves the project open and the transport running; reopening shows the saved parameters, cables and node positions.
- [ ] **The patch node editor draws Bus Routing**: the separately drawn cards, ports, wires and hit areas in `MixView.flow` are replaced by the patch editor bound to the project graph; the reported `glow` route through Stereo pan, pads, Space and Master exercised; each node selected, Space decay and group gain adjusted, a cable dragged, an outlet fanned out, undo, redo and reopen agree with the displayed values, saved routing and audio [DAW-GRAPH-SUBGRAPH].
- [ ] As many internal routes and sends as the project requires: branching, merging and routing into several buses through the shared node editor, with the full graph and timeline saved and reopened from one project JSON file [DAW-GRAPH-SUBGRAPH].

## Native review regressions [DAW-PAGES-REGRESSIONS]

Four host-free journeys failed independently ([screenshots and review](../../designs/prism-timeline/native-review/README.md)); each stays open until its existing assertions pass alongside the native interaction tests.

- [ ] The drawn sockets and cable hit areas select, connect and disconnect the intended patch ports.
- [ ] Changing module selection shows only that module's controls and preserves its routing.
- [ ] The device journey that checks pitch, level and filter response and displays playback completion or device failure restored.
- [ ] Dragonfly Early Reflections' Cocoa editor verified on an unlocked display (its first OpenGL repaint timed out on this Mac; Prism re-enables Open and accepts edits to all seven parameters after the timeout); native vendor editors on other platforms; plug-in-private state shared with a running DSP instance; the isolated CHOW descriptor worker crash in a macOS URL-session callback.
- [ ] The patch popup, focused-window Close, application Quit, keyboard focus return and screen-reader announcements verified on macOS and Windows.
- [ ] Spectral, shifter and reverb state after the generic fold fix: fractional samples under GC and ARC and the rendered audio against a known-good reference (float samples were read as integers when a generic function was passed directly to `foldList`; [compiler regression](../../regressions.md)).

## Patch layout [PRISM-PATCH-LAYOUT]

- [ ] A patch with no saved positions is laid out along its signal flow: sources on the left, the Output on the right, one column per step, branches stacked, no card over another and no cable through a card [PRISM-PATCH-LAYOUT].
- [ ] Cards keep their designed size in a patch opened from a track [PRISM-PATCH-LAYOUT].
- [ ] The routing page uses the real patch nodes and cables, not its own drawn cards [PRISM-ROUTING-EDITOR].
- [ ] The Grain cloud sample path stays inside its card: the filename in the waveform panel, truncated to its width, the full path in the inspector or an accessible description; verified in a narrow window with a long filename and the output socket clear [PRISM-PATCH-LAYOUT-SAMPLE-LABEL].

## Patch controls and chrome [DAW-CONTROLS-DIAL] [DAW-PAGES-STRIP]

- [ ] **Dials**: arrow-key steps and focus, double-press for the default (a `Parameter` needs a default field), dials on the cards, sliders; Patch Controls and the canvas inspector compared with `docs/designs/prism-timeline/patch-controls.png` in a live window (180 units of travel, Shift ten times finer, one undo per turn, Escape and typed entry are built) [DAW-CONTROLS-DIAL].
- [ ] **Patch Controls uses the design's toggle, dial, slider, menu and button**; the text boxes, "Connect/Disconnect" buttons and platform-styled menu go, and so does the number box that reformats while typing (392 became 923.0) [DAW-CONTROLS].
- [ ] **Strip the UI**: patch cards drop the small name above each card and "Edit in Patch controls" (tapping the card is the edit); values shown rounded with units ("261.63 Hz"); "1 inputs · 1 outputs" and the bottom legend of cable kinds dropped; the piano-roll "Bar 23" label no longer collides with the "Piano roll" title [DAW-PAGES-STRIP].

## Human-centred UI [DAW-HUMAN]

Spec: [DAW-COMMANDS], [DAW-KEYS], [DAW-ACCESS], [DAW-STORE] in [Perception](../../specs/synth/Perception.md#perception-studio-the-daw-daw).

- [ ] One command list feeding menus, shortcuts and a VS Code style palette [DAW-COMMANDS].
- [ ] Stacked commands: a named list of commands with its own parameters, one undo step, all or nothing, in an editable file; a shortcut runs a command or a stack with arguments from the keymap line [DAW-COMMANDS].
- [ ] Commands for an AI agent: run a command by name with arguments from outside the window, one line in and one line out; list the commands and what can run now [DAW-COMMANDS].
- [ ] Accessibility: focus and keyboard operation for every canvas control, role, name, value and state reported for each, contrast and colour independence checked, reduced motion and larger text followed [DAW-ACCESS].
- [ ] Pluggable storage: one store interface with JSON (today), database and git (a commit per save, history, restore) behind it [DAW-STORE].
- [ ] Design screenshots finished art at 2×: one type scale, one spacing grid, the Prism palette only, real project content, no clipped or overlapping text; the lead signs each off before code follows [DAW-HUMAN].

## Project file [DAW-PROJECT]

- [ ] Track and Master channel layouts (stereo default, any count) stored in the project; tracks render per channel [SYNTH-GRAPH-CHANNELS].
- [ ] **Built-in instruments become patches**: every built-in (Pluck, Bow, Bell, Mallet, Operator, Drum) expressed as a patch of synth modules that renders the same samples (a test compares the two renders), `tracks/*.json` migrated, the built-in variants and their renderers deleted; the first edit no longer silently saves a fixed approximation [DAW-PROJECT-PATCH].
- [ ] Every new control, size or limit lands with a test at its edges, at every size it takes [DAW-REVIEW-OPEN].

## Dynamics, buses and EQ [DAW-DYNAMICS] [DAW-MIX-BUS] [DAW-MIX-BUS-EQ]

- [ ] `Compressor` and `Limiter` components in `@osprey/synth`, any channel count, linked (`synth::Limiters` limits a stereo buffer only); tests: sine against the curve within 0.01 dB, inter-sample peaks under the ceiling [DAW-DYNAMICS].
- [ ] Gain reduction and true-peak meters on the Mix page; bus strips with EQ and compressor controls [DAW-DYNAMICS] [DAW-MIX-BUS].
- [ ] Multiband compressor and limiter: Linkwitz–Riley crossovers (3–5 bands) that sum flat, per-band dynamics, modules the user adds to a patch, a bus or Master with no built-in Master chain, settings in daw.td, native Osprey formulas and Lean proofs, flat-sum and per-band tests [DAW-DYNAMICS-MULTIBAND].
- [ ] Pitch effects on tracks and buses: the FFT pitch shifter in semitones and cents and autotune following key, scale or the timeline's notes, each with latency compensation in the renderer [DAW-PITCH-EFFECTS].
- [ ] Parametric EQ: up to 8 bands (bell, low and high shelf, notch, band-pass, tilt, high- and low-pass at 6–48 dB/oct) as RBJ cookbook biquads in native Osprey, each with frequency, gain and Q [DAW-MIX-BUS-EQ].
- [ ] Analog-matched response near Nyquist (oversampled or matched-z), a linear-phase mode for mastering with its latency reported, mid/side and left/right per band, dynamic EQ keyed by a sidechain, and smooth parameter changes without zipper noise or filter blow-up under automation [DAW-MIX-BUS-EQ].
- [ ] Spectrum overlay with the curve drawn on the Mix page and a band dragged on the curve [DAW-MIX-BUS-EQ].
- [ ] EQ tests: magnitude and phase response on swept sines at many frequencies, gains and Qs within 0.01 dB of the analytic curve, flat at 0 dB within 1e-9, stable at the extremes, settings in daw.td round-tripping through the project [DAW-MIX-BUS-EQ].
- [ ] Natively heard on the full codex render [DAW-MIX-BUS].

## Analysis and mix advice [DAW-ANALYSIS]

- [ ] **Track scope on the main view**: selecting a track on the Timeline shows its waveform and band spectrum in the track panel with no Scope module needed; the panel states the verdict in words (the range its notes ask for, where the energy is, what is wrong, for example "bass: most energy above 2 kHz"); the patch's Output card stops drawing an empty trace [DAW-ANALYSIS-TRACK].
- [ ] Band spectrum per track, bus and Master from the STFT, with peak and RMS, on the Mix page (`synth::Spectra` reads ten octave bands for Master today); pitch fit (measured fundamental and partials against the played notes, detuning in cents); per-instrument expected ranges in daw.td with energy outside them reported; a masking report between tracks in the same band; EQ advice (band, dB, reason) applied as a checked project edit [DAW-ANALYSIS].
- [ ] Native Osprey formulas, proofs of their production bodies, and tests on synthetic signals with known spectra [DAW-ANALYSIS].
- [ ] Mix checks per example project run by `make` and red on failure: tonal balance against the genre curve, LUFS and true peak, clipping, masking, mono low end, pitch fit; every project in `tracks/` passes and the mixes that fail are fixed [DAW-ANALYSIS-GATE].
- [ ] Checks per window along the song: a failure alerts at once and marks the bars on the Timeline with track, check, value and limit; tapping it seeks there [DAW-ANALYSIS-TIMELINE].
- [ ] `genre` in daw.td with genre target curves, loudness and dynamic range as versioned data [DAW-ANALYSIS-GENRE].
- [ ] AI review behind an effect: measured spectrum, loudness and masking plus genre in, typed findings out, advisory only, pinned model, timeout [DAW-ANALYSIS-AI].

## MIDI keyboard [DAW-MIDI]

- [ ] The renderer meets [DAW-PERF], then a callback backend plays one track's patch live.
- [ ] The host lists MIDI sources and delivers timed events through the native boundary (CoreMIDI first).
- [ ] Notes play the selected track's patch and record into the timeline at the playhead.
- [ ] Controller learn, stored in the project.

## OSC [DAW-OSC]

- [ ] OSC 1.0 packet reader and writer in Osprey with bounds checks, as a package with tests.
- [ ] UDP listener on localhost behind an effect.
- [ ] Addresses generated from daw.td; messages go through the same checked edits.
- [ ] Feedback of playhead, meters and changed values.

## Songs [SYNTH-SHOWCASE] [DAW-DEMO-SOUND]

- [ ] Original example songs in the spirit of Tipper, Amon Tobin and Mr. Bill (inspiration only; no copying of their tracks or sounds), each pushing granular micro-edits and glitch, spectral freeze, morph and formant shifts, FM and phase-distortion basses, physical models, resampled foley, heavy sidechain and bus processing [SYNTH-SHOWCASE].
- [ ] Each song passes the mix checks for its genre [DAW-ANALYSIS-GATE].
- [ ] The example music sounds too simple: Silt Relay hand-authored (arrangement and patches through the synth module palette), its master rendered and listened through the section changes; the other demo songs revisited (Violet Static is finished) [DAW-DEMO-SOUND].

## Native Osprey formulas and Lean proofs [DAW-MATH]

- [ ] The dial, canvas and timeline geometry migrated from Lean: the native source verified through its callers and each newly proved property connected to its production function; notation tracked in the [native formula checklist](../0041-dependent-types-and-mathlib.md#native-osprey-formulas-math-code-native-notation) [DAW-MATH].

## Performance [DAW-PERF]

Profiled with `sample` at 1 ms through `make -C src/toolchain/targets/macos/tests daw-profile`. Each row is done only when the same measurement is reported again with the target met. A commit to `packages/synth` is tested against `examples/daw` on a clean `git archive HEAD` copy before it lands.

### Engine [DAW-PERF-RENDER] [SYNTH-RENDER-BLOCK]

- [ ] A song with basic tracks and routing renders in at most a fifth of its length and no project in `tracks/` takes more than half (Phosphor Drift: 5.4× its length; bowed, additive, pitchShifter and spectral are the first kernels to optimise) [DAW-PERF-RENDER].
- [ ] Bars 9 and 10 of the codex project (`target/daw/daw tracks/codex-version.json <out> 32 8`, ARC) render in under 14 s (today 36 s) [DAW-PERF-RENDER].
- [ ] Block kernels for oscillator, filter, gain, mixer and output, bit-identical to the reference renderer, which stays as the oracle; then the remaining kernels and the daw player rendering through blocks [SYNTH-RENDER-BLOCK].
- [ ] States kept in schedule order and folded, so one node costs one state; `RenderInputs::parameter`, `input`, `held` and `Graphs::node` resolved once per block into the plan; `Renderer::finished` finds the sink once per render, collects a block's samples in one builder and meters once per block; `daw::BusMix` and `Arrangements::bused` fold into a builder once per side; before and after times reported.
- [ ] Ring delay lines: a `FloatBuffer` primitive in C with a checked index, behind a `Result` (an Osprey list has no in-place update).

### Runtime and code generation

Each lands with a regression program in `src/osprey/tests/regressions/` and the before and after of `daw-profile`.

- [ ] Direct element drops for lists whose elements the compiler proves are ARC bodies; `osp_mem_string_length` no longer takes the heap lock per comparison (a literal's length beside its pointer, or a pointer compare first).
- [ ] In-place reuse of unique records and lists in generated code (a record update no longer allocates and copies a dead unique record).
- [ ] String `==` and `!=` take the fast path (equal pointers, one pass over the bytes, a literal's length known at compile time) and no retain or release is emitted for string literals.
- [ ] `listGet (filterList …) 0` becomes a first-match search with no list (a `listFind` built-in or shape recognition; done by hand only in `ui::Lists::find`).
- [ ] Literal lambdas at `mapList`, `filterList` and `foldList` sites emitted inline with no heap environment; element reads inline in proved loops or runtime fast paths shipped as bitcode; loop-invariant effect-free calls hoisted out of lambdas.
- [ ] List literals as one allocation; the list layout passed to the allocation so `osp_mem_set_layout` leaves the allocation path; nested concat, flatten and map fused onto one builder.
- [ ] A `Result` returned by value when matched at once and never stored.

### Interface [DAW-PERF-ROUND]

Targets: an event that changes nothing under 0.1 ms, a Timeline or Mix event under 0.7 ms, a resize under 2 ms, a drag under 5 ms mean and 8.3 ms worst in `tests/prism_events.rs` (today 10.6 and 18.2 ms).

- [ ] An unchanged event answered without rebuilding the view (compare the root stamp first; tell only regions whose stamp changed).
- [ ] Knobs kept in a map by name or resolved to an index once per frame; `Header::shift` bound once outside the lambda; `Streams` rows written to the bridge as produced, with no `List<Etch>` in between.
- [ ] Atlas, toolbar and patch subtitle cached; a Patch resize redraws a card only when the card scale changed (2.84 ms today, every card redrawn), with `CardArts` and the templates taking a card's knob values once as a record.

### Build time [DAW-PERF-BUILD]

- [ ] One lowered body per function and type arguments in code generation; `unused_symbols` reuses the gate's solve; objects compiled per module, in parallel, cached by hash: `osprey src/osprey/examples/daw --compile --memory=arc --target=macos` takes under 10 s and an unchanged rebuild under 2 s (today about 50 s) [DAW-PERF-BUILD].

## Pocket Orchestra [ORCHESTRA-PLAN]

Phones as instruments around one desktop session host: LAN WebSockets, host-generated audio, three instruments and native and canvas presentations across macOS, Windows, iOS, Android and web; Linux is a follow-on target, and Linux hosting, trusted HTTPS/WSS, motion instruments, saved compositions, event recordings and replay, external lighting and Internet relay each need a follow-up scope decision [ORCHESTRA-PLAN-LATER]. `@osprey/synth` owns the DSP. Commands are in the [README](../../../src/osprey/examples/pocket_orchestra/README.md): `make -C src/toolchain/targets/macos pocket-orchestra`, `make -C src/toolchain/targets/tests pocket-orchestra`, `make -C src/toolchain/targets/macos/tests pocket-orchestra-headless`.

### Feasibility [ORCHESTRA-PLAN-SPIKE]

- [ ] Native capabilities, host canvas support, multitouch, audio and transport adapters inventoried with implementation evidence per platform against [ORCHESTRA-PLATFORMS]; reference hardware, latency distributions, audio buffers and frame timings recorded; failed feasibility criteria resolved before instruments are built out [ORCHESTRA-PLAN-SPIKE].
- [ ] A physical phone sends start, update and end events to a desktop through a bidirectional WebSocket adapter, with host acknowledgements and voice release on disconnect [ORCHESTRA-PLAN-SPIKE].
- [ ] Continuous audio output and simultaneous voices on macOS and Windows, with physical input-to-speaker latency measured against [ORCHESTRA-ACCEPTANCE] [ORCHESTRA-PLAN-SPIKE].
- [ ] The controller served from the desktop: local-network permission and HTTP/WebSocket behaviour verified in mobile Safari, Chromium and Firefox, with the supported local deployment procedure recorded; native controls beside a canvas and a switch to canvas-rendered controls proven on each required host, with simultaneous touches and accessible actions [ORCHESTRA-PLAN-SPIKE].
- [ ] Canonical models declared in typeDiagram, generation verified for Osprey and the required host bindings, JSON and TDBIN compared for codec support, framing and payload size, and the protocol recorded [ORCHESTRA-PLAN-SPIKE].

### Shared session core [ORCHESTRA-PLAN-CORE]

- [ ] Typed session transitions, admission, roster, player ownership, sequence and epoch validation and state snapshots [ORCHESTRA-NETWORK-PROTOCOL]; bounded queues, update coalescing, heartbeat expiry, reconnect and explicit voice release [ORCHESTRA-NETWORK-TIMING].
- [ ] Production and deterministic test handlers for the effects in [ORCHESTRA-EFFECTS], reusing the native capability boundary and the standard diagnostics package [ORCHESTRA-PLAN-CORE].
- [ ] Complete ensemble scenarios on a virtual clock with in-memory peers, including malformed events, denied roles, congestion and suspension, each linked to its spec ID [ORCHESTRA-PLAN-CORE].

### Playable desktop host and first controller [ORCHESTRA-PLAN-ENSEMBLE]

- [ ] macOS and Windows host adapters for transport, continuous audio, output selection and lifecycle, with application decisions in Osprey [ORCHESTRA-PLAN-ENSEMBLE].
- [ ] Percussion end to end from phone touch through authenticated networking to host sound and the shared stage; bass ribbon and chord field with shared key and scale selection, instrument changes and per-player volume and mute [ORCHESTRA-PLAN-ENSEMBLE].
- [ ] Conductor admission, connection health, QR and manual joining, master volume and Silence; both timing modes, accepted-event timestamps and remote stage clock estimation [ORCHESTRA-PLAN-ENSEMBLE].
- [ ] Device loss, host sleep, stalled peers and shutdown while notes are active, and the eight-controller workload, tested; a real ensemble performance through one speaker output with bounded resources and no stuck voices [ORCHESTRA-PLAN-ENSEMBLE].

### Platform and widget coverage [ORCHESTRA-PLAN-HOSTS]

- [ ] Native iOS and Android controllers on the shared Osprey core with local-network permissions, cancellation and foreground recovery; browser controllers and stage viewers served by the desktop host; the additional desktop controller mode on macOS and Windows; `pocket-orchestra` launch targets in the windows, ios, android and web Makefiles [ORCHESTRA-PLAN-HOSTS].
- [ ] Both presentations on every platform in [ORCHESTRA-PLATFORMS], with switching tested while connected, while holding a note and after a reconnect; native control classes, semantic HTML, canvas interaction, accessible equivalents, reduced motion, large text, safe areas and resizing verified [ORCHESTRA-PLAN-HOSTS].
- [ ] Windows packaged with WinUI and MSIX AppContainer capabilities; LAN hosting and local client connections verified under the installed package's actual restrictions, any sandbox exception documented with evidence [ORCHESTRA-PLAN-HOSTS].

### Delivery evidence [ORCHESTRA-PLAN-DELIVERY]

- [ ] Host implementation in the existing `src/toolchain/hosts/` platforms with shared recipes in `scripts/make/`, and view and instrument logic shared across presentations [ORCHESTRA-PLAN-DELIVERY].
- [ ] `pocket-orchestra` and `pocket-orchestra-headless` test targets on every delivered platform (macOS and the host-free suite exist), with app-specific diagnostics under `target/` [ORCHESTRA-PLAN-DELIVERY].
- [ ] Reviewed platform screenshot goldens, accessibility journeys, audio assertions, transport integration tests and measured physical-device acceptance runs; physical iPhone and Android to speaker latency measured on Wi-Fi with device and browser versions recorded; physical output-route switching verified [ORCHESTRA-PLAN-DELIVERY] [ORCHESTRA-PLAN-PLAYABLE].
- [ ] Build and static analysis, cached parallel testing with coverage, then the coverage check, wired into the required CI phases with package coverage thresholds retained or raised [ORCHESTRA-PLAN-DELIVERY].
- [ ] Static duplication analysis on the implementation, host adapters reviewed for duplicated application logic, and model generation reproducible [ORCHESTRA-PLAN-DELIVERY].
- [ ] The [ORCHESTRA-ACCEPTANCE] evidence completed with actual browser versions, operating systems, device models and measured limits; installation, starting a session, joining, permission recovery and LAN troubleshooting documented with the actual app commands [ORCHESTRA-PLAN-DELIVERY].
- [ ] Host echo and reverb storage extracted into reusable synth graph processors, with alias rejection and physical-device latency measured [ORCHESTRA-PLAN-SOUND].

## Tests and verification [DAW-TESTS]

- [ ] The daw test suite type-checked in one piece and the full suite run within its timeout (it exceeded five minutes); journey tests owed for the lane scrollbar and the note makers [DAW-TESTS].
- [ ] Every journey gets more input permutations and more assertions per interaction, across the daw and `@osprey/synth` [DAW-TESTS].
- [ ] Scripts and tooling written in Python-flavour Osprey, not Python [DAW-TESTS].
- [ ] One spec, one plan and one test home per feature: overlapping sections and duplicate helpers merged and the deslop gate green (4.1% against the 4.00% ceiling; `timeline_view.ospml` is the largest offender) [DAW-TESTS].
- [ ] `make ci` passes and a coordinated push is made under the TMC lock [DAW-TESTS].
