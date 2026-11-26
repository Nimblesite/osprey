# Advanced synthesis implementation plan [SYNTH-ADVANCED]

Build the advanced synthesis ecosystem on `@osprey/synth`: independent, composable DSP modules that can be routed, modulated, automated and combined in Prism, Light Orchestra and future hosts. Prism is the first complete integration, not the definition of the library. Every module must produce real audio; a name in the palette or an attractive waveform is not implementation.

The authoritative contracts are [Synthesis.md](../../specs/synth/Synth.md), especially [SYNTH-GRAPH], [SYNTH-SIGNALS], [SYNTH-RENDER] and [SYNTH-DSP]. The [research catalogue](../../specs/synth/Synth.md#sources-and-experiments-synth-research) supplies sources and comparison experiments. The existing [Prism routing plan](prism-routing.md) remains responsible for host lifecycle, live replacement, VST integration and release readiness; this plan adds the advanced DSP families and their complete user journeys.

**Current instruction (21 November 2026):** testing has resumed. Every change comes with loaded journey tests that run under GC and ARC; mark an item done only with recorded evidence, and never mark an unverified feature production-ready.

## Current state and delivery order [SYNTH-ADVANCED-ORDER]

Existing code provides oscillator/envelope/filter primitives, typed graphs, parameter descriptors, Signals, reference block processing, VST transport and Prism's modular editor. Advanced synthesis is not already implemented by those foundations. The granular reference kernel now has generated models, native Osprey formulas, checked pool state, stereo rendering and Signal observation. It has passed strict static checking only; sample-timed events and a seeded periodic/jittered scheduler are implemented. A stereo Grain Cloud graph factory, retained memory and Prism palette/parameter/keyboard integration are implemented. Independent uniform parameter distributions now compose with the onset scheduler in the library; native validation of distribution controls, application sample-file loading, dedicated source/scheduler graph nodes, modulation adapters and audio/UI verification remain outstanding.

Deliver in this order, finishing a playable vertical slice before beginning the next large family:

- [ ] Establish shared resource, state, modulation and scheduling contracts under [SYNTH-ADVANCED-CORE]. Done 21 Nov: typed `Fault`s, the `Lifecycles` transitions and `ResourceIdentity` hashes. Open: removing the fixed-demo `Voice` (its last caller is the Prism key preview), unlimited channels, polarity and rate adapter nodes, mixed note, parameter and trigger events in one frame, and the realtime render-path rules (preparation outside the callback, no allocation or locks).
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
- [ ] Make the selected Grain Cloud inspector expose meaningful units and bounded controls; retain Prism's original artwork and layout.
- [ ] Complete the manual journey: add Grain Cloud, load a sample, add/connect stereo output, play several notes, edit controls, stop and remove the cable/node.
- [ ] Trace the observed audio-queue starvation through producer, transport and device receipts; record workload and timings before selecting a fix.
- [ ] Remove the measured producer bottleneck without weakening validation or masking sustained throughput failure with a larger queue.
- [ ] Demonstrate sustained playback and stop/restart manually; keep automated correctness/performance acceptance unchecked during the pause.
- [ ] Add scheduler/source nodes and typed modulation adapters, then finish granular presets before beginning physical models.

## Shared component foundation [SYNTH-ADVANCED-CORE]

### Canonical models and module boundaries [SYNTH-ADVANCED-CORE-MODELS]

- [ ] Remove superseded fixed-demo DSP after callers migrate; do not retain parallel implementations that drift.

### Ports, parameters and automation [SYNTH-ADVANCED-CORE-PORTS]

- [ ] Unlimited channels [SYNTH-GRAPH-CHANNELS]: declare `ChannelLayout` (ordered named channels, stereo default, 5.1/7.1/7.1.4/any N) in `graph.td`; every port carries one.
- [ ] **[DAW-MODULAR-LAYOUT]: every track carries two channels.** Done: a patch's stereo output keeps its left and right to Master, a mono output and every built-in voice sit in the centre at equal power, and track pan is a balance with unity at the centre. Open: layouts other than stereo, `Grains` stereo summing and the `GraphClips` mono-to-stereo component.
- [ ] Track and Master output layouts in the project file (`daw.td`); sample import keeps every source channel; the macOS device queue plays any layout CoreAudio offers.
- [ ] Define explicit adapters for hertz/normalised values, bipolar/unipolar signals and control/audio rates. Partial: hertz/normalised is `Descriptors.fromControl`/`toControl` (logarithmic for positive frequencies and times), tested on every parameter of 24 components: ends, nine-point round trip, range and monotonicity (synth 49/49 ARC). Bipolar/unipolar exists only inside the LFO (`Waves.unipolar`); there are no adapter graph nodes for signal polarity or control/audio rate.
- [ ] Deliver sample-offset events in deterministic order, including simultaneous note, parameter and trigger events. Partial: same-frame parameter events apply in order and nine block splits render identical samples (synth test "same-frame automation…", 47/47 ARC); note on/off ownership across exact partitions is the daw MIDI timing test. Trigger events and mixed note+parameter frames remain unproven.

### Resources and bounded rendering [SYNTH-ADVANCED-CORE-RESOURCES]

- [ ] Decode files, prepare tables, allocate pools and construct transforms outside the audio callback.
- [ ] Align parallel routes automatically in preparation from the latency each component declares in `Timings.latency`.
- [ ] A grain cloud added to a running graph starts without a restart; `GraphRebase.rebased` already carries every other component across an edit.
- [ ] Avoid allocation, blocking locks, filesystem/network access, logging formatting and observer callbacks in production render paths.

### Signals and observers [SYNTH-ADVANCED-CORE-SIGNALS]

Complete. The modules and journeys that cite this ID hold the evidence.

## Prism and external-controller experience [SYNTH-ADVANCED-PRISM]

- [ ] Preserve the existing Prism logo, original node styling, visible cables, inspector and bottom-docked closable keyboard.
- [ ] Group module browsing by Sources, Grains, Physical, Spectral, Modulators, Effects, Utilities and Output; support search and drag/click insertion. Partial: the native picker lists all 29 kinds grouped in that category order with "Category · Title" labels; search and grouped canvas-palette browsing remain open.
- [ ] Give every inserted node real ports, useful defaults and an inspector generated from its descriptors.
- [ ] Make cable creation/removal, node movement/removal and Undo/Redo work for every advanced family.
- [ ] Show resource-loading and failure states on the affected node; avoid treating unavailable samples/models/plugins as a silent working instrument.
- [ ] Bind controls to applied parameter Signals and show pending/rejected changes distinctly.
- [ ] Provide macro controls that map to ordinary typed parameters with declared ranges and smoothing. Library part done: `Macros` validates and writes normalised amounts onto declared parameters through per-target ranges and curves; hosts smooth amounts with `Smoothers`. Prism controls remain open.
- [ ] Expose the same control commands to Light Orchestra/phone adapters without adding networking or session state to the synth library.
- [ ] Save complete patches, resources, seeds, module versions, automation and controller mappings; load incompatible/missing resources explicitly.

## Arrangements and the daw [SYNTH-ARRANGEMENT]

Added on request after the families: a piano-roll project file rendered offline by the advanced modules, and a command-line player that continues an existing recording. Nothing here is covered by an automated test yet; the evidence below is from rendering the included song.

- [ ] Automated tests for `NoteRenders`, `Arrangements`, `Sustains`, `SongFiles` and the player (paused with the other suites).
- [ ] Bandlimited sustain transposition, an oversampled or look-ahead limiter and stereo instruments. Partial: `synth::Limiters`, a linked true-peak (four-times oversampled) lookahead brick-wall, is done as a module. It belongs only where the user places it. The engine must never put it on Master by itself ([DAW-TRANSPARENCY]). Bandlimited sustain transposition and stereo instruments remain.
- [ ] Playback on Windows and Linux; `afplay` is macOS only.
- [ ] Detect an intro's tempo and key instead of measuring them by hand.

### Every processing point is a pluggable effect [SYNTH-ARRANGEMENT-EFFECTS]

Deferred until #427 is merged and the user approves; the repository is in stabilisation mode. Every place in the synth and the daw that processes a signal becomes an operation of an Osprey effect, so a handler supplies the processing. Examples are summing onto a bus, gain, pan, envelopes, filters, waveshaping, delays and the master stage. The default handler calls today's kernels (`ArrangeMath.bused`, `panLeft`, `panRight`, `enveloped`, `synth::Limiters` and others), so rendered output stays identical and the production proofs in [SYNTH-DSP-PROOFS](../../specs/synth/Synth.md) still describe the default.

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
- [ ] Verify physical-model tuning, decay/energy bounds, rate changes and extreme stable parameter values.
- [ ] Verify wavetable/additive spectral behaviour, phase continuity, modulation and partial/table replacement.
- [ ] Verify spectral reconstruction, overlap/add normalisation, latency, freeze/release and transport discontinuities.
- [ ] Compare nonlinear/phase outputs with independent higher-quality references under level/latency matching.
- [ ] Inject nonfinite data, malformed resources, invalid units, stale revisions, overflow and unavailable resources; require explicit failure and preserved prior state.

### Realtime and observer behaviour [SYNTH-ADVANCED-VERIFY-REALTIME]

- [ ] Instrument allocation, locking and unbounded operations in the compiled native callback path.
- [ ] Measure worst observed callback time, queue starvation, CPU, memory and latency on named hardware with declared workloads.
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
- [ ] Link code, specifications, patches and eventual test evidence through the section IDs in this plan.
- [ ] Mark experimental/production status from measured evidence; record remaining limitations without weakening the contracts.
- [ ] Reconcile this checklist with [Synthesis.md](../../specs/synth/Synth.md) and the [Prism routing plan](prism-routing.md), preserving their unfinished requirements.
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

Complete. The modules and journeys that cite this ID hold the evidence. The deferred verification gates under [SYNTH-ADVANCED-VERIFY] still apply.

### Schedulers and sample resources [SYNTH-ADVANCED-GRAINS-SCHEDULING]

Implementation: `grain_scheduler.ospml`, `grains.ospml` and the `GrainClouds` composition; strict static checking only while automated verification is paused. Scheduler and playback clocks are retained independently, and a caller commits both only after successful rendering.

- [ ] Verify native distribution controls, replay, partition invariance and observed statistics. Partial: the clip journey "grain position, rate, duration, gain and pan distributions…" (GC and ARC) checks 2,000 events at three seeds: every field inside its bounds, its mean within 4% of the midpoint, both ends reached within 1%, splits at 1, 700 and 1,999 reproduce the batch and seeds, empty batches consume nothing, seeds replay exactly and differ from each other, and a collapsed range plays its one setting. The Prism inspector driving these spreads is not yet verified.

### Real sample loading and ownership [SYNTH-ADVANCED-GRAINS-IMPORT]

The first import path is bounded local WAV loading, not a general codec framework. File selection and reading belong to the application; decoding and sample validation are pure library operations. No file access, decoding or allocation may be moved into the audio callback. Initial supported encodings and limits must be visible in the application and package documentation.

- [ ] Declare generated models for decoded sample metadata, prepared sample identity, channel policy and typed import errors. Partial: decoded metadata is `WaveSamples`/`WaveFormat` (`wave.td`), prepared identity is `ResourceIdentity` (`graph.td`, fb56cac8d) and import errors are typed `Fault`s (a malformed WAV is `MissingResource`). Import keeps the original interleaved lanes. A mono consumer requests an equal-weight mix. A generated channel-policy model remains open.
- [ ] Show the loaded resource name, duration/rate/channel metadata, waveform and current source position using bounded observations.
- [ ] Define an initial import-during-playback policy and show it clearly; do not block rendering while preparing a replacement.
- [ ] Persist resource identity/content hash and a portable resolution strategy; missing or changed files require an explicit recoverable result.

Implementation checkpoint: `WaveImport.decode` now prepares bounded PCM 8/16/24/32 or float32 WAV resources (mono/stereo, 8–192 kHz, at most 8 MiB and 1048576 frames), with original channels and generated metadata. Strict package checking passed; numerical/parser execution and application file selection remain outstanding. `SampleResources.decodeWave` now returns a checked immutable `SampleResource` for `Components.granular`, carrying original rate/channel count and explicit root tuning; the grain kernel uses `(left + right) / 2` and the bank retains both channels. Graphs retain that resource, and keyboard transposition uses its rate and root frequency instead of its buffer length. Content hashes, portable persistence and live resource replacement remain unfinished. Format references: [Microsoft WAVEFORMATEX](https://learn.microsoft.com/en-us/windows/win32/api/mmreg/ns-mmreg-waveformatex) and the [RIFF specification](https://www.mmsp.ece.mcgill.ca/Documents/AudioFormats/WAVE/Docs/riffmci.pdf).

Deferred verification: after the pause, cover supported encodings, empty/truncated/oversized files, malformed chunk arithmetic, cancellation, stale completions, node removal, repeated replacement, Undo/Redo, missing resources and pitch consistency across source/output sample rates through loaded application/library journeys.

### Graph and Prism integration [SYNTH-ADVANCED-GRAINS-INTEGRATION]

Evidence on 19 November 2026: strict package/app checks and compilation checks of their existing test sources passed; no tests were executed. Native archive and host builds passed (cached host rebuild: 16 seconds). Manual native UI inspection confirmed **Add Grain cloud**, creation of **Grain cloud 5**, its built-in source waveform and seven editable inspector fields, with the Prism logo, original nodes and docked keyboard retained. Manual native routing subsequently created cable `5:0 → 6:0` from Grain Cloud to a new stereo output, and the audition completed. Its receipt reported three empty-queue callbacks; this is not direct speaker-underrun measurement and does not establish reliable audio. Granular PCM correctness, error journeys and sustained playback remain unverified.

- [ ] Add independent sample-source/scheduler graph factories and event/control modulation ports; do not make a Prism-only synthesis path.
- [ ] Add Grain Scheduler and Sample Source entries to the module browser.
- [ ] Provide source selection, waveform/position display, density, duration, pitch, spread, seed and freeze controls in the existing inspector design.
- [ ] Map keyboard/phone expression to declared parameters rather than hard-coded instrument-specific commands.
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

Complete. The modules and journeys that cite this ID hold the evidence.

### Playable instruments [SYNTH-ADVANCED-PHYSICAL-INSTRUMENTS]

Complete. The modules and journeys that cite this ID hold the evidence.

### Graph and Prism integration [SYNTH-ADVANCED-PHYSICAL-INTEGRATION]

- [ ] Show meaningful activity/decay observations rather than decorative waveforms disconnected from audio.

## Wavetable and additive synthesis [SYNTH-ADVANCED-TABLES]

- [ ] Add prepared wavetable banks with explicit sample rate, table length, phase alignment and resource ownership.
- [ ] Add Wavetable and Partial Bank module cards and resource controls in Prism.

## Spectral synthesis and processing [SYNTH-ADVANCED-SPECTRAL]

### Transform and resynthesis foundation [SYNTH-ADVANCED-SPECTRAL-CORE]

Complete. The modules and journeys that cite this ID hold the evidence.

### Modules and expressive controls [SYNTH-ADVANCED-SPECTRAL-MODULES]

- [ ] Add pitch/time manipulation only with its own phase/transient handling and latency contract. Partial: `SpectralPitch` (transform 6) is now a peak-locked phase vocoder (Laroche and Dolson): each spectral peak's region moves by the whole bins nearest its true frequency's shift and keeps its lobe, the peak's phase advances at the shifted frequency and the bins around it keep their analysed offset (identity phase locking) ([spectral_peaks.ospml](../../../src/osprey/packages/synth/src/spectral_peaks.ospml), 4e212e09f, 1b45a3ba0). Latency is size − 1. The `pitchShifter` component is the FFT pitch-shift effect module (Prism palette "FFT pitch shifter", project component `pitchShifter`), preset to a plain shift (formant = ratio). Evidence, middle C through the song renderer at 1024 points: a sine keeps 60% of its energy a fifth up (ratio 1.5), 76% an octave up and 96% an octave down (the bin-by-bin mapping it replaced kept 12% an octave up); the shortfall is amplitude ripple, which [SYNTH-SPECTRAL-PITCH-EFFECT] requires removed. Shifts beyond an octave are out of scope; the daw test asserts G4 and C5 periods, half-period anti-correlation and more than half the level. Transient handling and time stretching remain open.
- [ ] Spectral morphing ([SYNTH-SPECTRAL-MORPH]), tests still owed at 48 kHz, size 4,096, hop 1,024: unmatched Partial peaks fade to 0.75 and 0.25 of their level within 1 dB at amount 0.25 (220 Hz against 277 Hz); a held amount of 0.5 keeps per-hop rms within 0.5 dB; an amount ramp from 0 to 1 over 0.5 s has no frame-to-frame jump over 2 dB beyond the trend. Then cross-frame peak tracking for inharmonic sounds, and a crossfade of phase donors near 0.5 in Blend ([failure modes](../../specs/synth/Synth.md#failure-modes)).
- [ ] Real-time autotune ([SYNTH-SPECTRAL-AUTOTUNE]): YIN-style fundamental detection with confidence, snapping to key/scale or the timeline's notes, retune speed, amount and humanise, formant-preserving FFT correction, unvoiced frames passed dry, bounded per-hop work without allocation, declared latency; native Osprey formulas; tests for a 30-cent-flat vibrato line, glide timing, unvoiced noise and block-split invariance.
- [ ] Showcase songs [SYNTH-SHOWCASE]: the shipped songs play every family (granular and spectral morph are missing today). The renderer must play granular and spectral nodes per note at usable speed. A test opens each song and checks every family is present.
- [ ] Synthesis coverage to 100%: `@osprey/synth` measured 63.4% (2,650 of 4,180 lines) on 21 November 2026; loaded journey tests per family raise it, and the threshold only goes up.
- [ ] Add spectral displays driven by bounded observer snapshots, not analysis on the UI thread.

## Nonlinear, spatial and feedback processing [SYNTH-ADVANCED-NONLINEAR]

- [ ] Expose Delay, Diffuser, Feedback Matrix, Waveshaper, Frequency Shifter and Reverb in Prism.

## Phase transformation and experimental oscillators [SYNTH-ADVANCED-PHASE]

- [ ] Add phase/operator modules and editable routing in Prism, including modulation depth and scaling adapters.

## Learned controls and offline discovery [SYNTH-ADVANCED-LEARNED]

- [ ] Add application controls to preview/apply/reject fitted parameters without overwriting a working patch silently.

## Advanced synthesis delivery plan [PRISM-ADVANCED-PLAN]

The 2026-11-17 handoff supplies the proposed delivery sequence below. The [active implementation checklist](advanced-synthesis.md) owns processor milestones and their status. The [package and UI specification](../../specs/synth/Synth.md#advanced-synthesis-proposal-synth-advanced) defines acceptance. The [research guide](../../specs/synth/Synth.md#sources-neural-granular-and-physical-synthesis-synth-research-focused) compares the papers and instruments behind these choices.

### Repository baseline and dependencies [PRISM-ADVANCED-BASELINE]

The proposal started from the package's `graph.td`, `render.td`, `src/clip_kernels.ospml`, README and Prism's `editor.td`. Reuse graph identities, typed cables, parameter revisions and retained block processing. The 17 November baseline used basic/external processor variants, oscillator/filter/delay kernel state and a fixed 48000 Hz clip path. The [family checkpoint](advanced-synthesis.md#advanced-family-checkpoint-synth-advanced-families-checkpoint) records subsequent changes. Keep the 16 November sustained-device failure open until fresh device evidence meets the contract.

| Layer | Integration and evidence |
| --- | --- |
| Schemas | Extend typeDiagram sources and regenerate. Round-trip old graphs and exercise missing and unknown fields. |
| DSP mathematics | Implement native Osprey formulas and prove properties of their parsed bodies for windows, interpolation, modal transitions, waveguide tuning, energy accounting and controller mappings. Check reference parity. |
| Package processing | Carry grain scheduling, body state and neural request descriptors through public preparation/block interfaces, validation and observations. |
| Host adapters | Bound asset decoding, model execution and generated-asset jobs. Reuse worker ownership/cancellation with a dedicated typed model protocol. |
| Prism | Extend existing editor state, palette, cards, inspector and keyboard gestures within the single accessible routing workspace. |
| Evidence | Extend loaded package/app journeys and platform screenshots. Keep artifacts below `target/`. |

### Shared foundation [PRISM-ADVANCED-FOUNDATION]

Follow the [shared component checklist](advanced-synthesis.md#shared-component-foundation-synth-advanced-core). Exercise invalid state, rate changes, asset generations, irregular partitions, unavailable models, cancellation and migration before extending behavior. Validate dimensions, products, limits, finite data and model operators before preparation. Thread format, processor-specific state and asset lifetimes through scalar and graph execution. Measure producer queues, fault publication, latency, cancellation ownership and device starvation. Preserve seeds, applied parameter semantics and hashes across save/reload and relinking.

Exit evidence covers old-processor results, rejected state, irregular-block parity and transactional replacement under GC and ARC on supported hosts. Exercise wasm32 package execution and explicit backend unavailability. Use the supported memory backend on native Windows.

### Granular package and interface [PRISM-ADVANCED-GRANULAR]

The [granular checklist](advanced-synthesis.md#granular-synthesis-synth-advanced-grains) records the current kernel, scheduler, asset and graph work. Use seeded event traces and window/resampling oracles for reverse and single-frame sources, replacement, capture wrap, zero/max density and exhaustion. Verify the shared sample/live kernel, independent pitch and scan, region boundaries, source retention and visible overload counts.

Drive sample import, notes, pitch changes at fixed scan, freeze, replacement during tails, capacity exhaustion, undo and save/reload through the Granular card and inspector. Exercise Size, Density, Pitch, Scan, Capture, Freeze, waveform/region gestures and numeric equivalents. Compare audio, applied state and screenshots; measure pitch and alias energy against the reference resampler. Demonstrate a frozen pad and time-stretched transient with reproducible event traces. GR-1 and Morphagene inform navigation, Quanta and Pigments inform sample voices, and Portal informs live capture.

### Physical package and interface [PRISM-ADVANCED-PHYSICAL]

Use the [physical processor checklist](advanced-synthesis.md#physical-and-modal-synthesis-synth-advanced-physical) for implementation status. Pin equations, boundary conditions, modal weights and ranges for bars, plates, membranes, strings and tubes. Establish impulse, tuning, decay and energy oracles. Progress from modal bodies and strike/pluck through fractional-delay strings/tubes to bounded bow/blow feedback. Exercise convergence failure, joint coupling, automation and material changes before live use.

Drive Exciter, Modal Body, String and Tube through palette selection, conditional controls, pickup/contact gestures and mode/energy observations. Save and reload struck metal, damped membrane, plucked/bowed string and blown tube patches after varying velocity, pickup, material and coupling. Collect isolated-mode tuning/T60, decay/energy bounds, solver-exhaustion evidence and headless/windowed screenshots. Rings/Elements inform exciter/body controls, Chromaphone informs coupling, and Collision/Tension inform gestures.

### Neural package and interface [PRISM-ADVANCED-NEURAL]

1. Select redistributable RAVE, DDSP, codec and text-to-audio fixtures independently. Record exact hashes, operator/runtime compatibility, licenses and available training-data provenance.
2. Benchmark load, warm-up, execution distribution, memory, hop/lookahead and cancellation on Windows and other target hosts. Select a backend only after the chosen models execute with bounded operator support. ONNX export and TorchScript are candidates.
3. Exercise manifest rejection, worker loss, late replies, shape errors and codebook bounds before wiring real inference through typed effects. Retain history across irregular blocks.
4. Verify RAVE transfer and DDSP Audio/MIDI with latency-compensated mixing and pitch confidence. Run real codec encode/decode fixtures.
5. Generate validated assets with cancellation. Preserve prompt, duration, seed and model revision. Exercise Preview and Insert independently.
6. Drive model selection, capability/error display, latent or harmonic/noise controls, codec inspection and generation. Cover missing assets, replacement, stale results, cancellation and save/reload.

Collect genuine audio, held-out measurements and lifecycle evidence for each capability. Record backend-specific nondeterminism in replay metadata. Offline text generation has an independent delivery gate. Live transfer also requires measured deadlines.

### Combined synthesis experiments [PRISM-ADVANCED-HYBRID]

Use one evolving body and learned grain contacts. Its energy affects the timing, location and texture of later contacts. The [combined-method contract](../../specs/synth/Synth.md#combined-grain-and-body-method-synth-advanced-hybrid) defines the state transition and work budget.

| Experiment | Deliverable | Decision evidence |
| --- | --- | --- |
| Body-dependent contacts | Analytic controller, then learned scheduler/texture decoder with energy-constrained forces | Initial body state changes future contact traces only with Interaction enabled. |
| Joint sound analysis | Estimate editable grain dictionary, contact envelope and material from reference audio | Held-out reconstruction and edits compared with independent parameter fitting. |
| Text-directed interactions | Generate contact/material trajectories for the same body | Prompt relevance, valid trajectories and predictable manual edits compared with generated-audio resampling. |

Review the full neural-granular, haptic modal/granular and differentiable-modal papers and broaden the prior-art search around contact-state feedback and energy-constrained learning. Define state units, force projection, material-change work, bounded transitions and failure behavior in native Osprey. Prove the energy bound against the production transition function. Establish analytic controls and ablation fixtures before training with disjoint recording/body splits, fixed seeds and matched budgets.

Evaluate offline, then run the learned controller through the neural lifecycle. Capture event traces, constrained updates, energy, latency and audio. Exercise the Combined module's Texture, Material, Body memory, Interaction and Energy controls, contact map, timeline, energy inspector, numeric/keyboard alternatives and controller selection. Compare ordinary routed grains/resonator, open-loop contacts, analytic feedback, learned control without energy features and the full method under matched loudness and latency. Retain negative results. Production work needs repeatable musical value within energy/resource limits. Joint analysis and text trajectories require independent evidence.

### Verification and delivery sequence [PRISM-ADVANCED-DELIVERY]

Shared preparation/assets/state precede granular and physical processing. Deliver the matching UI and journey at each stage. Neural work depends on backend selection and worker lifetime handling. The combined renderer depends on all three foundations, its native energy model, checked energy bound and analytic baseline. Measure that baseline before training the controller.

Use `make -C src/osprey/packages synth` for package verification and `make -C src/toolchain/targets/tests synth` for the host-free Prism suite when verification resumes. Windows has a synth launch target in its [platform Makefile](../../../src/toolchain/targets/windows/Makefile). Verify platform-specific launch, windowed and headless journeys through the appropriate scoped targets.

Delivery requires the [acceptance matrix](../../specs/synth/Synth.md#acceptance-contract-synth-advanced-verify) with real assets/models and public entry points. Verify Windows sandbox capabilities, keyboard accessibility, resize behavior and screenshots on every advertised host. Require a ten-minute maximum-workload run with zero starvation and callback allocation/lock instrumentation before claiming continuous playback. Record delivered capabilities and constraints in package and Prism documentation. Keep unverified milestones open.

Resolve resampling quality thresholds, membrane/tube reference models, runtime and redistributable model selection, platform memory/performance limits and the hybrid training dataset with their named experiments before optimizing dependent paths. Numerical defaults remain testable starting points.
