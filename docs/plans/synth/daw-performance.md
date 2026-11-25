# The daw: performance plan

Part of the [daw delivery plan](daw.md).
## Performance [DAW-PERF]

Profiled 21 Nov with `sample` at 1 ms. Each fix is owned, and is done only when its owner reports the same measurement again with the target met.

**Engine.** `target/daw/daw tracks/codex-version.json <out> 32 8` (bars 9 and 10, ARC): 42.2 s of CPU for 27.5 s of audio, so 0.65× real time. Target: under 14 s (2× real time).

- [ ] [DAW-PERF-RENDER] A song with basic tracks and routing renders in at most a fifth of its length (5 minutes in about 1 minute), and no project in `tracks/` takes more than half its length. Phosphor Drift on 21 Nov: 1229 s for 226.9 s, so 5.4× its length, about 27× too slow. The pi bench (pi-render-bench-1..3) is measuring the other projects. Per module, 1 beat of Violet Static (beat 64, one track alone): the bowed+additive+pitchShifter+spectral Choir smear took over 150 s, bowed swapped for a wavetable still over 150 s (additive), wavetable+LFO lowpass+shifter 8 s; the Melt lead fell from 29 s to 5 s without its FFT pitch shifter. A full Violet Static (54.9 s) did not finish in 1800 s, so bowed, additive, pitchShifter and spectral are the first kernels to optimise.
- [x] Index upstream routes once for [DAW-PATCH-HEARD]. The graph fixture with up to 320 routed modules, a parallel send, repeated roots, and both cable orders fell from 5.01 billion to 94.7 million retired instructions in `24be3243e`. The Silt Relay render from beat 9 for two beats retained all 141,496 frames byte for byte. The synth suite passed 91 tests under both GC and ARC, including delayed feedback.

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

- [x] Interrupted renders preserve the previous complete WAV. The renderer publishes the temporary WAV after successful encoding, and playback rejects truncated cached files. The interrupted-cache regression passes under GC and ARC.

**Interface.** `tests/prism_events.rs` drag: 10.6 ms mean, 18.2 ms worst per pointer move. Timeline taps: 0.57 ms mean. Target: drag under 5 ms mean, under 8.3 ms worst (one 120 Hz frame).

| Where a drag move goes | Share | Fix | Owner |
|---|---|---|---|
| `EditorView::keyboardDock` is rebuilt although nothing in it changed | 28% | Cache it with the card art | ferroui-prism-lead |
| `CardArts::card` redraws the dragged card, because its key holds its position | 25% | Draw each card at the origin once and move it | ferroui-prism-lead |
| `Tellings::render` and `Streams::write` encode the whole scene | 30% | Follows from the two rows above and the next | ferroui-prism-lead |
| `PrismArt::backgroundPart` re-emits the scenery | 13% | Keep the built scenery in the editor | ferroui-prism-lead |

- [x] Keyboard dock cached.
- [x] Dragging reuses the module artwork. The native journey checks the painted sprite, socket and switch track the pointer while the inspector stays fixed in three viewport shapes (077da2b1e). Mean patch move time fell from 3.22 ms to 0.74 ms. A separate five-second window benchmark on the Violet Static grain patch measured 1.80 ms per event before and 1.09 ms after, with 307 shapes in both runs.
- [x] Each Timeline redraw computes the visible ruler interval once (9c33cf379). Dragging the Timeline gain control in a 128-bar project with eight offscreen sixteenth-note parts fell from 30.12 ms to 0.83 ms per move. Both figures measure the native application event handler.
- [x] Scenery kept. With the dock: 4.8 ms mean, 5.1 ms worst (was 10.6 and 18.2); 22/22 journeys pass. The interface target is met.
- [x] `daw_events` also taps the ruler and drags every Mix gain fader: 1.8 ms mean, 10.7 ms worst (the release saves the project).
- [x] `prism_events` holds a drag move to one 120 Hz frame on average (8.3 ms), two at worst.

### Profile round, 21 Nov 10:30 [DAW-PERF-ROUND]

Measured at 486b165cf with `make -C src/toolchain/targets/macos/tests daw-profile` (new: `scripts/make/daw-profile.sh`, the ignored host test `tests/daw_profile.rs`, and `scripts/make/sample-attribution.py`, which names the Osprey function behind each runtime call in a 1 ms `sample`). Project: `tracks/codex-version.json`, ARC. A row below is done only when its owner runs the same target again and writes the new number beside the old one.

**What a user action costs** (milliseconds in the app's `event` export, mean of 117 or more events; the host adds 0.05):

| Action | Timeline | Mix | Patch (64 shapes) |
|---|---|---|---|
| Any pointer event, key or frame tick, **including one that changes nothing** | 2.0 to 2.1 | 1.5 to 1.6 | 0.47 to 0.50 |
| Window resize | 12.4 | not measured | 3.2 |
| Open the page | 2.3 | 1.7 | 3.3 |
| Show or hide Patch Controls, Keyboard | 5.0, 5.3 | | |
| Launch with the song | 50 | | |

Every event fits a 120 Hz frame, so nothing here is felt yet. The finding is the shape: the cost does not depend on what happened. A frame tick that changes nothing costs the same as a tap, because the whole page is rebuilt and rewritten on every event, and a resize costs six times that.

**Interface: where the time goes.** Share of app samples; "tick" is the Timeline frame tick (3,843 samples), "resize" the Timeline resize (3,870), "mix" the Mix tick (3,923), "patch" the Patch tick (3,842).

| Function (file:line) | Share | Cause | Fix | Owner |
|---|---|---|---|---|
| `synth::Header::placed`, `placedAreas` ([header_view.ospml:149](../../../src/osprey/examples/daw/src/header_view.ospml), :152), through `Header::shift` and `ui::Machines::number` | tick 34.5%, mix 50.3%, tap 26.7% | The lambda calls `shift editor` once per shape (about 400). Each call reads two knobs by name | Bind the shift once, outside the lambda | ferroui-prism-lead |
| `ui::Machines::knob` ([machine.ospml:112](../../../src/osprey/packages/native/src/machine.ospml)) and its predicate `k.knobName == name` (`__closure_fn_12`) | tick 35.9%, resize 42.1%, mix 51.8% | A knob is found by comparing its name with every knob's name, for every read, through `ui::Lists::find` ([base.ospml:23](../../../src/osprey/packages/native/src/base.ospml)): a whole-list filter, a list and a boxed result per read | Keep knobs in a map by name, or resolve a name to its index once per frame | ferroui-prism-lead |
| `ui::Streams::write`, `etches`, `path`, `step` ([stream.ospml:55-62, 76-78, 106, 116](../../../src/osprey/packages/native/src/stream.ospml)) | tick 41.5%, tap 44.4%, patch 21.6% | Every path step is built as a record inside a one-element list; `Lists::flatten` joins them; only then are the rows written. One step costs a record, a list, a builder and a join | Write each row to the bridge as it is produced, folding the refused count; no `List<Etch>` in between | ferroui-prism-lead |
| `ui::Lists::spanned` ([base.ospml](../../../src/osprey/packages/native/src/base.ospml), called by `flatten`) | tick 16.9%, tap 17.6% | Joining by halves still copies every element log n times and retains and drops each one | Goes away with the row above; elsewhere, one builder that every group is pushed into | ferroui-prism-lead |
| `ui::Surfaces::shown` and `ui::Tellings::render` (the whole telling) | 97.5% of every event | No event is ever answered with "nothing changed": the view is rebuilt, told and written even when the stamp is the same | Compare the root stamp first and answer an unchanged event without building the view; tell only regions whose stamp changed | ferroui-prism-lead |
| `synth::CardArts::refreshed` ([card_art.ospml:28](../../../src/osprey/examples/daw/src/card_art.ospml), from [editor_app.ospml:213](../../../src/osprey/examples/daw/src/editor_app.ospml)), `CardArts::card`, `PrismArt::drawn`, `ui::Canvases::artboard`, `ui::Templates::measure` and `fill` | resize 78.9% (card 64.8%, artboard 58.1%, measure 43.1%, fill 41.6%) | A resize redraws the art of every Patch card, whichever page is showing, and each template piece reads its knobs by name | Refresh card art only on the Patch page and only when the card scale changed; give a card its knob values once, as a record | ferroui-prism-lead |
| `synth::PrismArt::atlas` | patch 14.4%, mix 5.4%, tick 3.7% | The atlas list is rebuilt for every event | Build it once at launch and keep it in the editor | ferroui-prism-lead |
| `synth::Header::patchSubtitle`, `EditorView::toolbar` | patch 7.7%, 8.8% | Rebuilt per event although they change only with the page and the track | Cache with the header, keyed by page and track | ferroui-prism-lead |
| `synth::Graphs::node` | patch 4.4% | Finds a node by filtering the whole node list | Same search fix as `Lists::find` (code generation row 2) | synth-advanced |

Targets: an event that changes nothing under 0.1 ms; a Timeline or Mix event under 0.7 ms; a resize under 2 ms.

- [x] `Header::placed` and `placedAreas` bind the shift once. Same machine load, before and after: Timeline 2.52 ms to 1.71 ms per event, Mix 1.92 ms to 0.89 ms (the quiet-machine figures in the table above were 2.05 and 1.55).
- [ ] Unchanged events answered without rebuilding the view.
- [x] `Streams` writes rows directly, with no intermediate list: one fold-style encoding (`Streams::laid`, `etched`) that the reader collects and the writer sends row by row. With the three fixes below it, 21 Nov 11:46, same probe: Timeline event 1.65 ms to 0.52 ms, Timeline frame tick 1.88 to 0.58, Mix 0.93 to 0.65, Patch 0.60 to 0.40, command Timeline 1.98 to 0.60.
- [x] Knob search stops at the first match and builds no list (`ui::Lists::find` through `sought`); the name compare no longer takes a lock (code generation row 1). A map by name is still open if `Machines::knob` shows again in a profile.
- [x] Card art refreshed only on the Patch page (`CardArts::cards`): Timeline resize 12.43 ms to 2.34 ms. Still open: a Patch resize redraws every card (3.38 ms to 2.84 ms) although only the card scale matters.
- [ ] Atlas, toolbar and patch subtitle cached.

**Engine.** Bars 9 and 10 again: 36.0 s (35.4 s of CPU, with the sampler attached). Nothing has moved since 35.0 s, and none of the block-rendering items above has landed. The same functions hold the time (12,808 samples, own code plus the runtime each calls directly):

| Function (file:line) | Share | Cause |
|---|---|---|
| `synth::RenderInputs::parameter` ([render_inputs.ospml:17](../../../src/osprey/packages/synth/src/render_inputs.ospml)) | 23.2% | `listGet (filterList (snapshot.parameters, \p => p.node == identity && p.key == key)) 0` per parameter per node per sample: a string compare per parameter, a list and a boxed result per read |
| The per-sample closures of `synth::Renderer` ([renderer.ospml:46, 87, 91, 94](../../../src/osprey/packages/synth/src/renderer.ospml)): `sample`, `evaluated` (the fold over `plan.order` calling `nextNode`) and `finished` | 9.4% + 5.8% + 3.0% own, 92% inclusive | Four closures deep per sample, each a heap environment and a boxed `Result` through `Outcomes::carried`. `finished` searches `outputs plan.graph` again, appends one sample with `listAppend` and rebuilds every meter with `metered`, per sample |
| `RenderInputs::input` 6.4%, `emitted` 5.5%, `held` 4.3%, `Graphs::node` 4.2%, `RenderKernels::step` 4.2%, `RenderKernels::selected` 2.3% | 27% | State and node found by filtering lists, per node per sample |
| `daw::Arrangements::bused` | 3.6% | A list built per sample per side |
| `PartialBanks::partial`, `stepped` | 4.0% | One record per partial per sample |

Leaf time is the runtime, not arithmetic: `arc_drop_child` 18.2%, `arc_release_zero_locked` 18.1%, `osp_retain` 7.8%, `arc_alloc_locked` 6.9%, `osprey_list_get` 6.6%, `osp_release` 6.5%, `memset` 5.1%, `osp_mem_string_length` 3.0%.

- [ ] `RenderInputs::parameter`, `input`, `held` and `Graphs::node` resolved once per block into the plan (the existing block-rendering items; owner still OspreyOpus1 and synth-advanced, nothing delivered).
- [x] First step, 21 Nov 11:55: the per-sample lookups (`RenderInputs::held`, `parameter`, `input`, `sample`, `Renderer::measured`, two in `RenderKernels`) stop at the first match through `synth::Seek::first` and build no list: bars 9 and 10 render in 35.2 s, from 39.8 s at the same commit and load, audio byte-identical. `Graphs::node` keeps the filter: its closure is still generic where the daw's tests instantiate it, and code generation refuses a generic closure value. `Seek::from` is now 19% of the render, so resolving once per block is still the fix.
- [ ] `Renderer::finished` finds the sink once per render, collects a block's samples in one builder and meters once per block, not per sample.

**Code generation and runtime: what the generated code does that it need not.** Each row was read from the LLVM IR of the daw (`osprey src/osprey/examples/daw --llvm --memory=arc --target=macos`) and matched to the samples. These cost every Osprey program, not only the daw.

| # | What is generated | Evidence | Cost measured | Fix |
|---|---|---|---|---|
| 1 | **String `==` is an ordering compare with two locked length lookups.** `a == b` emits `call i32 @osp_string_compare` then `icmp eq 0`. `osp_string_compare` ([string_runtime.c:63](../../../src/toolchain/compiler/runtime/string_runtime.c)) takes both lengths through `osp_strlen`, which calls `osp_mem_string_length` ([memory_arc.c:234](../../../src/toolchain/compiler/runtime/memory_arc.c)): the heap lock and a registry lookup each, a `strlen` when the string is a literal, then `memcmp` | IR of `__closure_fn_12`; `osp_mem_string_length` + `memcmp` + `strlen` + `osp_string_compare` | tick 23%, resize 31%, render 7% | Emit `osp_string_equal` for `==` and `!=`: equal pointers are equal; otherwise one pass over the bytes with no length lookup. Against a literal, pass the literal's length, known at compile time |
| 2 | **`listGet (filterList (xs, p)) 0` scans the whole list and builds a list.** Emitted as: closure environment `osp_alloc_tagged`, `osprey_list_builder_new_of`, a loop over every element (`osprey_list_get`, indirect call, `osprey_list_builder_push_borrowed`), `osprey_list_builder_seal`, `osprey_list_in_bounds`, `osprey_list_get` | IR of `ui::Machines::knob` (143 lines for a one-line function) | `ui::Lists::find`, `RenderInputs::parameter`, `Graphs::node`: tick 36%, render 23% + 4% | Recognise the shape, or add a `listFind` built-in: a loop that returns at the first match, with no list and no builder. (Stopping early while still building the one-element list was tried and rejected above; the list is the cost) |
| 3 | **A lambda written at a `mapList`, `filterList` or `foldList` call is still a heap closure called through a pointer.** `osp_alloc_tagged(16, 513)`, `osp_retain` of each capture, then per element `load` of the function pointer and `call i1 %r21(...)`, so `-O2` cannot inline the body | IR of `ui::Machines::knob`, lines 20 to 46 | One allocation and one release per call site per call, and an indirect call per element everywhere | When the argument is a literal lambda, emit its body in the loop (or a direct call with the captures on the stack); no heap environment |
| 4 | **Every element read in a generated loop is an out-of-line `osprey_list_get`**, which checks the bounds the loop has already established; a checked `listGet` is two calls (`osprey_list_in_bounds`, `osprey_list_get`) | IR of every `mapList`/`filterList`/`foldList` loop | 6.2% to 6.6% of leaf time in all four profiles | Read the element inline when the loop bound proves the index; or ship the runtime's fast paths (`osprey_list_get`, `osp_retain`, `osp_release`) as bitcode so `-O2` inlines them |
| 5 | **A list literal goes through a builder.** `[x]` is `osprey_list_builder_new_of`, `osprey_list_builder_push_borrowed`, `osprey_list_builder_seal`, with `osp_alloc` and `osp_alloc_tagged` | IR of `ui::Streams::step`: five builders for five one- or two-element literals | Part of `Streams::step` and `path` (tick 20%) | A literal of known length is one allocation with its elements stored directly |
| 6 | **A new list is tagged by a second locked call.** `osp_mem_set_layout` ([memory_arc.c:323](../../../src/toolchain/compiler/runtime/memory_arc.c)) takes the heap lock and looks the block up again, straight after the allocation, for every list header and node ([list_trie.h:86, 125](../../../src/toolchain/compiler/runtime/list_trie.h)) | Leaf samples | tick 4.9%, resize 2.0%, render 1.8% | Pass the layout to the allocation |
| 7 | **Every `Result` is a heap object**, and turning one error into another allocates a second. `listGet` allocates `osp_alloc_tagged(24, 1281)` for its answer; `match … Error _ => Error(…)` allocates another and retains the message literal | IR of `ui::Machines::knob`: three `osp_alloc_tagged(…, 1281)` for one lookup; function type `{ i64, i8, i8* }*` | Part of `arc_alloc_locked` + `arc_release_zero_locked`: tick 11%, render 25% | Return a `Result` by value (two registers or `sret`) when it is matched at once and never stored |
| 8 | **A pure call that does not depend on the lambda's argument is evaluated for every element.** `mapList (shapes, \shape => moved (shape, shift editor))` calls `shift editor` 400 times | `Header::placed`, tick 34.5% | See the first interface row | Hoist a call to an effect-free function out of the loop when none of its arguments mentions the lambda's parameters |
| 9 | **Retain and release are emitted for string literals.** `call void @osp_retain(i8* %r40)` where `%r40` is `@.str.174` in read-only data; the runtime probes to find it is not its own | IR of `ui::Machines::knob` | Part of `osp_retain` (tick 11.1%) | The compiler knows a literal: emit no retain or release for it |
| 10 | **`listConcat`, `flatten` and `mapList` are never fused.** `listConcat [head] (listConcat (Lists::flatten (mapList (steps, step))) …)` allocates each intermediate list | `ui::Streams::path` (stream.ospml:76-78) | tick 10% own plus `Lists::spanned` 17% | Lower nested concat, flatten and map into pushes on one builder |
| 11 | Dropping a list of records drops each element through the registry (`arc_drop_child`) | Leaf samples | tick 16.8%, resize 11.1%, render 18.2%: the largest single leaf in three of four profiles | Already open above ("Direct element drops for proved lists"); this round says it is the first runtime fix to make |
| 12 | A record update allocates and copies | Already open above | | In-place reuse of a unique record |

Order by what it buys across all four profiles: 11 (drops), 1 (string equality), 2 (find), 7 (`Result` by value), 3 (inline lambdas), 4 (inline element reads), then 5, 6, 9, 10, 8.

- [x] 1. String compare decides at the first unequal byte with no length lookup (`osp_string_compare`, [string_runtime.c:63](../../../src/toolchain/compiler/runtime/string_runtime.c)); only strings equal up to a NUL take the two locked lengths. No new entry point was needed, so `==`, `!=`, ordering, list search and structural equality all gain. Timeline tick: `osp_mem_string_length` + `osp_string_compare` + `memcmp` 23% to 1.4% of samples. Still open: equal strings (one hit per search) take the slow path; a literal's length known at compile time would remove it.
- [ ] 2. First-match search with no list for `listGet (filterList …) 0`. Done by hand in `ui::Lists::find`; the compiler still builds the list at every other site (`RenderInputs::parameter`, `Graphs::node`, `CardArts::fixed`).
- [ ] 3. Literal lambdas at built-in loop sites emitted inline, no heap environment.
- [ ] 4. Inline element reads in proved loops, or runtime fast paths as bitcode.
- [ ] 5. List literals as one allocation.
- [ ] 6. List layout passed to the allocation; `osp_mem_set_layout` off the allocation path.
- [ ] 7. `Result` returned by value when matched at once.
- [ ] 8. Loop-invariant effect-free calls hoisted out of lambdas.
- [ ] 9. No retain or release on string literals.
- [ ] 10. Concat, flatten and map fused onto one builder.
- [ ] Each of these lands with a regression program in `src/osprey/tests/regressions/` and the before and after of `make daw-profile`.

### Build time, 21 Nov 12:40 [DAW-PERF-BUILD]

Building the daw must take seconds. Measured with `osprey src/osprey/examples/daw --compile --memory=arc --target=macos` and a 1 ms `sample` of the compiler.

- [x] The redundant-annotation analyser ran in every build and re-inferred the whole program once per proof: `--check` took 4 min 30 s. It is now its own pass (`--analyse`, and `--warnings-as-errors`), never part of a plain build (cb7515505): `--check` 21 s, full compile 50 s.

Where the 50 s go now (36,400 samples, then `clang`):

| Phase (function) | Time | Cause | Fix |
|---|---|---|---|
| Code generation: `osprey_codegen::lower::compile_module` | 18.6 s | 7.2 s of it is `aggregate::gen_generic_record` → `closure::specialized_named_cell` → `closure::source::emit_closure_keyed`, and 3.9 s is `genfn::try_inline` → `monofn::specialize_values` → `closure::lower_fn_body`: a function or closure is lowered again for each place that names it with the same types | Keep one lowered body per (function, type arguments) and reuse it |
| `clang` on the emitted IR | about 14 s | One translation unit, optimised whole | Emit per module and compile in parallel; cache objects by IR hash |
| Type check: `osprey_types::check::driver::check_and_publish` | 8.4 s | `osprey_ast::lower_static::hygiene::resolve` 2.0 s, `check_data_contracts` 2.3 s, `effect_rows::solver::converge` 1.0 s | Profile each on its own; none is a single loop |
| Unused-binding warnings: `osprey_types::unused::unused_symbols` | 4.7 s | Calls `infer_checked`: a second whole-program inference, only to learn method targets and constructors the gate's solve already had | Keep the source solve from `staging::lower_proved` in `CheckedProgram` and read it here |

- [ ] One lowered body per function and type arguments in code generation.
- [ ] `unused_symbols` reuses the gate's solve.
- [ ] Objects compiled per module, in parallel, cached by hash.
- [ ] Target: a full daw build under 10 s, an unchanged rebuild under 2 s.
