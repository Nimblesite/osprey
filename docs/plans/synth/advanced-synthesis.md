# Advanced synthesis implementation plan [SYNTH-ADVANCED]

Build the advanced synthesis ecosystem on `@osprey/synth`: independent, composable DSP modules that can be routed, modulated, automated and combined in Prism, Light Orchestra and future hosts. Prism is the first complete integration, not the definition of the library. Every module must produce real audio; a name in the palette or an attractive waveform is not implementation.

The authoritative contracts are [Synthesis.md](../../specs/synth/Synthesis.md), especially [SYNTH-GRAPH], [SYNTH-SIGNALS], [SYNTH-RENDER] and [SYNTH-DSP]. The [research catalogue](../../research/synthesis/README.md) supplies sources and comparison experiments. The existing [Prism routing plan](prism-routing.md) remains responsible for host lifecycle, live replacement, VST integration and release readiness; this plan adds the advanced DSP families and their complete user journeys.

**Current instruction (21 November 2026):** testing has resumed. Every change comes with loaded journey tests that run under GC and ARC; mark an item done only with recorded evidence, and never mark an unverified feature production-ready.

## Current state and delivery order [SYNTH-ADVANCED-ORDER]

Existing code provides oscillator/envelope/filter primitives, typed graphs, parameter descriptors, Signals, reference block processing, VST transport and Prism's modular editor. Advanced synthesis is not already implemented by those foundations. The granular reference kernel now has generated models, native Osprey formulas, checked pool state, stereo rendering and Signal observation. It has passed strict static checking only; sample-timed events and a seeded periodic/jittered scheduler are implemented. A stereo Grain Cloud graph factory, retained memory and Prism palette/parameter/keyboard integration are implemented. Independent uniform parameter distributions now compose with the onset scheduler in the library; native validation of distribution controls, application sample-file loading, dedicated source/scheduler graph nodes, modulation adapters and audio/UI verification remain outstanding.

Deliver in this order, finishing a playable vertical slice before beginning the next large family:

- [x] Move all eleven production synth math modules to native `.ospml` and check properties derived from their source [SYNTH-DSP-PROOFS-PRODUCTION]. Commit `4f8ad8a14` removes the eleven production Lean inputs. The synth suite passed all 70 cases on both GC and ARC. The synthesis spec records the scope of each theorem.
- [ ] Establish shared resource, state, modulation and scheduling contracts under [SYNTH-ADVANCED-CORE]. Done 21 Nov: typed `Fault`s, the `Lifecycles` transitions and `ResourceIdentity` hashes. Open: removing the fixed-demo `Voice` (its last caller is the Prism key preview), unlimited channels, polarity and rate adapter nodes, mixed note, parameter and trigger events in one frame, and the realtime render-path rules (preparation outside the callback, no allocation or locks).
- [x] Deliver granular playback, scheduling and a playable Prism grain-cloud patch under [SYNTH-ADVANCED-GRAINS]. Grain cloud → downmix → output plays; `sampleFileJourney` (fbe2dd1ab) covers WAV import refusals and stale results. Slice evidence (21 Nov): `familiesJourney` (d018db43a) renders a note through the song renderer, finite, repeatable and, where pitched, in tune at middle C by autocorrelation; the Prism module browser adds it; the project file saves its routing. Open items in its section stay open.
- [x] Deliver modular excitation/resonators and playable physical instruments under [SYNTH-ADVANCED-PHYSICAL]. Exciter → waveguide plays in tune; the codex song plays keys, chime, echoes and cello from these modules. Slice evidence (21 Nov): `familiesJourney` (d018db43a) renders a note through the song renderer, finite, repeatable and, where pitched, in tune at middle C by autocorrelation; the Prism module browser adds it; the project file saves its routing. Open items in its section stay open.
- [x] Deliver wavetable and additive instruments under [SYNTH-ADVANCED-TABLES]. Both play in tune; the codex song uses them (tablepad, glass). Slice evidence (21 Nov): `familiesJourney` (d018db43a) renders a note through the song renderer, finite, repeatable and, where pitched, in tune at middle C by autocorrelation; the Prism module browser adds it; the project file saves its routing. Open items in its section stay open.
- [x] Deliver spectral analysis, freeze, morphing and resynthesis under [SYNTH-ADVANCED-SPECTRAL]. Sine → spectral (1024) passes the note in tune; freeze, morph and phase-vocoder pitch/formant are in `SpectralFrames`. Slice evidence (21 Nov): `familiesJourney` (d018db43a) renders a note through the song renderer, finite, repeatable and, where pitched, in tune at middle C by autocorrelation; the Prism module browser adds it; the project file saves its routing. Open items in its section stay open.
- [x] Deliver nonlinear/feedback effects and expressive phase synthesis under [SYNTH-ADVANCED-NONLINEAR] and [SYNTH-ADVANCED-PHASE]. Sine → waveshaper (drive 3) and the phase operator both play in tune; the codex song uses shaper, shifter and operator patches. Slice evidence (21 Nov): `familiesJourney` (d018db43a) renders a note through the song renderer, finite, repeatable and, where pitched, in tune at middle C by autocorrelation; the Prism module browser adds it; the project file saves its routing. Open items in its section stay open.
- [ ] Deliver offline parameter fitting and learned-control adapters under [SYNTH-ADVANCED-LEARNED]. Done: `Fitting` works offline and `FitReports` records algorithm, licence and training/evaluation provenance (e971fb90f). Open: the preview/apply/reject controls in the app.
- [ ] Finish cross-platform application integration, documentation and the deferred verification gates under [SYNTH-ADVANCED-DELIVERY]. Open: Windows/browser/mobile hosts, the module reference, realtime measurements and a green `make ci` push.

A vertical slice includes the DSP module, its graph factory, live parameter mapping, Signal observation, a Prism module card/inspector and a playable patch. Each acceptance gate records its own evidence.

## Execution milestones and completion rules [SYNTH-ADVANCED-MILESTONES]

The backlog tracks implementation and acceptance separately. Keep the reference-kernel results below and record the relevant audio quality, timing or platform evidence when checking an acceptance item.

| Milestone | Dependencies | Concrete deliverable | Completion evidence |
| --- | --- | --- | --- |
| Sample-backed Grain Cloud | Existing grain kernel and Prism graph | Import a WAV, add the node, cable it to stereo output and play it from the docked keyboard | Source metadata, working controls, audible output and recoverable import failures |
| Composable granular instrument | Sample-backed Grain Cloud; event/control ports | Separate source, scheduler and player; route modulation; replace a sample without invalidating active grains | Editable cloud, rhythmic and frozen-texture patches |
| Physical instrument | Prepared resources; fractional delay; excitation; voice lifecycle | Plucked string and struck modal body assembled from reusable nodes | Tuning/decay evidence and playable patches with visible routing |
| Wavetable and additive instruments | Resource preparation; voice lifecycle; modulation | Morphing tables and independently controlled partials | Pitch-range/aliasing evidence and saved example patches |
| Spectral instrument | Prepared transforms; spectral ports; latency accounting | Shared analysis feeding freeze/morph/resynthesis | Reconstruction and latency evidence plus an editable freeze patch |
| Experimental effects and phase instruments | Causal feedback; oversampling; latency accounting | Reverb, diffusion, nonlinear effects and phase networks | Stability/quality evidence and combined-family patches |
| Offline discovery | Stable operators, parameter contracts and patch persistence | Fit parameters, preview a result, explicitly apply or reject it | Reproducible fitting job and unchanged original patch on failure/cancel |
| Production/platform readiness | Delivered modules and resumed verification | Documented capability matrix, bounded rendering and complete host journeys | Deferred acceptance gates below, including actual device audio |

Deliver the first milestone before expanding to another synthesis family. Shared contracts should be extracted as the first real consumers need them; do not postpone audible progress behind an unused general framework. Preserve the separate scheduler/player design even when Grain Cloud offers a convenient composite node.

### Next implementation sequence [SYNTH-ADVANCED-MILESTONES-NEXT]

- [ ] Finish the sample-import checklist [SYNTH-ADVANCED-GRAINS-IMPORT] and retain the built-in source as an explicit selectable resource.
- [ ] Correct keyboard pitch mapping for sample-rate metadata and root note; keep source scanning independent from pitch.
- [ ] Make the selected Grain Cloud inspector expose meaningful units and bounded controls; retain Prism's original artwork and layout.
- [ ] Complete the manual journey: add Grain Cloud, load a sample, add/connect stereo output, play several notes, edit controls, stop and remove the cable/node.
- [ ] Trace the observed audio-queue starvation through producer, transport and device receipts; record workload and timings before selecting a fix.
- [ ] Remove the measured producer bottleneck without weakening validation or masking sustained throughput failure with a larger queue.
- [ ] Demonstrate sustained playback and stop/restart manually; keep automated correctness/performance acceptance unchecked during the pause.
- [ ] Add scheduler/source nodes and typed modulation adapters, then finish granular presets before beginning physical models.

### Evidence and change discipline [SYNTH-ADVANCED-MILESTONES-EVIDENCE]

- [x] For each delivered slice, record the commit, relevant specification IDs, module paths and exact limitations beside its checklist section. Done in [SYNTH-ADVANCED-FAMILIES-CHECKPOINT].
- [x] Record compilation/static-analysis results separately from manual interaction, captured audio and automated-test results. The checkpoint separates strict checks, scratch measurements and (absent) automated tests.
- [x] Record reference/offline versus realtime capability per module; never infer realtime readiness from native compilation alone. Every advanced family is recorded as reference/offline only.
- [x] Keep failures, partial work and deferred verification unchecked; split a mixed item when only one part is complete. Mixed items stay unchecked with notes.
- [x] Commit each coherent implementation step, preserve unrelated work and avoid a push that implicitly claims deferred CI has passed. One commit per family/slice on `feat/synth-advanced`; nothing pushed.

## Shared component foundation [SYNTH-ADVANCED-CORE]

### Canonical models and module boundaries [SYNTH-ADVANCED-CORE-MODELS]

- [x] Declare configuration, prepared resources, persistent DSP state, commands, observations and faults in typeDiagram; generate their Osprey representations. Twelve schemas (`dsp`, `graph`, `grains`, `host`, `physical`, `phase`, `fitting`, `render`, `wave`, `tables`, `patch`, `spectral` .td) generate every type in `src/*_model.ospml`; no hand-written type is left (checked 21 Nov). Configs, `SampleResource`/`Wavetable`, per-family state, `ParameterChange`/`CommandReceipt`, `Observation` and `GraphFault` are all declared there; module refusals as typed faults are tracked under the typed-failures item.
- [x] Keep application models, canvas/native widgets, networking, file dialogs and operating-system audio devices outside the library. All advanced modules live in `@osprey/synth` with no UI, file, network or device imports.
- [x] Author DSP formulas in native Osprey and derive the definitions used by the proofs from those modules. PhysicalMath, TableMath, SpectralMath, NonlinearMath, PhaseMath and FitMath.
- [x] Give each component a stable kind/version and each instance an independent identity; never share mutable phase, delay memory, random state or voices between instances. `Kinds.kind` / `Kinds.version` / `Kinds.compatible` name every processor and version output-changing revisions; instances are keyed by node identity and each holds its own kernel state (no shared phase, delay memory, seeds or voices).
- [x] Define prepare, activate, render, reset, release/tail and dispose transitions, including sample-rate/block-size changes. `Lifecycles` with `Stage`, `Transition` and `Instance` in `render.td`, specified in [Synthesis.md](../../specs/synth/Synthesis.md). The clip journey "a graph instance prepares, activates, renders, releases exactly its tail…" (GC and ARC) checks every refused transition by its message, the 200-frame tail at 48 kHz (184 at 44.1, 400 at 96), that running, release and tail equal one uninterrupted render, reset restarting the opening, and re-preparation at 96 kHz: the same sine read at 48 kHz is in tune an octave down (spectral probe, 0.5 cents, −12.04 dB).
- [x] Separate immutable prepared resources from per-instance render state so multiple voices can share sample/table data safely. Sample resources and prepared wavetables live immutably in the node's `Processor`; per-instance state (phases, lines, voices, grains) lives in that node's kernel memory, so voices and instances read shared data without sharing mutable state.
- [x] Expose typed failures for invalid configuration, exhausted capacity, missing resources, incompatible layouts and numerical failures; never fabricate a successful silent result. Every module refusal is a `Fault` union (`dsp.td`: `InvalidConfiguration`, `ExhaustedCapacity`, `MissingResource`, `IncompatibleLayout`, `NumericalFailure`) carrying its sentence, read with `Faults.detail`; builtin conversions go through `Outcomes.whole` as `NumericalFailure`. Specified in [Synthesis.md](../../specs/synth/Synthesis.md). Tests assert class and exact text with `refusedAs` (configuration: lifecycle settings; layout: a two-output graph; resource: a 4-byte WAV; capacity: transposition past the grain rate limit; numerical: NaN, ±∞ and 10⁶ graph output). Synth suite 63/63 under GC and under ARC (21 Nov); the DAW converts at its display boundary.
- [ ] Remove superseded fixed-demo DSP after callers migrate; do not retain parallel implementations that drift.

### Ports, parameters and automation [SYNTH-ADVANCED-CORE-PORTS]

- [x] Declare audio, control and timestamped-event ports, channel layouts and physical units for every new component. `Port` (graph.td) carries rate, channels and unit; the synth test "every component declares coherent port rates…" checks 32 components: audio is amplitude on 1+ channels, control is one normalised channel, events are one channel of notes, names unique per side (50/50 ARC).
- [ ] Unlimited channels [SYNTH-GRAPH-CHANNELS]: declare `ChannelLayout` (ordered named channels, stereo default, 5.1/7.1/7.1.4/any N) in `graph.td`; every port carries one.
- [ ] **[DAW-MODULAR-LAYOUT]: every track carries two channels.** Done: a patch's stereo output keeps its left and right to Master, a mono output and every built-in voice sit in the centre at equal power, and track pan is a balance with unity at the centre. Open: layouts other than stereo, `Grains` stereo summing and the `GraphClips` mono-to-stereo component.
- [x] Add Split, Join, Route (channel matrix) and Mix (per-input gain) graph components and Prism cards; an any-layout equal-power panner whose gain formula lives in native Osprey. Prism’s picker inserts all five cards and preserves their ports through Undo/Redo. The panner and mixer retain their width and settings through project save/reopen. The prepared graph currently accepts 1–64 channels.
- [ ] Track and Master output layouts in the project file (`daw.td`); sample import keeps every source channel; the macOS device queue plays any layout CoreAudio offers.
  - [x] WAV import preserves each PCM word in channel order for the supported mono and stereo layouts. Swapping the left and right channels changes the resource hash. The mono grain consumer averages each pair once at preparation.
- [x] Verify a 7.1 patch through Split, Join, Route and Mix, and reject a stereo-to-5.1 cable without Route. On 21 November 2026 the two channel journeys in [the synth suite](../../../src/osprey/packages/synth/tests/main.ospml) passed under GC and ARC. The mixer receives eight distinct channels in forward and reversed order. The test checks all 512 output samples against independent gains of 0.25 and 0.75.
- [ ] Define explicit adapters for hertz/normalised values, bipolar/unipolar signals and control/audio rates. Partial: hertz/normalised is `Descriptors.fromControl`/`toControl` (logarithmic for positive frequencies and times), tested on every parameter of 24 components: ends, nine-point round trip, range and monotonicity (synth 49/49 ARC). Bipolar/unipolar exists only inside the LFO (`Waves.unipolar`); there are no adapter graph nodes for signal polarity or control/audio rate.
- [x] Declare names, units, defaults, minimum/maximum values, linear/logarithmic scaling, modulation semantics and automation eligibility for every parameter. Descriptors carry key, label, unit, default, bounds and `automatable`; `Descriptors.scale` declares logarithmic scaling for positive frequency/time ranges (probe: 220 Hz → control 0.3447 → 220 Hz), and `Descriptors.modulation` declares each parameter's inlet rule.
- [x] Define modulation as explicit additive, multiplicative or replacement operations; reject ambiguous multiple writers unless a combiner is present. `ModulationRule` is `Unmodulated`, `Replacing`, `Multiplying` or `Exponential` per parameter; each inlet accepts exactly one cable (`OccupiedInput`), so multiple writers need an explicit mixer.
- [x] Add reusable smoothing/slew components with declared response and state continuity; avoid hiding unrelated smoothing inside each module. `Smoothers` (one-pole: 0.632 after τ; linear: full scale in `time`, measured 0.5 at τ/2) and the `smoother` graph node.
- [ ] Deliver sample-offset events in deterministic order, including simultaneous note, parameter and trigger events. Partial: same-frame parameter events apply in order and nine block splits render identical samples (synth test "same-frame automation…", 47/47 ARC); note on/off ownership across exact partitions is the daw MIDI timing test. Trigger events and mixed note+parameter frames remain unproven.
- [x] Define which changes apply at a sample boundary, block boundary or prepared-resource swap; expose pending versus applied values. Defined in [Synthesis.md](../../specs/synth/Synthesis.md): parameter events apply before their exact sample; topology and resource replacement (`Repatch.apply`) publish at a block boundary; `Commands` receipts are `Pending`, `Applied(revision)` or `Rejected(fault)`. Tests: same-frame automation with nine block splits, "replace a playing graph without resetting phase", and the controller-write journey (synth 51/51 ARC).
- [x] Preserve surviving state across valid graph changes, and keep the previous running graph if preparation fails. `GraphRebase.rebased` carries surviving components' memory into an edited graph (layout and storage must still fit), starts new components silent and drops removed ones; an invalid edit returns an error and the caller keeps its previous graph and state. Probe: inserting a reverb into a running sine → gain graph continues rendering with the oscillator phase unchanged (0.16667); without rebasing the edit is rejected. New grain clouds still need a restart.

### Resources and bounded rendering [SYNTH-ADVANCED-CORE-RESOURCES]

- [x] Define sample/table/impulse resources with identity, content hash, channel count, sample rate, length and version. `Resources.sample` and `Resources.table` derive a `ResourceIdentity` (`graph.td`): name, 64-bit FNV-1a hash over every sample word and its metadata, channels, rate, frames and version; an impulse response is a sample resource. `Resources.matches` refuses a changed or renamed resource as `MissingResource`. The wave journey "sample, impulse and table resources carry a content identity…" checks ten variants hash distinctly (one bit, -0.0, rate, channels, root, order, length, empty), frame counts for five layouts and four tables (GC and ARC).
- [ ] Decode files, prepare tables, allocate pools and construct transforms outside the audio callback.
- [x] Set explicit limits for voices, grains, modes, partials, FFT sizes, delay memory, automation queues and resource bytes. Voices ≤ 16, grains ≤ 256, modes 8, partials 64, FFT 64–4096, fitting ≤ 16 parameters/10,000 evaluations, delay storage ≤ 1,920,000 samples.
- [x] Define a capacity policy per resource: reject, steal with a release, or drop with a diagnostic; do not allocate on exhaustion. Grains reject new onsets (counted); `StringVoices` steal with a release (counted); spectral nodes over the per-graph budget and graphs over the storage budget are rejected before allocation; fitting rejects jobs over 16 parameters or 10,000 evaluations.
- [x] Specify source-boundary behaviour separately for looping, one-shot, reverse playback and interpolation. Specified in [Synthesis.md](../../specs/synth/Synthesis.md) [SYNTH-ADVANCED-GRAINS-BOUNDARY]: loop wraps (tap after the end is the region start), one-shot stops (tap repeats the last frame, the grain ends outside), reverse is a negative rate, interpolation is linear. `regionJourney` in grain_journeys.ospml checks loop/one-shot × forward/reverse/held. Reflect is not implemented.
- [ ] Avoid allocation, blocking locks, filesystem/network access, logging formatting and observer callbacks in production render paths.
- [x] Define denormal handling, nonfinite-input policy, maximum internal levels and output headroom; do not silently clip intermediate graph signals. Denormals: every feedback loop (waveguide, bowed, resonator/modal, reverb, mesh) flushes state below 1e-30 to zero (`PhysicalMath.flushed`; probe: a 10 ms resonator reaches exactly 0.0). Nonfinite or |x| > 1e6 node outputs are rejected by `RenderInputs.sample`; intermediate signals are never clipped; module docs state headroom (unnormalised sums for voices/grains, peak-safe additive).
- [x] Declare algorithmic latency and tail length so preparation can align parallel routes and dispose of tails correctly. `Timings.latency` / `Timings.tail` declare samples at 48 kHz for every component from its current parameters (e.g. spectral 1024/256: latency 1,023, tail 1,279; reverb T60 2 s: tail 192,000). Route alignment in preparation is not yet automatic.

### Signals and observers [SYNTH-ADVANCED-CORE-SIGNALS]

- [x] Publish applied parameter revision and audio sample position with each coherent observation. `Observation` carries `revision` and `samplePosition`; `Renderer.observed` and `Grains.observation` stamp both, and selected views keep them.
- [x] Expose active voices/grains/modes, envelope state, meters, resource readiness, capacity pressure and typed diagnostics as appropriate to each family. Grains: `Grains.observation` (active, capacity, requested/accepted/rejected). Advanced families: `Activities.strings` (sounding/capacity/stolen/energy; probe 5 notes on 4 voices → 4/4, 1 stolen), `modes` (ringing slots and energy; probe 5 of 8 bell modes ringing), `spectral` (frames, bins, freeze held) and `reverb` (stored energy); meters come from `Renderer.observed`; diagnostics are the typed errors each module returns.
- [x] Keep `Signal<T>` observation distinct from audio-rate DSP streams; observers must not execute on the audio thread. Advanced modules return samples and state only; observation is the separate `Observations`/`Grains.observe` Signal path, and `SpectralStreams.view` is an explicit off-thread snapshot.
- [x] Publish through bounded storage; let slow subscribers coalesce snapshots without blocking rendering. `SnapshotSlots` holds only the latest snapshot: `publish` replaces it in constant storage and `read` returns the newest plus how many were skipped. Probe: five publications before a slow read → revision 5 with 4 coalesced; the next read → revision 6, 0 coalesced.
- [x] Support selected-field subscriptions and independent observations of multiple same-kind instances. `Observations.selected` / `subscribed` filter a snapshot to chosen nodes and keys, keeping its revision and sample position; entries are keyed by node identity, so two sine instances read independently (probe: node 1's frequency 220 Hz selected from two oscillators with revision 7, position 4,800 retained).
- [x] Make UI/controller writes typed commands; show whether a request is pending, applied or rejected. `Commands.pending` / `resolve` turn a typed `ParameterChange` into a `CommandReceipt` of `Pending`, `Applied(revision)` or `Rejected(fault)`; probe: a valid edit is applied as revision 1, a stale revision and an out-of-range value are rejected.

## Prism and external-controller experience [SYNTH-ADVANCED-PRISM]

- [ ] Preserve the existing Prism logo, original node styling, visible cables, inspector and bottom-docked closable keyboard.
- [ ] Group module browsing by Sources, Grains, Physical, Spectral, Modulators, Effects, Utilities and Output; support search and drag/click insertion. Partial: the native picker lists all 29 kinds grouped in that category order with "Category · Title" labels; search and grouped canvas-palette browsing remain open.
- [ ] Give every inserted node real ports, useful defaults and an inspector generated from its descriptors.
- [ ] Make cable creation/removal, node movement/removal and Undo/Redo work for every advanced family.
- [ ] Show resource-loading and failure states on the affected node; avoid treating unavailable samples/models/plugins as a silent working instrument.
- [ ] Bind controls to applied parameter Signals and show pending/rejected changes distinctly.
  - [x] Play Patch applies an accepted gain edit on the following device block. The same running audition reaches exact silence at zero and sounds again at 0.5 under GC and ARC.
  - [x] Song playback reads accepted Prism patch edits between device blocks. A held note follows gain and pitch changes, and a 17-track codex song device run confirmed that tablepad gain changed the next audio packet. See [live song controls](daw.md#slider-drag-lag-daw-mix-drag).
- [ ] Provide macro controls that map to ordinary typed parameters with declared ranges and smoothing. Library part done: `Macros` validates and writes normalised amounts onto declared parameters through per-target ranges and curves; hosts smooth amounts with `Smoothers`. Prism controls remain open.
- [ ] Expose the same control commands to Light Orchestra/phone adapters without adding networking or session state to the synth library.
- [ ] Save complete patches, resources, seeds, module versions, automation and controller mappings; load incompatible/missing resources explicitly.
- [x] Keep advanced modules available to headless/offline consumers without requiring Prism or the native UI package. Evidence: `src/osprey/examples/daw` builds from the synth and JSON source roots only and renders strings, modal banks, operators, the reverb and the spectral freeze from a command line.

## Arrangements and the daw [SYNTH-ARRANGEMENT]

Added on request after the families: a piano-roll project file rendered offline by the advanced modules, and a command-line player that continues an existing recording. Nothing here is covered by an automated test yet; the evidence below is from rendering the included song.

- [x] Declare the arrangement (tempo, rate, master drive, reverb space, sustain appearances, tracks, notes, six instruments) in `src/osprey/examples/daw/daw.td`; formulas in its `arrange_math.ospml`.
- [x] Render one note per instrument from the existing modules (`NoteRenders`): plucked and bowed strings, bell and bar modal banks, a detuned phase-distortion operator pair and a drum; each module validates its own controls.
- [x] Hold the end of a recording with the spectral freeze and replay the hold at a pitch ratio with its own rise, hold and fade (`Sustains`) [SYNTH-ARRANGEMENT-SUSTAIN]. The freeze gained identity phase locking: every bin turns with its nearest magnitude peak.
- [x] Decode the JSON project in one boundary module (`SongFiles`) into the typed arrangement [SYNTH-ARRANGEMENT-FILE]; malformed JSON, unknown instruments and malformed notes are errors.
- [x] Command-line player `src/osprey/examples/daw` and `make -C src/toolchain/targets/macos daw-render` and `daw-play` [SYNTH-ARRANGEMENT-PLAYER]: renders, writes WAV parts through `xxd` and joins them after the intro with `ffmpeg`; `daw-play` plays with `afplay`; every host command runs under `timeout`.
- [x] A song project written to continue the Broken Circuitry recording (first a melodic version: 44 bars, 14 tracks, 2,074 notes; now `phosphor-drift.json`, below, 2,904 notes).
- [x] Routed instruments (`PatchVoices`) [SYNTH-ARRANGEMENT-PATCH]: a note builds a graph from named modules and cables, with settings that follow pitch or velocity, and renders through `Renderer`. The mono renderer rejects LFO routing and granular; `spectral` and `reverb` nodes are too slow per note.
- [x] `phosphor-drift.json` ("Phosphor Drift", a new piece, not part of Broken Circuitry): 40 bars, 17 tracks (8 routed patches), 8 sustain appearances; rendered at 48 kHz after the recording, level across the join 0.18 → 0.13 RMS, sides within 0.2 dB.
- [x] The project file names its own intro (`Intro`: recording, tail, crossfade) in `daw.td`; the player only renders, and `daw-render` / `daw-play` render and play [SYNTH-ARRANGEMENT-PLAYER].
- [x] The daw (`src/osprey/examples/daw`, formerly `songs`) owns the project model, `daw.td`, and the arrangement modules; `@osprey/synth` keeps only the synthesis modules.
- [ ] Automated tests for `NoteRenders`, `Arrangements`, `Sustains`, `SongFiles` and the player (paused with the other suites).
- [ ] Bandlimited sustain transposition, an oversampled or look-ahead limiter and stereo instruments. Partial: `synth::Limiters`, a linked true-peak (four-times oversampled) lookahead brick-wall, is done as a module. It belongs only where the user places it. The engine must never put it on Master by itself ([DAW-TRANSPARENCY]). Bandlimited sustain transposition and stereo instruments remain.
- [ ] Playback on Windows and Linux; `afplay` is macOS only.
- [ ] Detect an intro's tempo and key instead of measuring them by hand.

### Every processing point is a pluggable effect [SYNTH-ARRANGEMENT-EFFECTS]

Deferred until #427 is merged and the user approves; the repository is in stabilisation mode. Every place in the synth and the daw that processes a signal becomes an operation of an Osprey effect, so a handler supplies the processing. Examples are summing onto a bus, gain, pan, envelopes, filters, waveshaping, delays and the master stage. The default handler calls today's kernels (`ArrangeMath.bused`, `panLeft`, `panRight`, `enveloped`, `synth::Limiters` and others), so rendered output stays identical and the production proofs in [SYNTH-DSP-PROOFS](../../specs/synth/Synthesis.md) still describe the default.

An analog summing handler replaces the plain weighted sum at the mixing operation. A console's nonlinear stages sit in fixed places: a gentle curve on each channel before the sum, and a summing-amplifier and transformer curve on the bus after it. Placement matters because saturation does not commute with addition (sat(a) + sat(b) ≠ sat(a + b)): only bus saturation makes sources intermodulate. The handler models:

- channel and bus saturation, slightly asymmetric for even harmonics, built from the antialiased `Waveshapers` and the oversamplers;
- band limits: a one-pole high-pass near 10–20 Hz (coupling capacitors and transformers) and a low-pass near 30–80 kHz (amplifier bandwidth);
- noise that rises with the number of summed channels (op-amp noise gain), seeded for deterministic renders;
- channel gain tolerances near ±1 % and small crosstalk;
- rail clipping and slew limiting when the bus is driven hard.

Each drive defaults to zero, which reproduces the clean sum exactly. The linear core keeps the mixing proofs (`mixSuperposition`, `mixPower`, `panEqualPower`); every nonlinear stage needs its own proof, as the waveshaper stages already have.

- [ ] List every processing call site in `@osprey/synth` and the daw, and declare one effect per kind of processing.
- [ ] Route each call site through its effect with a default handler that is byte-identical to the current output on the existing goldens.
- [ ] Write the analog summing handler, with a test that sat(a) + sat(b) differs from sat(a + b) and that zero drive equals the plain sum.

## Deferred acceptance checklist [SYNTH-ADVANCED-VERIFY]

Testing resumed on 21 November 2026. Record results for the stated workload, backend and host before checking each gate.

### Correctness and stability [SYNTH-ADVANCED-VERIFY-CORRECTNESS]

- [ ] Run loaded end-to-end patch journeys with repeated same-kind instances, fan-out, modulation, removal, undo, save/load and failures.
- [ ] Compare arbitrary block partitions against continuous rendering while preserving event ownership at exact boundaries.
- [ ] Exercise sample rates 44.1/48/96/192 kHz and supported block sizes, including unequal and one-sample partitions.
- [x] Verify grain windows, wrapping, reverse playback, deterministic schedules, pool exhaustion and resource replacement. The GC and ARC suites exercised rendered Hann and ASR envelopes, reverse and fractional-rate loop crossings, seeded onset replay, rejection counts at capacity and immutable source replacement while an old grain finishes. [Window rendering](../../../src/osprey/packages/synth/tests/main.ospml) · [Loop and capacity tests](../../../src/osprey/packages/synth/tests/grain_limits.ospml) · [Source replacement](../../../src/osprey/packages/synth/tests/grain_journeys.ospml)
- [ ] Verify physical-model tuning, decay/energy bounds, rate changes and extreme stable parameter values.
- [ ] Verify wavetable/additive spectral behaviour, phase continuity, modulation and partial/table replacement.
- [ ] Verify spectral reconstruction, overlap/add normalisation, latency, freeze/release and transport discontinuities.
- [ ] Compare nonlinear/phase outputs with independent higher-quality references under level/latency matching.
- [ ] Inject nonfinite data, malformed resources, invalid units, stale revisions, overflow and unavailable resources; require explicit failure and preserved prior state.

### Realtime and observer behaviour [SYNTH-ADVANCED-VERIFY-REALTIME]

- [ ] Instrument allocation, locking and unbounded operations in the compiled native callback path.
- [ ] Measure worst observed callback time, queue starvation, CPU, memory and latency on named hardware with declared workloads.
- [x] Exercise simultaneous grains, modes, spectral transforms, feedback and VST nodes under explicit resource budgets. The combined preset routes its grain cloud, modal bank, spectral transform and feedback reverb through a hosted effect and verifies its output. The same journey rejects out-of-range blocks and excessive delay storage.
- [ ] Verify slow/detached Signal subscribers never stall rendering and observations retain coherent revisions/sample positions.
- [ ] Verify live graph/resource swaps, state transfer, crossfade policy, tail disposal and failure rollback.
- [ ] Separate reference/offline availability from verified realtime capability in public descriptors and documentation.

### Application and platform evidence [SYNTH-ADVANCED-VERIFY-PLATFORMS]

- [ ] Run headless and windowed native journeys that add, connect, edit, play and remove the advanced modules.
- [ ] Inspect screenshots against Prism's main-branch visual style; retain original artwork and verify new nodes follow it.
- [ ] Verify actual mouse dragging, cable deletion, keyboard docking, window resizing and accessible controls.
- [ ] Capture actual audio output and level-matched listening examples for each family; include source patch/resource/seed metadata.
- [ ] Verify desktop macOS and Windows integrations, documenting device/backend differences.
- [ ] Verify browser and mobile builds through their supported hosts; explicitly report unsupported modules, resource limits and latency instead of silently substituting another algorithm.
- [ ] Complete static analysis, package coverage, required CI preparation and a coordinated validated push; inspect the resulting CI.

## Completion and handoff [SYNTH-ADVANCED-DELIVERY]

- [ ] Document each module's ports, units, scaling, defaults, state/reset semantics, latency, capacity and supported rates/channels.
- [x] Record the source paper/reference and exact algorithm/version for each research-derived implementation.
- [ ] Link code, specifications, patches and eventual test evidence through the section IDs in this plan.
- [x] Include reproducible example patches for every delivered DSP family and a combined patch. `Patches.combined()` routes seeded grains through a downmix, modal body, spectral processor, waveshaper, LFO-controlled gain and reverb. The GC and ARC journey checks the emitting grain, physical, spectral, modulated and output stages after a 1,024-frame render. [Patch factories](../../../src/osprey/packages/synth/src/patches.ospml) · [Combined journey](../../../src/osprey/packages/synth/tests/clip_journeys.ospml) · [Single-family journeys](../../../src/osprey/packages/synth/tests/spectrum_sources.ospml)
- [ ] Mark experimental/production status from measured evidence; record remaining limitations without weakening the contracts.
- [ ] Reconcile this checklist with [Synthesis.md](../../specs/synth/Synthesis.md) and the [Prism routing plan](prism-routing.md), preserving their unfinished requirements.
- [ ] Commit work in reviewable module-sized steps, preserve unrelated unfinished work and remove only task-owned temporary processes/artifacts.

### Granular producer checkpoint [SYNTH-ADVANCED-GRAINS-PRODUCER]

The native audition exposed queue-starvation observations. `Grains.scheduled` now owns both event application and block rendering so its private sample loop can rely on one entry validation of pool/voice/clock invariants. Single-sample `next` and external `trigger` calls still validate independently, every source tap still checks its actual sample, and failed events preserve the caller's immutable state. The redundant separate `GrainTimeline` module was removed. Strict app checking and native archive/host builds passed; a bounded static invariant review found no concrete defect. No timing improvement or zero-starvation claim is made. The revised executable is installed for the next launch; the open user-edited patch was preserved without restarting it.

Implementation checkpoint [SYNTH-ADVANCED-GRAINS-WINDOWS]: generated `GrainWindow` values select Hann or linear ASR. ASR fractions must be positive and sum to at most one; endpoint guards return zero. Grain Cloud exposes shape, attack and release descriptors, decoded without rounding; active grains retain their immutable window. Strict compilation passed; numerical/audio verification remains outstanding under the current testing pause.

Implementation checkpoint [SYNTH-ADVANCED-GRAINS-TIMING]: generated `GrainTiming` selects periodic jitter or exponential gaps rounded up and capped to 1–1048576 samples. The existing Park–Miller stream advances once per onset for both modes; changing the policy preserves the already scheduled onset. Graph descriptors expose timing selection, underlying mean and maximum gap, and cloud Signals report the applied policy. Package and application strict compilation passed; distribution/audio execution is still unverified.

Implementation checkpoint [SYNTH-ADVANCED-GRAINS-OVERLOAD]: `Grains.trigger` returns a generated `GrainAdmission` receipt. Full pools reject only the new request and retain existing voices; timeline rendering consumes the event and continues. Request revision counts valid requests, accepted identity counts started grains, and their difference counts rejection. Observations report counts and lifetime-average events per output frame (zero before any frame has elapsed); hosts convert to events/second using their sample rate. The policy never steals. Strict library/application compilation passed; rendered overload verification remains pending.

Implementation checkpoint [SYNTH-ADVANCED-GRAINS-DISTRIBUTIONS]: generated bounds/seeds/event models and `GrainDistributions.events` produce checked uniform parameter variations. `GrainClouds.variedBlock` commits the existing scheduler, renderer and all variation streams together; rejected onsets consume draws while invalid operations return no partial state. Static compilation passed; graph/UI exposure and numerical/audio evidence remain pending.

Integration checkpoint [SYNTH-ADVANCED-GRAINS-DISTRIBUTIONS]: `GrainCloudState` now owns variation seeds, and graph rendering uses `GrainClouds.variedBlock` with bounded `GrainSpreads` ranges. Five zero-default widths preserve existing centers; the variation seed initialises only during graph preparation. Library/application strict compilation passed. The running app has not been rebuilt for this change, and native interaction/audio evidence remains pending.

### Advanced family checkpoint [SYNTH-ADVANCED-FAMILIES-CHECKPOINT]

Recorded 20 November 2026 on `feat/synth-advanced`. Strict package and Prism source checks passed for each commit; **no test suite ran** (testing pause). Evidence comes from scratch measurement programs outside the repository at 48 kHz, rendering through the library modules and the reference graph renderer. Every family is reference/offline only: list-based state, per-sample allocation and, for spectral and reverb nodes, milliseconds per sample in the graph. None is realtime-ready; all deferred verification items above remain open.

| Family | Commits | Modules | Measured evidence | Known limits |
| --- | --- | --- | --- | --- |
| Physical/modal [SYNTH-ADVANCED-PHYSICAL] | 834b11fe2, 1ac08ca13 | `Exciters`, `DelayLines`, `LossFilters`, `Allpasses`, `Resonators`, `ModalBanks`, `Waveguides`, `PhysicalKernels` | Waveguide 440 Hz with dispersion 0.3 and pickup 0.2 measured 439.998 Hz; T60 = 1 s gives −54.0 dB at 0.9 s (brightness 1); modal string 220.008 Hz with exact T60 | No bowed/blown model, voice allocation, sample-rate state transfer or mesh; linear fractional delay; time-varying direct-form coefficient changes not proven transient-free |
| Wavetable/additive [SYNTH-ADVANCED-TABLES] | 1ac08ca13 | `Wavetables`, `PartialBanks`, `TableKernels` | Wavetable 100, 1000 and −440 Hz exact, peak ≤ 0.921; graph wavetable 261.626 Hz; additive 330.03 Hz | Table changes and modulation are parameters, not typed ports; no table import (needs FFT resynthesis); level switching is not crossfaded |
| Spectral [SYNTH-ADVANCED-SPECTRAL] | e184932a9, 31b65e220 | `Ffts`, `SpectralStreams`, `SpectralFrames`, `SpectralKernels`, `Sequences` | Pass-through error 4e-16 at latency size − 1; band rejects an out-of-band tone to 5.8e-6 rms; phase-vocoder freeze holds a 1 kHz capture at 0.3537 rms within 3.7e-4 | Shift is integer-bin; no pitch/time, shared analysis, spectral ports, tail flushing or CPU budget |
| Nonlinear/feedback [SYNTH-ADVANCED-NONLINEAR] | 31b65e220, 2ba9fa938 | `Waveshapers`, `Oversamplers`, `Reverbs`, `Shifters`, `EffectKernels` | Hard clip at 5156 Hz, drive 8: alias/total energy 5.4% naive, 1.2% ADAA, 0.35% 2×, 0.19% 2× + ADAA; FDN T60 2 s measured −8.7 dB over 0.3 s; freeze energy within 3%; shifter ±351.6 Hz on the expected bin, other sideband −46.5 dB | 2× only; delay/matrix not separate graph nodes; no pitch-shifted feedback |
| Phase [SYNTH-ADVANCED-PHASE] | 6ebc380ac, 2ba9fa938 | `PhaseOperators`, `OperatorKernels` | WPPM (d 0.5, I 0) and PD (d 0.5) are pure sines; windowed PM with R 1.3 stays harmonic | Not antialiased |
| Learned/offline [SYNTH-ADVANCED-LEARNED] | a7f657df4 | `Fitting` | Waveguide started at 250 Hz fits a 330 Hz target to 329.93 Hz (loss 21.7 → 0.048, 49 evaluations, seed 12345) | Spectral objective only; no provenance records, neural components or application preview UI |
| Example patches [SYNTH-ADVANCED-DELIVERY] | ee2d13f1f | `Patches` | All eight patches render 4,096 samples with peak ≤ 0.5; the combined patch runs through `GraphClips` | Prism palette cards, observation Signals and saved patch files for these families are not built |
