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
- [ ] A Prism patch edit is saved into its track and reopens as the same graph ([DAW-PROJECT-PATCH]).
- [ ] A single typeDiagram model: merge `editor.td` into `daw.td`, make the JSON keys the field names, and migrate `tracks/*.json` ([DAW-PROJECT-MODEL]).
- [ ] Node positions, grain sample assets and VST bindings are stored in the project.

## Transport [DAW-TRANSPORT]

- [x] The renderer's excerpt mode (`<from-beat> <beats>`) renders the window without the intro.
- [x] Play renders the current snapshot into a per-bar folder, plays only real audio, replays an unchanged snapshot and renders again after an edit.
- [x] Tickets are bound to the launched folder, so moving the piano roll during a render cannot play the wrong bars.
- [ ] The transport journey test (`the transport renders two bars …`) passes in `make -C src/toolchain/targets/tests synth`. It is written, but blocked by codegen errors in the page code currently being changed.
- [ ] Host cancellation stops the renderer and `afplay` processes (FerroUI).
- [ ] The renderer is fast enough to play from the playhead without waiting (per-note patch voices dominate).

## Pages [DAW-PAGES]

- [ ] Timeline, Mix and Patch pages on FerroUI, with screenshot goldens.
- [ ] A native end-to-end journey: open the codex project, select keysL at bar 9, change the mix, edit the patch, save, reopen and play.

## Verification

- [ ] The daw test suite passes under GC and ARC.
- [ ] `make ci` passes; a coordinated push is made under the TMC lock.
