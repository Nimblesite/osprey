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

- [x] **Render song** in the header bounces the whole project to `<song>/song/piece.wav`: refused while busy, only real audio counts, an unchanged project is kept, Stop cancels it. The transport journey test covers it; the daw suite passes 22/22.
- [ ] Natively verified on the FerroUI host with the codex project.

## Timeline transport [DAW-TRANSPORT-TIMELINE]

- [x] Play bars and Stop from the Timeline.
- [ ] Playhead in daw.td, shown as `bar.beat`, set by tapping the ruler, saved (OspreyOpus1).
- [ ] Start plays from the playhead; Stop, then return to start (OspreyOpus1).
- [ ] Loop span on the ruler.
- [ ] The line follows playback.

## Dynamics [DAW-DYNAMICS]

- [x] Compressor and limiter formulas in [DynamicsMath.lean](../../src/osprey/packages/synth/src/DynamicsMath.lean); the Osprey module generates.
- [ ] `Compressor` and `Limiter` components in `@osprey/synth`, any channel count, linked (synth-advanced).
- [ ] Tests: sine against the curve within 0.01 dB; inter-sample peaks under the ceiling (synth-advanced).
- [ ] Master ends in the compressor and limiter, replacing the `ArrangeMath` limiter; settings in daw.td.
- [ ] Gain reduction and true-peak meters on the Mix page.

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
- [x] Piano-roll note selection and velocity edits are saved into the project.
- [x] Native bar-9 journey on a copy of the codex project. Selected keysL, changed gain, pan and Space, inserted a delay, saved, reopened and played 13.75 seconds of stereo at 48 kHz.

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

- [ ] States folded in schedule order; before and after times reported (synth-advanced).
- [ ] Ring delay lines; before and after times reported (synth-advanced).
- [ ] Parameters and inputs resolved once per block; before and after times reported (OspreyOpus1).
- [ ] Bars 9 and 10 of the codex project render in under 14 s.

**Interface.** `tests/prism_events.rs` drag: 10.6 ms mean, 18.2 ms worst per pointer move. Timeline taps: 0.57 ms mean. Target: drag under 5 ms mean, under 8.3 ms worst (one 120 Hz frame).

| Where a drag move goes | Share | Fix | Owner |
|---|---|---|---|
| `EditorView::keyboardDock` is rebuilt although nothing in it changed | 28% | Cache it with the card art | ferroui-prism-lead |
| `CardArts::card` redraws the dragged card, because its key holds its position | 25% | Draw each card at the origin once and move it | ferroui-prism-lead |
| `Tellings::render` and `Streams::write` encode the whole scene | 30% | Follows from the two rows above and the next | ferroui-prism-lead |
| `PrismArt::backgroundPart` re-emits the scenery | 13% | Keep the built scenery in the editor | ferroui-prism-lead |

- [x] Keyboard dock cached.
- [ ] Dragged card moved, not redrawn.
- [x] Scenery kept. With the dock: 5.1 ms mean, 9.0 ms worst (was 10.6 and 18.2); 22/22 journeys pass.
- [x] `prism_events` holds a drag move to one 120 Hz frame on average (8.3 ms), two at worst.

## Verification

- [x] The 22 DAW journeys passed under GC and ARC. The generated field codec also round-tripped the edited 17-track project with all 2,913 notes and every JSON value equal.
- [ ] `make ci` passes; a coordinated push is made under the TMC lock.
