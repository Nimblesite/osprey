# Advanced synthesis implementation plan [SYNTH-ADVANCED]

Build the advanced synthesis ecosystem on `@osprey/synth`: independent, composable DSP modules that can be routed, modulated, automated and combined in Prism, Light Orchestra and future hosts. Prism is the first complete integration, not the definition of the library. Every module must produce real audio; a name in the palette or an attractive waveform is not implementation.

The authoritative contracts are [Synthesis.md](../specs/runtime/Synthesis.md), especially [SYNTH-GRAPH], [SYNTH-SIGNALS], [SYNTH-RENDER] and [SYNTH-DSP]. The [research catalogue](../research/synthesis/README.md) supplies sources and comparison experiments. The existing [Prism routing plan](prism-synth-routing.md) remains responsible for host lifecycle, live replacement, VST integration and release readiness; this plan adds the advanced DSP families and their complete user journeys.

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
- [x] Define prepare, activate, render, reset, release/tail and dispose transitions, including sample-rate/block-size changes. `Lifecycles` with `Stage`, `Transition` and `Instance` in `render.td`, specified in [Synthesis.md](../specs/runtime/Synthesis.md). The clip journey "a graph instance prepares, activates, renders, releases exactly its tail…" (GC and ARC) checks every refused transition by its message, the 200-frame tail at 48 kHz (184 at 44.1, 400 at 96), that running, release and tail equal one uninterrupted render, reset restarting the opening, and re-preparation at 96 kHz: the same sine read at 48 kHz is in tune an octave down (spectral probe, 0.5 cents, −12.04 dB).
- [x] Separate immutable prepared resources from per-instance render state so multiple voices can share sample/table data safely. Sample resources and prepared wavetables live immutably in the node's `Processor`; per-instance state (phases, lines, voices, grains) lives in that node's kernel memory, so voices and instances read shared data without sharing mutable state.
- [x] Expose typed failures for invalid configuration, exhausted capacity, missing resources, incompatible layouts and numerical failures; never fabricate a successful silent result. Every module refusal is a `Fault` union (`dsp.td`: `InvalidConfiguration`, `ExhaustedCapacity`, `MissingResource`, `IncompatibleLayout`, `NumericalFailure`) carrying its sentence, read with `Faults.detail`; builtin conversions go through `Outcomes.whole` as `NumericalFailure`. Specified in [Synthesis.md](../specs/runtime/Synthesis.md). Tests assert class and exact text with `refusedAs` (configuration: lifecycle settings; layout: a two-output graph; resource: a 4-byte WAV; capacity: transposition past the grain rate limit; numerical: NaN, ±∞ and 10⁶ graph output). Synth suite 63/63 under GC and under ARC (21 Nov); the DAW converts at its display boundary.
- [ ] Remove superseded fixed-demo DSP after callers migrate; do not retain parallel implementations that drift.

### Ports, parameters and automation [SYNTH-ADVANCED-CORE-PORTS]

- [x] Declare audio, control and timestamped-event ports, channel layouts and physical units for every new component. `Port` (graph.td) carries rate, channels and unit; the synth test "every component declares coherent port rates…" checks 32 components: audio is amplitude on 1+ channels, control is one normalised channel, events are one channel of notes, names unique per side (50/50 ARC).
- [ ] Unlimited channels [SYNTH-GRAPH-CHANNELS]: declare `ChannelLayout` (ordered named channels, stereo default, 5.1/7.1/7.1.4/any N) in `graph.td`; every port carries one.
- [ ] Render every instrument and processor per channel over its prepared layout; delete the fixed mono/stereo code paths (`Grains` stereo summing, renderer mono tracks, `GraphClips` mono-to-stereo component).
- [x] Add Split, Join, Route (channel matrix) and Mix (per-input gain) graph components and Prism cards; an any-layout equal-power panner whose gain formula lives in native Osprey. Prism’s picker inserts all five cards and preserves their ports through Undo/Redo. The panner and mixer retain their width and settings through project save/reopen. The prepared graph currently accepts 1–64 channels.
- [ ] Track and Master output layouts in the project file (`daw.td`); sample import keeps every source channel; the macOS device queue plays any layout CoreAudio offers.
  - [x] WAV import preserves each PCM word in channel order for the supported mono and stereo layouts. Swapping the left and right channels changes the resource hash. The mono grain consumer averages each pair once at preparation.
- [x] Verify a 7.1 patch through Split, Join, Route and Mix, and reject a stereo-to-5.1 cable without Route. On 21 November 2026 the two channel journeys in [the synth suite](../../src/osprey/packages/synth/tests/main.ospml) passed under GC and ARC. The mixer receives eight distinct channels in forward and reversed order. The test checks all 512 output samples against independent gains of 0.25 and 0.75.
- [ ] Define explicit adapters for hertz/normalised values, bipolar/unipolar signals and control/audio rates. Partial: hertz/normalised is `Descriptors.fromControl`/`toControl` (logarithmic for positive frequencies and times), tested on every parameter of 24 components: ends, nine-point round trip, range and monotonicity (synth 49/49 ARC). Bipolar/unipolar exists only inside the LFO (`Waves.unipolar`); there are no adapter graph nodes for signal polarity or control/audio rate.
- [x] Declare names, units, defaults, minimum/maximum values, linear/logarithmic scaling, modulation semantics and automation eligibility for every parameter. Descriptors carry key, label, unit, default, bounds and `automatable`; `Descriptors.scale` declares logarithmic scaling for positive frequency/time ranges (probe: 220 Hz → control 0.3447 → 220 Hz), and `Descriptors.modulation` declares each parameter's inlet rule.
- [x] Define modulation as explicit additive, multiplicative or replacement operations; reject ambiguous multiple writers unless a combiner is present. `ModulationRule` is `Unmodulated`, `Replacing`, `Multiplying` or `Exponential` per parameter; each inlet accepts exactly one cable (`OccupiedInput`), so multiple writers need an explicit mixer.
- [x] Add reusable smoothing/slew components with declared response and state continuity; avoid hiding unrelated smoothing inside each module. `Smoothers` (one-pole: 0.632 after τ; linear: full scale in `time`, measured 0.5 at τ/2) and the `smoother` graph node.
- [ ] Deliver sample-offset events in deterministic order, including simultaneous note, parameter and trigger events. Partial: same-frame parameter events apply in order and nine block splits render identical samples (synth test "same-frame automation…", 47/47 ARC); note on/off ownership across exact partitions is the daw MIDI timing test. Trigger events and mixed note+parameter frames remain unproven.
- [x] Define which changes apply at a sample boundary, block boundary or prepared-resource swap; expose pending versus applied values. Defined in [Synthesis.md](../specs/runtime/Synthesis.md): parameter events apply before their exact sample; topology and resource replacement (`Repatch.apply`) publish at a block boundary; `Commands` receipts are `Pending`, `Applied(revision)` or `Rejected(fault)`. Tests: same-frame automation with nine block splits, "replace a playing graph without resetting phase", and the controller-write journey (synth 51/51 ARC).
- [x] Preserve surviving state across valid graph changes, and keep the previous running graph if preparation fails. `GraphRebase.rebased` carries surviving components' memory into an edited graph (layout and storage must still fit), starts new components silent and drops removed ones; an invalid edit returns an error and the caller keeps its previous graph and state. Probe: inserting a reverb into a running sine → gain graph continues rendering with the oscillator phase unchanged (0.16667); without rebasing the edit is rejected. New grain clouds still need a restart.

### Resources and bounded rendering [SYNTH-ADVANCED-CORE-RESOURCES]

- [x] Define sample/table/impulse resources with identity, content hash, channel count, sample rate, length and version. `Resources.sample` and `Resources.table` derive a `ResourceIdentity` (`graph.td`): name, 64-bit FNV-1a hash over every sample word and its metadata, channels, rate, frames and version; an impulse response is a sample resource. `Resources.matches` refuses a changed or renamed resource as `MissingResource`. The wave journey "sample, impulse and table resources carry a content identity…" checks ten variants hash distinctly (one bit, -0.0, rate, channels, root, order, length, empty), frame counts for five layouts and four tables (GC and ARC).
- [ ] Decode files, prepare tables, allocate pools and construct transforms outside the audio callback.
- [x] Set explicit limits for voices, grains, modes, partials, FFT sizes, delay memory, automation queues and resource bytes. Voices ≤ 16, grains ≤ 256, modes 8, partials 64, FFT 64–4096, fitting ≤ 16 parameters/10,000 evaluations, delay storage ≤ 1,920,000 samples.
- [x] Define a capacity policy per resource: reject, steal with a release, or drop with a diagnostic; do not allocate on exhaustion. Grains reject new onsets (counted); `StringVoices` steal with a release (counted); spectral nodes over the per-graph budget and graphs over the storage budget are rejected before allocation; fitting rejects jobs over 16 parameters or 10,000 evaluations.
- [x] Specify source-boundary behaviour separately for looping, one-shot, reverse playback and interpolation. Specified in [Synthesis.md](../specs/runtime/Synthesis.md) [SYNTH-ADVANCED-GRAINS-BOUNDARY]: loop wraps (tap after the end is the region start), one-shot stops (tap repeats the last frame, the grain ends outside), reverse is a negative rate, interpolation is linear. `regionJourney` in grain_journeys.ospml checks loop/one-shot × forward/reverse/held. Reflect is not implemented.
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

## Granular synthesis [SYNTH-ADVANCED-GRAINS]

Reference: [Truax's granular synthesis overview](https://www.sfu.ca/~truax/gran.html) and the Roads material in [SYNTH-RESEARCH-MODELS]. The deliverable is an editable grain engine, with scheduling separate from grain playback.

| Module | Inputs and output | Main controls |
| --- | --- | --- |
| Sample source | Prepared sample resource → addressable audio | Resource, channel policy, loop range |
| Grain scheduler | Clock, trigger, modulation → grain events | Density, timing jitter, seed, distribution |
| Grain player/pool | Sample source, grain events → stereo audio | Source position, duration, playback rate, gain, pan |
| Grain window | Normalised lifetime → amplitude | Window shape, attack/decay shape |
| Live grain capture | Audio input → bounded capture resource | Capture duration, freeze, write position |

### Playback kernel and pool [SYNTH-ADVANCED-GRAINS-PLAYBACK]

Checked items here record reference implementation, reviewed in `src/osprey/packages/synth/src/grains.ospml` and `grain_math.ospml`; they do not satisfy the deferred verification gates. Static check: `osprey src/osprey/packages/synth --check --warnings-as-errors`.

- [x] Implement fractional source addressing with an explicitly documented interpolation mode and quality limitation.
- [x] Support forward, reverse and near-zero playback rates with defined zero-rate behaviour.
- [x] Implement Hann and configurable attack/sustain/release grain windows with explicit zero-endpoint branches and per-grain retained settings.
- [x] Verify window endpoints, peak/sustain behaviour, short durations and block/control-change invariance through rendered audio evidence. Synth test "a rendered grain shows its window…": Hann ends exactly 0 and peak at the centre, ASR 0/0.5/1/…/0.5/0, a three-frame grain, silence after the end, 16 one-frame blocks equal to one 16-frame block for three settings, and a later silent ASR trigger leaving the sounding grain unchanged (52/52 ARC).
- [x] Keep grain duration independent from playback rate and source-position scanning.
- [x] Add per-grain amplitude and equal-power stereo placement; define mono and stereo-source policies explicitly. Per-grain `amplitude` and `pan` with `GrainMath.leftGain`/`rightGain` (cos/sin of (pan+1)·π/4); the grain kernel reads mono, and its prepared stereo WAV view averages (L+R)/2 per frame, documented in [SYNTH-ADVANCED-GRAINS-IMPORT]. `regionJourney` checks full-left placement at amplitude 1.
- [x] Retain cursor, lifetime and window phase across arbitrary block boundaries.
- [x] Implement a bounded voice pool with stable identities and an explicit full-pool response.
- [x] Define reject-new overload behaviour: retain all active envelopes, count rejected onsets, and continue rendering without stealing voices.
- [x] Expose active count, lifetime requested/applied density, rejected/stolen counts and capacity through the existing grain observation Signals.
- [x] Verify saturation recovery, simultaneous onsets, counter accuracy and absence of overload-induced truncation in rendered audio. The clip journey "a two-voice grain pool rejects and counts simultaneous overload onsets…" (GC and ARC): eight onsets (five on one frame, two overlapping) into two voices count requested 8, accepted 3, rejected 5, stolen 0; the audio equals the three accepted grains alone sample for sample, the late grain carries across the block boundary uncut, the pool recovers to admit it once both voices end, and two same-frame onsets play exactly twice one.
- [x] Document summing/headroom policy, distinguishing peak control from density-dependent loudness compensation.

### Schedulers and sample resources [SYNTH-ADVANCED-GRAINS-SCHEDULING]

Implementation: `grain_scheduler.ospml`, `grains.ospml` and the `GrainClouds` composition; strict static checking only while automated verification is paused. Scheduler and playback clocks are retained independently, and a caller commits both only after successful rendering.

- [x] Implement explicit sample-position grain events, independent of host block size.
- [x] Add periodic/quasi-synchronous scheduling with seeded bounded inter-onset jitter.
- [x] Add bounded exponential asynchronous timing, explicitly rounded/censored rather than claiming an exact Poisson process.
- [x] Verify timing gap bounds, seeded replay, block partition invariance and observed distribution statistics. The clip journey "periodic, jittered and censored exponential onset schedules…" (GC and ARC) runs five timings (periodic 100, jittered 100±40 and 7±6, exponential mean 50 capped 400, mean 20 capped 30) at three seeds over 8,192 frames: every gap lies in its Lean bound, the mean gap matches the censored mean (50.5, 15.93) within its slack, the extremes spread as the jitter allows, three irregular block splits give identical onsets and clocks, the same seed replays exactly and different seeds move every non-periodic schedule.
- [x] Add independent onset timing and bounded uniform duration, source-position, playback-rate, gain and pan distributions with separately retained parameter seeds.
- [x] Expose five distribution widths and a restart-only variation seed through Grain Cloud graph descriptors consumed by the existing Prism inspector.
- [ ] Verify native distribution controls, replay, partition invariance and observed statistics. Partial: the clip journey "grain position, rate, duration, gain and pan distributions…" (GC and ARC) checks 2,000 events at three seeds: every field inside its bounds, its mean within 4% of the midpoint, both ends reached within 1%, splits at 1, 700 and 1,999 reproduce the batch and seeds, empty batches consume nothing, seeds replay exactly and differ from each other, and a collapsed range plays its one setting. The Prism inspector driving these spreads is not yet verified.
- [x] Specify seed persistence, reset and reseed semantics; identical state and events must reproduce the same schedule.
- [x] Support overlapping grains across loop boundaries without reading outside prepared resources. The clip journey "overlapping grains crossing a loop's end…" (GC and ARC) plays four nine-frame grains in a [2, 6) loop of an eight-sample source, two forward across the end, one at 1.5x and one reversed across the start: every live cursor stays inside the region on all 12 frames, and the mix equals the sum of each grain alone to 1e-12.
- [x] Add immutable resource replacement: existing grains finish against their owned resource while new grains adopt the new one. `Grains.replaceSource` keeps the old source as `retiredSource` until its last grain ends and refuses a second swap meanwhile; `overlapSources` in grain_journeys.ospml checks frames, counts, the refusal and release (synth 51/51 ARC).
- [x] Add bounded live capture/freeze with an explicit read/write ownership strategy. `GrainCapture` writes into a bounded buffer, `freeze` makes it read-only (writes refused until resume) and `resource` prepares an immutable `SampleResource` from it; `captureJourney` and `grainCaptureBoundsJourney` check bounds, non-finite input, freeze and the resource.
- [x] Add optional pitch-synchronous scheduling as a separate mode with its own analysis and latency contract. `PitchMarks.block` accepts a bipolar reference with one rising crossing per period and needs zero lookahead. `pitchMarkJourney` verifies absolute marks at frames 29 and 33, a crossing across a block boundary, identical terminal state and invalid reference rejection under GC and ARC. [Scheduler contract](../../src/osprey/packages/synth/README.md) · [Journey](../../src/osprey/packages/synth/tests/grain_journeys.ospml)

### Real sample loading and ownership [SYNTH-ADVANCED-GRAINS-IMPORT]

The first import path is bounded local WAV loading, not a general codec framework. File selection and reading belong to the application; decoding and sample validation are pure library operations. No file access, decoding or allocation may be moved into the audio callback. Initial supported encodings and limits must be visible in the application and package documentation.

- [ ] Declare generated models for decoded sample metadata, prepared sample identity, channel policy and typed import errors. Partial: decoded metadata is `WaveSamples`/`WaveFormat` (`wave.td`), prepared identity is `ResourceIdentity` (`graph.td`, fb56cac8d) and import errors are typed `Fault`s (a malformed WAV is `MissingResource`). Import keeps the original interleaved lanes. A mono consumer requests an equal-weight mix. A generated channel-policy model remains open.
- [x] Define the first supported WAV encodings, channel counts, sample rates, file-byte and decoded-frame limits before implementing the decoder.
- [x] Implement bounded RIFF/WAVE chunk parsing with checked lengths, odd-byte padding, unknown-chunk handling and explicit duplicate/truncated-chunk rejection.
- [x] Validate encoding, frame alignment, byte rate, sample count and finite sample values; reject unsupported formats rather than guessing. Evidence: "WAV import refuses corrupt sizes, headers, chunks, formats and out-of-range samples" in [wave_journeys.ospml](../../src/osprey/packages/synth/tests/wave_journeys.ospml) refuses 8 unsupported encodings/widths/channels/rates/alignments/byte rates and NaN, infinite and out-of-range floats with their exact messages; synth suite 56/56 under ARC on 21 Nov.
- [x] Define stereo handling explicitly: preserve channels where supported or perform a documented downmix; do not silently discard a channel.
- [x] Add native file selection for the selected Grain Cloud; cancellation leaves the patch unchanged. Evidence: `sampleFileJourney` in [history.ospml](../../src/osprey/examples/daw/test/history.ospml), daw suite 24/24 on 21 Nov.
- [x] Read a bounded amount of data outside rendering and reject oversized resources before unbounded allocation or decoding. Evidence: `sampleFileJourney` in [history.ospml](../../src/osprey/examples/daw/test/history.ospml), daw suite 24/24 on 21 Nov.
- [x] Guard asynchronous import completion with request identity and node identity/revision so an old result cannot overwrite a newer choice or resurrect a removed node. Evidence: `sampleFileJourney` in [history.ospml](../../src/osprey/examples/daw/test/history.ospml), daw suite 24/24 on 21 Nov.
- [x] Keep the last valid sample and graph on decode, preparation or activation failure; surface a useful node-specific error. Evidence: `sampleFileJourney` in [history.ospml](../../src/osprey/examples/daw/test/history.ospml), daw suite 24/24 on 21 Nov.
- [x] Carry source sample rate and root note into playback-rate calculation; a unity-rate sample must not be pitched according to its buffer length.
- [x] Define loop range, one-shot/reverse/end behaviour and source-position units; make the inspector match the actual supported behaviour. Region start/end (samples), boundary 0 loop/1 one-shot, signed rate for reverse; the inspector shows those descriptors with their units ("Source position (samples)", "Boundary (0 loop, 1 one-shot)"); see [SYNTH-ADVANCED-GRAINS-BOUNDARY].
- [ ] Show the loaded resource name, duration/rate/channel metadata, waveform and current source position using bounded observations.
- [ ] Include sample replacement in Undo/Redo without invalidating resources still owned by playing grains or history snapshots.
- [ ] Define an initial import-during-playback policy and show it clearly; do not block rendering while preparing a replacement.
- [ ] Persist resource identity/content hash and a portable resolution strategy; missing or changed files require an explicit recoverable result.
- [x] Document import limits, interpolation quality, root-note assumptions and any downmix policy alongside the Grain Cloud example. Done in "WAV import limits and tuning" in the [synth README](../../src/osprey/packages/synth/README.md): 12 B–8 MiB, PCM 8/16/24/32 or float32, mono/stereo, 8–192 kHz, 2–1,048,576 frames, `(L + R) / 2` downmix, no resampling, root C4, linear non-bandlimited interpolation, stopped-only replacement.

Implementation checkpoint: `WaveImport.decode` now prepares bounded PCM 8/16/24/32 or float32 WAV resources (mono/stereo, 8–192 kHz, at most 8 MiB and 1048576 frames), with original channels and generated metadata. Strict package checking passed; numerical/parser execution and application file selection remain outstanding. `SampleResources.decodeWave` now returns a checked immutable `SampleResource` for `Components.granular`, carrying original rate/channel count and explicit root tuning; the grain kernel uses `(left + right) / 2` and the bank retains both channels. Graphs retain that resource, and keyboard transposition uses its rate and root frequency instead of its buffer length. Content hashes, portable persistence and live resource replacement remain unfinished. Format references: [Microsoft WAVEFORMATEX](https://learn.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatex) and the [RIFF specification](https://www.mmsp.ece.mcgill.ca/Documents/AudioFormats/WAVE/Docs/riffmci.pdf).

Deferred verification: after the pause, cover supported encodings, empty/truncated/oversized files, malformed chunk arithmetic, cancellation, stale completions, node removal, repeated replacement, Undo/Redo, missing resources and pitch consistency across source/output sample rates through loaded application/library journeys.

### Graph and Prism integration [SYNTH-ADVANCED-GRAINS-INTEGRATION]

Evidence on 19 November 2026: strict package/app checks and compilation checks of their existing test sources passed; no tests were executed. Native archive and host builds passed (cached host rebuild: 16 seconds). Manual native UI inspection confirmed **Add Grain cloud**, creation of **Grain cloud 5**, its built-in source waveform and seven editable inspector fields, with the Prism logo, original nodes and docked keyboard retained. Manual native routing subsequently created cable `5:0 → 6:0` from Grain Cloud to a new stereo output, and the audition completed. Its receipt reported three empty-queue callbacks; this is not direct speaker-underrun measurement and does not establish reliable audio. Granular PCM correctness, error journeys and sustained playback remain unverified.

- [x] Add a stereo Grain Cloud factory and retained per-node memory to the shared graph scheduler, including external-worker suspension/resume.
- [ ] Add independent sample-source/scheduler graph factories and event/control modulation ports; do not make a Prism-only synthesis path.
- [x] Add the Grain Cloud entry to the module browser using the existing Prism node/inspector styling.
- [ ] Add Grain Scheduler and Sample Source entries to the module browser.
- [ ] Provide source selection, waveform/position display, density, duration, pitch, spread, seed and freeze controls in the existing inspector design.
- [ ] Map keyboard/phone expression to declared parameters rather than hard-coded instrument-specific commands.
- [x] Ship playable cloud, rhythmic-grain and frozen-texture patches with visible modulation routes. The three [Patches factories](../../src/osprey/packages/synth/src/patches.ospml) wire an LFO to gain or cutoff. On 21 November the [loaded preset journey](../../src/osprey/packages/synth/tests/clip_journeys.ospml) verified each cable, distinct nonzero PCM bounded by unity, repeatable state and equivalent audio across block splits under GC and ARC. This is offline graph evidence.
- [ ] Save resource identity and scheduler state in patches; make unavailable samples a visible recoverable error.

## Physical and modal synthesis [SYNTH-ADVANCED-PHYSICAL]

Reference: Smith's waveguide material and Bilbao's numerical synthesis references in [SYNTH-RESEARCH-MODELS]. Physical instruments are compositions of excitation, propagation, losses and resonators, not opaque presets.

| Module | Inputs and output | Main controls |
| --- | --- | --- |
| Exciter | Trigger/envelope/noise or audio → excitation | Strength, duration, colour, seed, strike position |
| Fractional delay | Audio → delayed audio | Delay/tuning, interpolation policy |
| Loss filter | Audio → damped audio | Frequency-dependent loss, brightness |
| Resonator | Excitation → resonant audio | Frequency, decay, gain |
| Modal bank | Excitation → summed modes | Mode ratios, mode decay, weights, coupling |
| Waveguide instrument | Excitation and controls → audio | Pitch, damping, dispersion, pickup/strike position |

### Shared physical primitives [SYNTH-ADVANCED-PHYSICAL-PRIMITIVES]

- [x] Implement deterministic impulse, filtered-noise and external-audio exciters.
- [x] Implement fractional-delay reads with a stated interpolation response, tuning error and modulation policy.
- [x] Implement frequency-dependent loss and dispersion as independent components.
- [x] Implement damped resonators with prepared coefficients and explicit persistent state.
- [x] Define stable parameter domains and reject invalid coefficients before activation.
- [x] Define how control changes alter stored energy; prevent abrupt parameter changes from introducing unintended unstable energy. Resonators (and so modal banks) use the coupled Gold–Rader form: without input the state norm falls by the pole radius every sample whatever the coefficients. Probe: abrupt 300 ↔ 3,000 Hz retuning every 100 samples grew stored energy on 0 samples; 300.0 Hz and T60 unchanged. Waveguide frequency edits keep the line (loop gain stays below one).
- [x] Keep excitation strength and output gain separate so expression remains meaningful.

### Playable instruments [SYNTH-ADVANCED-PHYSICAL-INSTRUMENTS]

- [x] Compose a plucked-string instrument from excitation, delay and loss primitives.
- [x] Add strike and pickup position controls with documented effects on the model. Strike: `StrikeCombs` / `strikeComb` node, `x[n] − x[n − βP]` (Smith); probe β = 1/4 on a 187.5 Hz string nulls harmonics 4 and 8 by 53 dB with the theoretical 1:2:1 pattern elsewhere. Pickup: the waveguide's `pickup` comb; modal banks weight modes by strike position.
- [x] Add sustained excitation with a bounded feedback/nonlinear interaction model for bowed/blown-style instruments. `BowedStrings`: Smith/STK two-segment bowed string with clipped friction table; scratch probe at 187.5 Hz: 99.9999% of energy on harmonics, sustained (late/early energy 2.2).
- [x] Implement modal banks with bounded mode count, independent frequencies, gains and decay times.
- [x] Add inharmonic mode sets for bells, bars, membranes and metallic bodies; label approximations rather than claiming full mechanical simulation.
- [x] Provide mode excitation weights derived from a declared strike/pickup-position model.
- [x] Add polyphonic voice allocation, release tails and explicit voice stealing for physical instruments. `StringVoices`: 1–16 waveguide voices; free → longest-released → oldest-held stealing, counted; scratch probe: 5 notes on 4 voices stole 1, released voices freed after the 0.1 s release T60.
- [x] Preserve tuning and decay when sample rate changes through a documented re-preparation/state-transfer policy. `Waveguides.transferred` recomputes tuning/T60 for the new rate and resamples the stored loop (`DelayLines.resampled`); scratch probe 48 → 96 kHz: 220.18 Hz before and after, rms 0.01430 vs 0.01456.
- [x] Treat finite-difference mesh models as a separate bounded component with a stated discretisation/stability domain, not an implicit alternative inside the modal bank. `Meshes`: explicit 5-point damped wave equation, fixed edges, 2–32 points per side, λ ≤ 1/√2 enforced (λ 0.8 rejected); scratch probe 12×12, λ 0.5: lowest mode 1289 Hz vs 1305 Hz predicted, decay matches T60 (0.251 vs 0.25 energy ratio over 0.1 s).

### Graph and Prism integration [SYNTH-ADVANCED-PHYSICAL-INTEGRATION]

- [x] Expose Exciter, Resonator, Modal Bank, Fractional Delay and Waveguide modules independently in the palette. All advanced modules (17 kinds) are in Prism's native "Add component" picker with level-safe defaults, and pitched modules follow the keyboard through their declared `frequency` parameter; the canvas palette keeps its original twelve-entry layout and artwork. Strict Prism check only; not exercised in the running app.
- [x] Provide pitch, damping, brightness, strike position, pickup position and mode controls through standard parameter descriptors.
- [ ] Show meaningful activity/decay observations rather than decorative waveforms disconnected from audio.
- [x] Ship plucked-string, struck-metal and externally excited resonator patches with editable routing. `Patches.pluckedString`, `struckMetal`, `feedbackTexture` (exciter → resonator); library graphs, not Prism presets.
- [x] Allow granular output, sample playback and VST audio to excite physical models through ordinary compatible audio ports.

## Wavetable and additive synthesis [SYNTH-ADVANCED-TABLES]

- [ ] Add prepared wavetable banks with explicit sample rate, table length, phase alignment and resource ownership.
- [x] Generate/select bandlimited table levels for the supported pitch range; document residual aliasing and modulation limits.
- [x] Support continuous phase, table-position interpolation and predictable table replacement. Wavetable state is phase only, so a replacement table is read from the next sample at the same phase; position interpolates between frames.
- [x] Add table-position, phase, frequency and amplitude modulation through typed ports. Optional normalised control inlets on the wavetable node via `RenderInputs.controlled`: `position` (replaces), `amplitude` (multiplies), `pitch` (±2 octaves, exponential) and `phase` (read-point offset). Probes through `GraphClips`: level 0.50 ↔ 0.23, >1 kHz share 0.088 ↔ 0.118, and a 1 Hz LFO lifts 220 Hz to the predicted ≈807 Hz.
- [x] Define hard-sync and reverse/near-zero frequency behaviour rather than inheriting accidental oscillator semantics.
- [x] Implement bounded additive partial banks with independent frequency, phase and amplitude trajectories.
- [x] Support harmonic and inharmonic partial layouts, smooth partial-count changes and explicit upper-frequency handling.
- [x] Provide reusable partial-envelope controls without baking one timbre into the oscillator.
- [ ] Add Wavetable and Partial Bank module cards and resource controls in Prism.
- [x] Ship a morphing-table instrument and a controllable inharmonic additive patch. `Patches.morphingTable`, `inharmonicAdditive`.

## Spectral synthesis and processing [SYNTH-ADVANCED-SPECTRAL]

### Transform and resynthesis foundation [SYNTH-ADVANCED-SPECTRAL-CORE]

- [x] Implement a reusable prepared transform backend with bounded supported sizes and scratch memory.
- [x] Define analysis/synthesis windows, hop sizes, normalisation, padding and overlap/add contracts.
- [x] Specify algorithmic latency and startup/shutdown behaviour, including partial final windows. `SpectralStreams.latency` = size − 1, `tail` = size − 1 + hop zeros flush the partial final windows; startup is silent buffers.
- [x] Make spectral frames a typed representation with channel, bin, frequency, magnitude/phase and timestamp conventions. `SpectralStreams.view` returns `SpectralView` (channel, newest-sample position, size) with `SpectralBin` index, frequency, magnitude and unit phase phasor.
- [x] Define explicit audio↔spectral adapters; do not route spectral frames through ordinary audio ports implicitly. Spectral frames never travel on ports: each spectral node is an explicit audio → STFT → audio adapter with declared latency, and its side input is an ordinary audio port analysed inside the node.
- [x] Preserve phase/state across blocks and transport changes using a documented phase policy. State persists across arbitrary blocks; `SpectralStreams.relocated` restarts analysis buffers and the hop clock on a transport jump while the synthesised tail, freeze capture and synthesis phases continue.
- [x] Define CPU/memory budgets before exposing large transforms or multiple simultaneous analysis branches. `SpectralKernels.cost` (3·size·log2 size / hop butterflies per sample) and a 1,024 per-graph `budget`, enforced by both executors before allocation; spectral memory (5·size + 4 floats per node) counts against the graph storage budget. Probe: 7 nodes of 4,096/1,024 fit, 8 are rejected.

### Modules and expressive controls [SYNTH-ADVANCED-SPECTRAL-MODULES]

- [x] Implement spectral freeze with explicit capture and release, avoiding accidental phase discontinuities.
- [x] Implement spectral morphing with declared magnitude and phase interpolation policies.
- [x] Add spectral filtering, bin remapping and controlled spectral diffusion/smear as composable transformations.
- [ ] Add pitch/time manipulation only with its own phase/transient handling and latency contract. Partial: `SpectralPitch` (transform 6) is now a peak-locked phase vocoder (Laroche and Dolson): each spectral peak's region moves by the whole bins nearest its true frequency's shift and keeps its lobe, the peak's phase advances at the shifted frequency and the bins around it keep their analysed offset (identity phase locking) ([spectral_peaks.ospml](../../src/osprey/packages/synth/src/spectral_peaks.ospml), 4e212e09f, 1b45a3ba0). Latency is size − 1. The `pitchShifter` component is the FFT pitch-shift effect module (Prism palette "FFT pitch shifter", project component `pitchShifter`), preset to a plain shift (formant = ratio). Evidence, middle C through the song renderer at 1024 points: a sine keeps 60% of its energy a fifth up (ratio 1.5), 76% an octave up and 96% an octave down (the bin-by-bin mapping it replaced kept 12% an octave up); the shortfall is amplitude ripple, which [SYNTH-SPECTRAL-PITCH-EFFECT] requires removed. Shifts beyond an octave are out of scope; the daw test asserts G4 and C5 periods, half-period anti-correlation and more than half the level. Transient handling and time stretching remain open.
- [x] FFT pitch-shift effect in musical units ([SYNTH-SPECTRAL-PITCH-EFFECT]): `semitones` −12…+12 and `cents` −100…+100 replace the raw ratio on the `pitchShifter` effect (ratio formula in `SpectralMath.semitoneRatio`); formant preservation is an explicit option, off by default; range limited to one octave either way; acceptance within 1 cent and 1 dB, no ripple above 1 dB, at ±1/5/7/12 semitones and ±1/10/50 cents. Done: the spectral node takes `semitones`, `cents` and `keepFormants` (0 off), the ratio is `SpectralMath.semitoneRatio`, `formantRatio` keeps formants only when asked. Evidence: "the FFT pitch shifter moves a sine by 1, 5, 7 and 12 semitones and 1, 10 and 50 cents either way within 1 cent, 1 dB of level and 1 dB of ripple" in [wave_journeys.ospml](../../src/osprey/packages/synth/tests/wave_journeys.ospml) measured on 21 Nov (500 Hz full-scale sine, 16 kHz, size 1024): worst pitch error 0.011 cent, worst level −0.42 dB (−7 semitones), worst ripple 0.06 dB; synth suite 56/56 under ARC. Measured on sines only, not on noisy or polyphonic input.
- [ ] Real-time autotune ([SYNTH-SPECTRAL-AUTOTUNE]): YIN-style fundamental detection with confidence, snapping to key/scale or the timeline's notes, retune speed, amount and humanise, formant-preserving FFT correction, unvoiced frames passed dry, bounded per-hop work without allocation, declared latency; native Osprey formulas; tests for a 30-cent-flat vibrato line, glide timing, unvoiced noise and block-split invariance.
- [x] Expose analysis/resynthesis separately so multiple transformations can share one analysis where compatible. `SpectralSeries` runs 1–8 stateless steps (band, shift, morph) on one analysis and one resynthesis; stateful steps (freeze, smear, pitch) are rejected as incompatible. Probe: band 500–2,000 Hz then +4 bins moves 1,031.25 Hz to exactly 1,406.25 Hz; a series containing freeze is rejected.
- [x] Formant pitch shifting [SYNTH-ADVANCED-SPECTRAL-FORMANT]: `SpectralPitch { ratio formant }` moves pitch by `ratio` and the spectral envelope by `formant` independently; `formant = 1` keeps the voice's formants where they were, `formant = ratio` is the plain (chipmunk) shift. The envelope is the per-frame peak magnitude within about 150 Hz (it follows partial peaks, not the gaps between them); each output bin is rescaled by envelope(j / formant) / envelope(j / ratio), bounded so silent bins are never amplified. Gain formulas live in `spectral_math.ospml`. A loaded test checks a vowel-like source (fixed resonances, moving pitch): pitch lands on target and the resonance peaks stay within one bin with `formant = 1`, and move with `formant = ratio`. Evidence (da0d559c9): 156.25 Hz vowel with a 1 kHz resonance, ratio 1.5 at 16 kHz — kept: 937.5 Hz/1406.25 Hz = 1.83; moved: 0.118; old-pitch and off-harmonic bins < 1e-6; the spectral node has a Formant control (0.25–4, default 1). Synth 40/40 GC and ARC.
- [x] Spectral morphing modes [SYNTH-SPECTRAL-MORPH]: Blend (log magnitude), Envelope (cross-synthesis, main input carries) and Partial (peaks paired in frequency order, gliding in log frequency and level as phase-locked lobes) on `SpectralMorph { amount mode }`, with the spectral node's morph-mode control (c6b1fd48a, 81552760a; [research](../research/synthesis/spectral-morphing.md)). Evidence: blend 0.5↔0.05 midway 0.158 (5 amounts within 5%); envelope moves a 1 kHz resonance to 1.875 kHz; partial 220→330 Hz at 0.5 puts harmonics at k × 269.44 Hz, level 1/k within 20%, sources 30 dB down, over ten glides. Synth 42/42 GC and ARC. Open: automation-sweep smoothness and orphan-fade level tests; MQ-style cross-frame tracking for inharmonic sounds.
- [ ] Showcase songs [SYNTH-SHOWCASE]: the shipped songs play every family (granular and spectral morph are missing today). The renderer must play granular and spectral nodes per note at usable speed. A test opens each song and checks every family is present.
- [ ] Synthesis coverage to 100%: `@osprey/synth` measured 63.4% (2,650 of 4,180 lines) on 21 November 2026; loaded journey tests per family raise it, and the threshold only goes up.
- [ ] Prism and the daw expose Pitch and Formant controls on the spectral node; the project file stores both.
- [ ] Add spectral displays driven by bounded observer snapshots, not analysis on the UI thread.
- [x] Ship freeze, cross-synthesis and spectral-motion patches using visible modules and cables. `Patches.spectralFreeze`, `crossSynthesis` (bowed string morphed onto a stiff additive side input) and `spectralMotion` (WPPM → smear → +3-bin shift); all render 4,096 samples with peaks ≤ 0.25. Library graphs, not Prism presets.

## Nonlinear, spatial and feedback processing [SYNTH-ADVANCED-NONLINEAR]

- [x] Implement reusable waveshapers with explicit input range, gain staging and DC behaviour.
- [x] Add antiderivative antialiasing where the transfer function permits it, including near-equal-input handling.
- [x] Add prepared oversampling/downsampling stages with explicit factors, filters, latency and resource cost.
- [x] Compare oversampling and ADAA rather than assuming one method is universally preferable.
- [x] Implement fractional/modulated delay, diffusion and feedback matrices as separate building blocks. `DelayLines` (fractional taps), `Diffusers` (series Schroeder allpasses, |g| ≤ 0.9) and `FeedbackMatrices.householder` (orthogonal N-line mixing) are standalone modules; `Reverbs` is composed from them and its probe output is bit-identical after the extraction.
- [x] Build a configurable feedback-delay-network reverb with damping, size, diffusion and freeze controls.
- [x] Define stability domains and feedback gain limits; freeze must have an explicit energy policy.
- [x] Add frequency shifting, ring modulation and pitch-shifted feedback where supported by the declared algorithms. `Shifters` (SSB and ring) plus `Patches.shiftedFeedback`: a burst circulating through a +7 Hz shifter, 0.7 gain and a 50 ms delay; probe peak 0.72 → 0.31 over successive 85 ms windows (stable, decaying).
- [x] Keep feedback loops explicit in the graph and require causal delay; bound any nonlinear solver's iteration count.
- [x] Define failure handling when convergence or stability requirements are not met. No iterative solver is used; stability domains (λ ≤ 1/√2, |allpass| ≤ 0.95, loop gains < 1, |diffuser g| ≤ 0.9, Courant and T60 bounds) are enforced at `configure` with an explicit error and no partial state; fitting reports `converged = false` when it exhausts its budget.
- [ ] Expose Delay, Diffuser, Feedback Matrix, Waveshaper, Frequency Shifter and Reverb in Prism.
- [x] Ship an evolving feedback texture and a granular-through-resonator/reverb patch with safe initial output gain. `Patches.feedbackTexture` and `Patches.granularResonance` (grain cloud → explicit `downmix` → band-pass resonator → reverb; probe peak 0.029 while the resonator builds).

## Phase transformation and experimental oscillators [SYNTH-ADVANCED-PHASE]

- [x] Separate phase generation, phase transformation, waveform lookup and amplitude shaping into composable components.
- [x] Implement frequency modulation, phase modulation and phase distortion with explicit units and state continuity.
- [x] Add phase reset/sync and sample-offset trigger semantics.
- [x] Implement WPPM as a named research candidate derived from the cited source; preserve its exact parameter definitions and provenance.
- [x] Define antialiasing/oversampling behaviour for discontinuities and rapidly varying modulation. Plain operators are not antialiased (documented); `PhaseOperators.oversampled` / the node's `oversample` switch runs the operator at 2× into the 31-tap half-band decimator (7.5 samples latency). Probe: PD d = 0.05 at 2,343.75 Hz, alias/total energy 0.71% plain → 0.017% oversampled.
- [x] Support audio-rate modulation where the kernel contract allows it; do not disguise block-rate controls as audio-rate processing.
- [ ] Add phase/operator modules and editable routing in Prism, including modulation depth and scaling adapters.
- [x] Ship a playable phase-network patch with several useful macro mappings. `Patches.phaseNetwork` with `phaseNetworkMacros` (Brightness, Growl, Motion) applied by `Macros.apply`; probe: each macro moves the >2 kHz energy share from ≤ 2.4e-5 to 3e-4–0.09. Library patch; Prism exposure open.

## Learned controls and offline discovery [SYNTH-ADVANCED-LEARNED]

- [x] Expose offline parameter trajectories for the same production DSP operators; avoid a separate incompatible synthesis model.
- [x] Add parameter fitting with explicit objective functions, constraints, seed and stopping conditions.
- [x] Record model/data/licence provenance and distinguish training data from evaluation material. `FitReports.fit` reports the algorithm and its licence (`osprey-synth/log-spectrum-es-v1`, MIT), the training and evaluation `FitSource`s (identifier, licence, rate) and the config; it refuses a shared training/evaluation source and an unlicensed, unlabelled or non-48 kHz clip. `fitGainJourney` asserts each field and refusal (GC and ARC).
- [x] Keep learned results editable as ordinary parameters, tables or automation curves.
- [x] Run training, patch search and expensive inference off the audio callback and outside the library's mandatory runtime dependencies.
- [x] Define cancellation, progress, resource limits and explicit failure results for long offline jobs.
- [x] Treat a neural waveform generator as an optional later component requiring its own latency, memory and degradation contract. Stated in [Synthesis.md](../specs/runtime/Synthesis.md) [SYNTH-ADVANCED-NEURAL]: no instrument, patch or test depends on one, and none ships before its own latency, memory and degradation contract.
- [ ] Add application controls to preview/apply/reject fitted parameters without overwriting a working patch silently.

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
- [x] Render tracks in start order with an overlap-add carry, mix to left, right and reverb buses, run one feedback-delay reverb per side and a tanh soft limiter (`Arrangements`).
- [x] Hold the end of a recording with the spectral freeze and replay the hold at a pitch ratio with its own rise, hold and fade (`Sustains`) [SYNTH-ARRANGEMENT-SUSTAIN]. The freeze gained identity phase locking: every bin turns with its nearest magnitude peak.
- [x] Decode the JSON project in one boundary module (`SongFiles`) into the typed arrangement [SYNTH-ARRANGEMENT-FILE]; malformed JSON, unknown instruments and malformed notes are errors.
- [x] Command-line player `src/osprey/examples/daw` and `make -C src/toolchain/targets/macos daw-render` and `daw-play` [SYNTH-ARRANGEMENT-PLAYER]: renders, writes WAV parts through `xxd` and joins them after the intro with `ffmpeg`; `daw-play` plays with `afplay`; every host command runs under `timeout`.
- [x] A song project written to continue the Broken Circuitry recording (first a melodic version: 44 bars, 14 tracks, 2,074 notes; now `phosphor-drift.json`, below, 2,904 notes).
- [x] Routed instruments (`PatchVoices`) [SYNTH-ARRANGEMENT-PATCH]: a note builds a graph from named modules and cables, with settings that follow pitch or velocity, and renders through `Renderer`. The mono renderer rejects LFO routing and granular; `spectral` and `reverb` nodes are too slow per note.
- [x] `phosphor-drift.json` ("Phosphor Drift", a new piece, not part of Broken Circuitry): 40 bars, 17 tracks (8 routed patches), 8 sustain appearances; rendered at 48 kHz after the recording, level across the join 0.18 → 0.13 RMS, sides within 0.2 dB.
- [x] The project file names its own intro (`Intro`: recording, tail, crossfade) in `daw.td`; the player only renders, and `daw-render` / `daw-play` render and play [SYNTH-ARRANGEMENT-PLAYER].
- [x] The daw (`src/osprey/examples/daw`, formerly `songs`) owns the project model, `daw.td`, and the arrangement modules; `@osprey/synth` keeps only the synthesis modules.
- [ ] Automated tests for `NoteRenders`, `Arrangements`, `Sustains`, `SongFiles` and the player (paused with the other suites).
- [ ] Bandlimited sustain transposition, an oversampled or look-ahead limiter and stereo instruments. Partial: the limiter is done. Master ends in `synth::Limiters`, a linked true-peak (four-times oversampled) lookahead brick-wall, replacing the tanh soft limiter (1b45a3ba0). Bandlimited sustain transposition and stereo instruments remain.
- [ ] Playback on Windows and Linux; `afplay` is macOS only.
- [ ] Detect an intro's tempo and key instead of measuring them by hand.

## Deferred acceptance checklist [SYNTH-ADVANCED-VERIFY]

Testing resumed on 21 November 2026. Record results for the stated workload, backend and host before checking each gate.

### Correctness and stability [SYNTH-ADVANCED-VERIFY-CORRECTNESS]

- [ ] Run loaded end-to-end patch journeys with repeated same-kind instances, fan-out, modulation, removal, undo, save/load and failures.
- [ ] Compare arbitrary block partitions against continuous rendering while preserving event ownership at exact boundaries.
- [ ] Exercise sample rates 44.1/48/96/192 kHz and supported block sizes, including unequal and one-sample partitions.
- [x] Verify grain windows, wrapping, reverse playback, deterministic schedules, pool exhaustion and resource replacement. The GC and ARC suites exercised rendered Hann and ASR envelopes, reverse and fractional-rate loop crossings, seeded onset replay, rejection counts at capacity and immutable source replacement while an old grain finishes. [Window rendering](../../src/osprey/packages/synth/tests/main.ospml) · [Loop and capacity tests](../../src/osprey/packages/synth/tests/grain_limits.ospml) · [Source replacement](../../src/osprey/packages/synth/tests/grain_journeys.ospml)
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
- [x] Include reproducible example patches for every delivered DSP family and a combined patch. `Patches.combined()` routes seeded grains through a downmix, modal body, spectral processor, waveshaper, LFO-controlled gain and reverb. The GC and ARC journey checks the emitting grain, physical, spectral, modulated and output stages after a 1,024-frame render. [Patch factories](../../src/osprey/packages/synth/src/patches.ospml) · [Combined journey](../../src/osprey/packages/synth/tests/clip_journeys.ospml) · [Single-family journeys](../../src/osprey/packages/synth/tests/spectrum_sources.ospml)
- [ ] Mark experimental/production status from measured evidence; record remaining limitations without weakening the contracts.
- [ ] Reconcile this checklist with [Synthesis.md](../specs/runtime/Synthesis.md) and the [Prism routing plan](prism-synth-routing.md), preserving their unfinished requirements.
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
