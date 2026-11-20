# The daw: delivery plan [DAW-PLAN]

The [daw specification](../specs/applications/Daw.md) defines the requirements. The [Timeline designs](../designs/prism-timeline/README.md) are the visual reference, within the [scope](../specs/applications/Daw.md#scope-daw-scope) the specification sets.

## Math is Lean [DAW-MATH]

Every formula in the daw is a `def` in a `.lean` file, and Osprey code for it is generated from that file ([MATH-CODE](../specs/tooling/MathToCode.md)). This covers timing, envelopes, mix ranges, pan laws, the excerpt window, and timeline and piano-roll geometry. No arithmetic formula is written by hand in `.ospml`. The generated modules are never committed. A Lean construct the reader cannot handle is fixed by extending the reader, not by writing Osprey math.

- [x] `ArrangeMath.lean`: beat and second frames, envelopes, pan, the limiter, `mixable`, `inWindow` and `shifted`. `Projects` and `Arrangements` call them.
- [x] `TimelineGeometry.lean`: bars, `barStart`, and lane, roll and mix placement. The transport and timeline call `barStart`.
- [ ] Find the remaining hand-written arithmetic in `src/osprey/examples/daw/src/*.ospml` (pixel offsets, meters, percentages) and move it to Lean.

## Project file [DAW-PROJECT]

- [x] `SongFiles::encode` writes every key `decode` reads; a loaded test checks that the project round-trips, including a routed patch's bindings and cables.
- [x] `Projects::mixed`, `replaced` and `saved` reject out-of-range gain, pan and send, missing tracks and quoted paths. Save checks that the encoded text decodes before it writes.
- [x] An encoding failure is an error result instead of a `"null"` file.
- [x] A Prism patch edit is saved into its track and reopens as the same graph ([DAW-PROJECT-PATCH]).
- [x] The project JSON keys are the `daw.td` field names, and `tracks/*.json` is migrated ([DAW-PROJECT-MODEL]).
- [x] Generate the project and editor types and the project JSON field readers and writer from one `daw.td` with `make -C src/toolchain/targets/tests daw-models TYPEDIAGRAM_DIST=<dist>`.
- [ ] Node positions, grain sample assets and VST bindings are stored in the project.
- [ ] Track and Master channel layouts (stereo default, any count) are stored in the project; tracks render per channel ([SYNTH-GRAPH-CHANNELS]).

## Transport [DAW-TRANSPORT]

- [x] The renderer's excerpt mode (`<from-beat> <beats>`) renders the window without the intro.
- [x] Play renders the current snapshot into a per-bar folder, plays only real audio, replays an unchanged snapshot and renders again after an edit.
- [x] Tickets are bound to the launched folder, so moving the piano roll during a render cannot play the wrong bars.
- [x] The transport journey test (`the transport renders two bars …`) passes. The daw suite passes 22/22 under ARC.
- [x] Natively verified: Play → renderer → afplay → finished, on the codex project (17.7 s stereo, peak −1.0 dB).
- [x] **Play bars** names the two-bar transport. **Save project** writes the current patch without leaving its page.
- [x] Stop cancels the FerroUI transport process group. Native Play → Stop left no renderer and a zero-byte WAV. The host process tests also exercise descendant cancellation.
- [ ] The renderer is fast enough to play from the playhead without waiting (per-note patch voices dominate).

## Rendering the song [DAW-RENDER]

- [x] **Render song** in the header bounces the whole project to `<song>/song/song.wav`: refused while busy, only real audio counts, an unchanged project is kept, Stop cancels it. The transport journey test covers it; the daw suite passes 22/22.
- [ ] Natively verified on the FerroUI host with the codex project.

## Timeline transport [DAW-TRANSPORT-TIMELINE]

- [x] Play bars and Stop from the Timeline.
- [x] Playhead at bar resolution (`Editor.rollBar` in daw.td): a line down the lanes with its handle on the ruler and its position shown; tapping the ruler moves it and keeps the track. Tested in the transport journey.
- [x] Play bars starts at the playhead. Stop halts; Stop again returns the playhead to bar 1.
- [ ] Playhead in beats, saved in the project file.
- [ ] Loop span on the ruler.
- [ ] The line follows playback.

## Dynamics [DAW-DYNAMICS]

- [x] Compressor and limiter formulas in [DynamicsMath.lean](../../src/osprey/packages/synth/src/DynamicsMath.lean); the Osprey module generates.
- [ ] `Compressor` and `Limiter` components in `@osprey/synth`, any channel count, linked (synth-advanced).
- [ ] Tests: sine against the curve within 0.01 dB; inter-sample peaks under the ceiling (synth-advanced).
- [ ] Master ends in the compressor and limiter, replacing the `ArrangeMath` limiter; settings in daw.td.
- [ ] Gain reduction and true-peak meters on the Mix page.

## Buses and sidechain [DAW-MIX-BUS]

- [x] daw.td `Bus` (EQ, compressor, sidechain) and `Track.bus`; codec generated; both projects migrated.
- [x] The renderer sums routed tracks into their bus, then EQ, compressor (keyed by the sidechain track) and gain into Master.
- [x] Bus journey test: 0 dB EQ passes the input, the sidechain ducks by exactly −15 dB (20 dB over, ratio 4), self-keyed and boosted buses, every refusal, codex round trip.
- [x] Codex: `glow` and `tablepad` on a `pads` bus ducked by `kick`.
- [ ] Bus strips on the Mix page: EQ and compressor controls, gain reduction meter.
- [ ] Natively heard on the full codex render.

## Best-of-breed EQ [DAW-MIX-BUS-EQ]

The three-band split is a start, not the EQ. Target: a parametric EQ as good as the best commercial ones, on every track, bus and Master.

- [ ] Up to 8 bands: bell, low and high shelf, notch, band-pass, tilt, and high- and low-pass at 6–48 dB/oct. Biquads from the RBJ cookbook, written in Lean; each band has frequency, gain and Q.
- [ ] Analog-matched response near Nyquist (no cramping): oversampled or matched-z design. A linear-phase mode for mastering, with its latency reported.
- [ ] Mid/side and left/right per band. Dynamic EQ: a band's gain follows a threshold and can be keyed by a sidechain.
- [ ] Smooth parameter changes without zipper noise or filter blow-up under automation.
- [ ] Spectrum overlay with the curve drawn on the Mix page, and a band dragged on the curve.
- [ ] Tests with many input permutations: the magnitude and phase response measured on swept sines at many frequencies, gains and Qs, each within 0.01 dB of the analytic curve; flat at 0 dB within 1e-9; stable at the extremes; settings in daw.td round-trip through the project.

## Songs that push the synthesis [SYNTH-SHOWCASE]

- [ ] Original example songs in the spirit of Tipper, Amon Tobin and Mr. Bill (inspiration only; no copying of their tracks or sounds). Each pushes every technique as far as it goes: granular micro-edits and glitch, spectral freeze, morph and formant shifts, FM and phase-distortion basses, physical models, resampled foley, heavy sidechain and bus processing.
- [ ] Each song passes the mix checks ([DAW-ANALYSIS-GATE]) for its genre.

## Tests and tooling [DAW-TESTS]

- [ ] Every journey gets more input permutations and more assertions per interaction, across the daw and @osprey/synth.
- [ ] Scripts and tooling are written in Python-flavour Osprey, not Python. Tonight's one-off migrations were Python and are not committed.
- [ ] Consolidate: one spec, one plan and one test home per feature; merge overlapping sections and duplicate helpers (the deslop gate must go green).

## Spectral analysis and mix advice [DAW-ANALYSIS]

- [ ] Band spectrum per track, bus and Master from the STFT, with peak and RMS, on the Mix page.
- [ ] Pitch fit: measured fundamental and partials against the played notes, detuning in cents.
- [ ] Per-instrument expected ranges in daw.td; energy outside them reported.
- [ ] EQ advice (band, dB, reason: masking, resonance, mud, harshness, low end), applied as a checked project edit.
- [ ] Masking report between tracks in the same band.
- [ ] Lean formulas and tests on synthetic signals with known spectra.
- [ ] Mix checks per example project, run by `make` and red on failure: tonal balance against the genre curve, LUFS and true peak, clipping, masking, mono low end, pitch fit ([DAW-ANALYSIS-GATE]).
- [ ] Every project in `tracks/` passes its mix checks; fix the mixes that fail.
- [ ] Checks per window along the song; a failure alerts at once and marks the bars on the Timeline with track, check, value and limit; tapping it seeks there ([DAW-ANALYSIS-TIMELINE]).
- [ ] `genre` in daw.td; genre target curves, loudness and dynamic range as versioned data ([DAW-ANALYSIS-GENRE]).
- [ ] AI review behind an effect: measured spectrum, loudness and masking plus genre in, typed findings out, advisory only, pinned model, timeout ([DAW-ANALYSIS-AI]).

## MIDI keyboard [DAW-MIDI]

- [ ] Realtime audio path: the renderer meets [DAW-PERF], then a callback backend plays one track's patch live.
- [ ] Host lists MIDI sources and delivers timed events through the native boundary (CoreMIDI first).
- [ ] Notes play the selected track's patch.
- [ ] Record into the timeline at the playhead.
- [ ] Controller learn, stored in the project.

## OSC [DAW-OSC]

- [ ] OSC 1.0 packet reader and writer in Osprey with bounds checks, as a package with tests.
- [ ] UDP listener on localhost behind an effect.
- [ ] Addresses generated from daw.td; messages go through the same checked edits.
- [ ] Feedback of playhead, meters and changed values.

## Pages [DAW-PAGES]

- [x] Timeline, Mix and Patch pages on FerroUI ([screens](../../src/osprey/examples/daw/screens)).
- [ ] Screenshot goldens compared automatically in the app test targets.
- [x] Native video journey ([DAW-VIDEO]): 23 clicks across five voice types, Mix and Play bars; each click changes the screen; Play rendered 6.8 MB of bars audio in 26 s.
- [ ] Fix: a render cancelled mid-way leaves a 0-byte `piece.wav` that the next Play replays as unchanged (found by [DAW-VIDEO]).
- [ ] Fix: Patch → Timeline after Audition ignored the click in 1 of 3 native runs (found by [DAW-VIDEO]).
- [ ] Audition audio in the video soundtrack (the host writes what it plays).
- [x] Piano-roll note selection and velocity edits are saved into the project.
- [x] Native bar-9 journey on a copy of the codex project. Selected keysL, changed gain, pan and Space, inserted a delay, saved, reopened and played 13.75 seconds of stereo at 48 kHz.

### Menu bar and header [DAW-MENU] [DAW-HEADER]

- [x] The macOS menu bar holds the commands: File (Open Song ⌘O, Save ⌘S, Render Song ⌘R), Edit (Undo, Redo), View (pages ⌘1–3, Modules, Patch Controls, Keyboard) and Transport. The FerroUI host builds it from `Ui::section`s among the root's context-menu actions; read back from the running app and driven with View → Timeline.
- [x] One header on Patch, Timeline and Mix, drawn after [piano-roll.png](../designs/prism-timeline/piano-roll.png): bordered tabs with icons, song strip (BPM, meter, position, what is sounding), Play/Stop icon group. Save, Open, Render, Keyboard, Audition and Patch Controls are no longer canvas buttons.
- [x] Pages fill the window (zoom 0) unless the keyboard is docked.
- [x] The native application menu reads Prism. The FerroUI host sets the application name before the platform exports its menus.
- [x] Match the reference’s 1440 × 940 canvas. The header ends at y=59, the song strip ends at y=142, seven full-height arrangement lanes sit above the piano roll, and the footer begins at y=909.
- [x] Keep the native menu bar alive when Mix adds the Routing menu. The macOS journey drives 23 interactions, then records rendered bars in a stereo video. Choosing Master and pads through Accessibility restores every value in the decoded project.
- [x] Choose the track output in place from Timeline or Mix and create a named stereo group in one save. The host journey covers cancel, duplicate and reserved names, pointer assignment and keyboard assignment. The running macOS app routed both keys tracks with every other project field preserved. [Chooser and group screenshots](../designs/prism-timeline/README.md#running-ferroui-window)
- [x] Select a grouped track’s dry bus or Space return in the signal diagram. The host journey reads back the appropriate inspector fields and verifies that inspecting both branches preserves the saved project.
- [x] Cache the miniature pitch traces when the project opens. The native test taps every arrangement page and ruler, moves every gain fader, and rereads the project after switching the dry route from pads to Master and back.

- [x] `ui::Lists::flatten` joined groups one by one, copying the list so far each time, and every frame's paths and scene rows went through it. It joins by halves now: a Timeline tap fell from 10 ms to 1.3 ms (45e335118).
- [x] The song strip shows the song overview: one column per bar, as tall as the bar is busy, with the piano roll's two bars framed; cached when the song opens.
- [x] The application menu is Services, Hide Prism, Hide Others, Show All and Quit: the toolkit's "About FerroUI" is dropped (read back from the running app).
- [x] The Patch page's header reads "Prism / Patch": the open page is lettered over the subtitle baked into the skin's art.
- [ ] Separate windows for Timeline, Patch and Mix (user: "shoot towards multi window"); the host mounts one window today.

### Human-centred UI, user orders of 21 Nov [DAW-HUMAN]

Spec: [Daw.md](../specs/applications/Daw.md) sections [DAW-HUMAN], [DAW-MODULAR], [DAW-CONTROLS], [DAW-ACCESS], [DAW-COMMANDS], [DAW-KEYS], [DAW-STORE], [DAW-INSPECTOR].

- [x] Spec states the human-centred basis (ISO 9241-210) as checkable rules.
- [x] Canvas inspector for modules without skin art: a slider per parameter, set in place, one undo step (26a2241f6).
- [x] Sockets are 28-unit targets that win over overlapping cards; a wire drag is tested the way a hand drags (3996374e3).
- [x] Song pages centre in a wide window, show "No song open" with Open Song, and the patch keyboard docks under Track patch only (a9cfc0a76).
- [x] Patch Controls panel is dark in the Prism palette, without a second transport row (3cd896ee6).
- [ ] **Dials ([DAW-CONTROLS-DIAL]).** Every numeric field is a round dial with its number: press, hold, move up or down; Shift for fine steps; double-press for the default; typing and arrow keys. Slider (horizontal or vertical) where a length reads better. Design worked out with prism-routing-design. Owner: ferroui-prism-lead.
- [ ] **Patch Controls uses the design's controls.** The text boxes, "Connect/Disconnect" buttons and platform-styled menu in that panel are replaced by the toggle, dial, slider, menu and button of [DAW-CONTROLS]; the number box that reformats while typing (392 became 923.0) goes with them.
- [ ] **Keymaps ([DAW-KEYS]).** Prism, Ableton style and Logic style from a Shortcuts menu, a custom keymap file, the choice kept between launches, and a shortcut on every command. In progress: model, presets, file checks and menus are written and not yet committed.
- [ ] **Command palette ([DAW-COMMANDS]).** One command list feeding menus, shortcuts and a VS Code style palette.
- [ ] **Stacked commands ([DAW-COMMANDS]).** A stack is a named list of commands with its own parameters (a macro), one undo step, all or nothing, in an editable file. A shortcut runs a command or a stack, with arguments if the keymap line gives them. Needs the one command list first.
- [ ] **Design screenshots are gorgeous (user order).** Every PNG in `docs/designs/prism-timeline` is finished art at 2×: one type scale, one spacing grid, the Prism palette only, real project content (no placeholder names), no clipped or overlapping text. prism-routing-design draws; the lead signs each off before code follows it.
- [ ] **Commands for an AI agent ([DAW-COMMANDS]).** Run a command by name with arguments from outside the window, one line in and one line out; list the commands and what can run now.
- [ ] **Undo and Redo everywhere ([DAW-HUMAN]).** Today they cover the patch graph; notes, mix and routing edits save straight to the project and do not undo.
- [ ] **Accessibility ([DAW-ACCESS]).** Focus and keyboard operation for every canvas control, role/name/value/state reported for each, contrast and colour-independence checked, reduced motion and larger text followed.
- [ ] **Pluggable storage ([DAW-STORE]).** One store interface with JSON (today), database and git (a commit per save, history, restore) behind it.
- [ ] **Master-only render ([DAW-RENDER-MASTER]).** Default render is the single mastered track (done); stems as a separate, explicit render (not built).
- [ ] **Patch Output and inputs ([DAW-MODULAR]).** A patch ends in an Output routed to Master by default; the skin's "Master" card is renamed and no module is called master. Patches declare audio and MIDI inputs as modules and route them with cables.
- [ ] Windows goldens under `examples/daw/goldens/windows` predate the new header and inspector and must be rewritten on Windows (`make -C src/toolchain/targets/windows/tests synth-update`); `routed-delay.png` is the one reported.
- [ ] Real mouse drag on sockets and the menu bar need rechecking in the live app once the desktop is unlocked (the screen was locked during this work).

### Strip the UI [DAW-PAGES-STRIP]

From the committed screens: cut everything that repeats or says nothing (ferroui-prism-lead owns the views).

- [ ] Patch cards: drop the small name above each card (the title already says it) and "Edit in Patch controls" on every card and in the inspector; tapping the card is the edit.
- [ ] Round values for display: "261.6255653005986" shows as "261.63 Hz" (C4). Units on every value.
- [ ] Drop "1 inputs · 1 outputs" (the ports show it) and the bottom legend of cable kinds.
- [x] One play control: the header has one Play and one Stop (an icon group); Audition is a Transport menu command.
- [ ] Long enumerated labels ("Curve (0 tanh, 1 hard, 2 cubic, 3 fold)") become a named choice, not a number with a legend in the label.
- [ ] Header subtitles ("Signal flow", "Timeline") repeat the selected tab; drop them.
- [ ] Inspector shows the selected node's parameters, never an empty "Parameters" heading.
- [ ] Timeline: the piano-roll "Bar 23" label must not collide with the "Piano roll" title.

## Performance [DAW-PERF]

Profiled 21 Nov with `sample` at 1 ms. Each fix is owned, and is done only when its owner reports the same measurement again with the target met.

**Engine.** `target/daw/daw tracks/codex-version.json <out> 32 8` (bars 9 and 10, ARC): 42.2 s of CPU for 27.5 s of audio, so 0.65× real time. Target: under 14 s (2× real time).

| Where the time goes | Share | Fix | Owner |
|---|---|---|---|
| Reference counting and allocation (`arc_drop_child`, `osp_retain`, `osp_release`, `arc_alloc`, `memset`) | 78% | See the next three rows; the DSP arithmetic itself is under 5% | synth-advanced |
| `RenderInputs::emitted` copies every kernel state for each node of each sample, and `held` filters them again | largest | Keep states in schedule order and fold over them, so one node costs one state | synth-advanced |
| Delay lines are rebuilt each sample (`listAppend tail input` copies the whole line) | large | A ring: fixed storage with a write position | synth-advanced |
| `RenderInputs::parameter` finds each value by node and string key for each sample (`strlen`, `memcmp`, `filterList`) | 6% plus its allocations | Resolve each node's parameters, inputs and `enabled` once per block into the plan | OspreyOpus1 |

Measured attribution (self plus runtime time, of 15,300 samples): `RenderInputs::parameter` 19%, an anonymous per-sample closure 11%, dropping the old frame 7%, `RenderInputs::input` 7%, `emitted` 6.5%, `Arrangements::bused` 5%, `Graphs::node` 4.5%, `held` 4.5%, `RenderKernels::step` (the `enabled` string test) 4%.

Tried and rejected: replacing `listGet (filterList …) 0` with a `foldList` search doubled the time (42.3 s to 88.7 s, identical audio). A closure call per element costs more than the built-in filter, so the lookups must leave the sample loop, not be rewritten inside it.

Also tried and rejected: narrowing the snapshot to one component's parameters before its lookups left the time unchanged (42.25 s). The lists are already short; the cost is the fixed price of each lookup, allocation and state copy, paid once per component per sample.

**Decision: render in blocks ([SYNTH-RENDER-BLOCK]).** Each component renders a whole block (64 samples by default) from its input blocks in one call. Parameters, inputs and state are resolved once per block, and state is written back once per block, so the fixed price is divided by the block size and the arithmetic is what remains. A feedback cable delays by one block, or its cycle runs at a block of one sample when the patch asks for sample-exact feedback. Automation splits a block at its event. The audio of a patch without feedback must stay bit-identical to the reference renderer, which stays as the oracle in tests.

- [ ] Block kernels for oscillator, filter, gain, mixer and output, checked bit-identical against the reference (synth-advanced).
- [ ] The remaining kernels, then the daw player renders through blocks (synth-advanced, OspreyOpus1).
- [ ] States folded in schedule order; before and after times reported (synth-advanced).
- [ ] Ring delay lines. Blocked (synth-advanced, 21 Nov): an Osprey list has no in-place update, so a ring still copies; it needs a runtime primitive for a fixed float buffer, kept in C.
- [ ] Parameters and inputs resolved once per block; before and after times reported (OspreyOpus1).
- [ ] Bars 9 and 10 of the codex project render in under 14 s.

**Runtime and code generation ([MEM-ARC-RECENT]).** The renderer is slow at every level under it, so each level has its own row. Second profile after the first fix (10,300 samples): `arc_release_zero_locked` 17%, `arc_drop_child` 15%, `arc_alloc_locked` 7%, `osp_release` 6%, `osprey_list_get` 6%, `osp_retain` 6%, `memset` 5%, `osp_arc_free` 4%, `osp_mem_string_length` with `strlen` 6%. The Osprey code itself is under 5%.

| Level | Bottleneck | Fix | Result or owner |
|---|---|---|---|
| ARC runtime | Every dup, drop and child drop probed the registry hash table to ask "is this ours?" | Direct-mapped cache of recent members, cleared where membership ends | Done: 42.3 s to 35.1 s, identical audio |
| ARC runtime | A freed large block left a tombstone, so probe runs grew until the next rebuild | Backward-shift deletion, no tombstones | Done: no measurable change once the cache was in (35.9 s); kept because probe length no longer depends on churn |
| ARC runtime | A string literal is in rodata, so its length, dup and drop each ran a whole probe to miss | Cache of recent non-members, cleared on insert | Done: 35.0 s |
| ARC runtime | A list of records drops each element through the probe (`arc_drop_list_hdr`) | Tag a list whose elements the compiler proves are ARC bodies, and drop them by direct header read, as `MASK_DIRECT` does for records | Open (lead) |
| ARC runtime | `osp_mem_string_length` takes the heap lock and looks the string up for every comparison | Store the length beside the pointer for literals, or compare by pointer first when both are literals | Open (lead) |
| Code generation | A record update (`run(held = …)`) allocates a new record and dups every field although the old one is unique and dead | Reuse the unique record in place: the drop-then-allocate pair of the same layout becomes a field store | Open (OspreyOpus1) |
| Code generation | `mapList`, `filterList` and `listAppend` allocate a new list for each call inside the sample loop | Reuse the argument's storage when it is unique; fuse `listGet (filterList …) 0` into a search with no list | Open (OspreyOpus1) |
| List runtime | No fixed float buffer, so a delay line is copied per sample | A `FloatBuffer` primitive in C with checked index, behind a `Result` | Open (synth-advanced reported the blocker) |
| Renderer | The fixed price per component per sample | Block rendering, below | synth-advanced, OspreyOpus1 |
| daw mix | `BusMix` and `Arrangements::bused` build each side with `listAppend` and `listGet` per sample | Fold into a builder once per side; measured at HEAD with buses: 35.6 s, so no regression yet | synth-advanced |

Tried and rejected in code generation: stopping `listGet (filterList …) 0` at its first match (the list then holds one element). 34.8 s against 35.0 s, identical audio, so it was backed out: the cost is building and dropping the one-element list, not the scan. The fix that pays is a search that returns the element with no list at all, which needs the fused `match listGet` path to own the element across the loop's region.

The runtime fixes alone reach 0.79× real time. The 2× target needs the code generation rows and block rendering; no runtime change can remove allocations the generated code asks for.

- [x] Member cache, non-member cache and tombstone-free deletion in the ARC registry (42.3 s to 35.0 s).
- [ ] Direct element drops for proved lists.
- [ ] In-place reuse of unique records and lists in generated code.
- [ ] `FloatBuffer` primitive.

**Accountability, 01:06 on 21 Nov.** Each owner was asked for a hash and a measured time.

| Owner | Asked for | Delivered |
|---|---|---|
| ferroui-prism-lead (lead) | Interface target; runtime and code generation rows | Interface target met (4.8 ms mean). ARC registry b730e7d28: 42.3 s to 35.0 s. Two code generation and two renderer experiments measured and rejected, recorded above. Engine target not met: 0.79× real time against 2× |
| synth-advanced | States folded in order, ring delays, block kernels | None landed. Reported the ring-delay blocker (no fixed buffer primitive). Landed buses with EQ and a sidechain compressor on `DynamicsMath.lean` instead (cf62e50b7), committed while HEAD did not compile, tested on a dirty tree. No reply at 01:06 |
| OspreyOpus1 | Parameters resolved once per block | Not started. Landed Split, Join and Route (bb930140d) without the daw's exhaustive match, so HEAD was red for 12 minutes until 6fc3979d6; the second red HEAD of the night. No reply on performance at 01:06 |

Rule from tonight: a commit to `packages/synth` is tested against `examples/daw` on a clean `git archive HEAD` copy before it lands.

**Open defects reported by OspreyOpus1 from the native journey (not yet reproduced by a test).**

- [ ] A render killed part-way leaves `piece.wav` that the next Play accepts or replays as silence. `fresh` already refuses a file of 44 bytes or less, so the suspect is a partly written file; the renderer should write to a temporary name and rename. Failing test first.
- [ ] A tap on Timeline right after Audition is ignored about one run in three. Failing test first.

**Interface.** `tests/prism_events.rs` drag: 10.6 ms mean, 18.2 ms worst per pointer move. Timeline taps: 0.57 ms mean. Target: drag under 5 ms mean, under 8.3 ms worst (one 120 Hz frame).

| Where a drag move goes | Share | Fix | Owner |
|---|---|---|---|
| `EditorView::keyboardDock` is rebuilt although nothing in it changed | 28% | Cache it with the card art | ferroui-prism-lead |
| `CardArts::card` redraws the dragged card, because its key holds its position | 25% | Draw each card at the origin once and move it | ferroui-prism-lead |
| `Tellings::render` and `Streams::write` encode the whole scene | 30% | Follows from the two rows above and the next | ferroui-prism-lead |
| `PrismArt::backgroundPart` re-emits the scenery | 13% | Keep the built scenery in the editor | ferroui-prism-lead |

- [x] Keyboard dock cached.
- [ ] Dragged card moved, not redrawn (the largest part of what remains; not needed for the target).
- [x] Scenery kept. With the dock: 4.8 ms mean, 5.1 ms worst (was 10.6 and 18.2); 22/22 journeys pass. The interface target is met.
- [x] `daw_events` also taps the ruler and drags every Mix gain fader: 1.8 ms mean, 10.7 ms worst (the release saves the project).
- [x] `prism_events` holds a drag move to one 120 Hz frame on average (8.3 ms), two at worst.

## Verification

- [x] The 22 DAW journeys passed under GC and ARC. The generated field codec also round-tripped the edited 17-track project with all 2,913 notes and every JSON value equal.
- [x] 21 Nov 01:16: the daw suite passes 24/24 (Render song, buses and sidechain, arrangements), checked on a clean `git archive` of HEAD. `make -C src/toolchain/targets/macos/tests daw-events-headless` passes on the FerroUI host: Mix fader drags 1.16 ms mean (7.35 ms worst, 136 events), Timeline taps 0.51 ms (0.95 ms, 119), Prism drags 3.68 ms (4.00 ms, 55).
- [ ] The duplication gate is green: 4.1% against the 4.00% ceiling on 21 Nov.
- [ ] `make ci` passes; a coordinated push is made under the TMC lock.
