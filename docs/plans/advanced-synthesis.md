# Advanced synthesis implementation plan [SYNTH-ADVANCED]

Build the advanced synthesis ecosystem on `@osprey/synth`: independent, composable DSP modules that can be routed, modulated, automated and combined in Prism, Light Orchestra and future hosts. Prism is the first complete integration, not the definition of the library. Every module must produce real audio; a name in the palette or an attractive waveform is not implementation.

The authoritative contracts are [Synthesis.md](../specs/runtime/Synthesis.md), especially [SYNTH-GRAPH], [SYNTH-SIGNALS], [SYNTH-RENDER] and [SYNTH-DSP]. The [research catalogue](../research/synthesis/README.md) supplies sources and comparison experiments. The existing [Prism routing plan](prism-synth-routing.md) remains responsible for host lifecycle, live replacement, VST integration and release readiness; this plan adds the advanced DSP families and their complete user journeys.

**Current instruction:** automated testing is paused at the user's request on 19 November 2026. Work on implementation and the application; retain the verification checklist below for later. Do not run test suites, silently waive evidence requirements, or mark an unverified feature production-ready. Compilation and static analysis are separate from automated test execution.

## Current state and delivery order [SYNTH-ADVANCED-ORDER]

Existing code provides oscillator/envelope/filter primitives, typed graphs, parameter descriptors, Signals, reference block processing, VST transport and Prism's modular editor. Advanced synthesis is not already implemented by those foundations. The granular reference kernel now has generated models, Lean formulas, checked pool state, stereo rendering and Signal observation. It has passed strict static checking only; scheduling, graph integration, Prism controls and audio verification remain outstanding.

Deliver in this order, finishing a playable vertical slice before beginning the next large family:

- [ ] Establish shared resource, state, modulation and scheduling contracts under [SYNTH-ADVANCED-CORE].
- [ ] Deliver granular playback, scheduling and a playable Prism grain-cloud patch under [SYNTH-ADVANCED-GRAINS].
- [ ] Deliver modular excitation/resonators and playable physical instruments under [SYNTH-ADVANCED-PHYSICAL].
- [ ] Deliver wavetable and additive instruments under [SYNTH-ADVANCED-TABLES].
- [ ] Deliver spectral analysis, freeze, morphing and resynthesis under [SYNTH-ADVANCED-SPECTRAL].
- [ ] Deliver nonlinear/feedback effects and expressive phase synthesis under [SYNTH-ADVANCED-NONLINEAR] and [SYNTH-ADVANCED-PHASE].
- [ ] Deliver offline parameter fitting and learned-control adapters under [SYNTH-ADVANCED-LEARNED].
- [ ] Finish cross-platform application integration, documentation and the deferred verification gates under [SYNTH-ADVANCED-DELIVERY].

A vertical slice includes the DSP module, its graph factory, live parameter mapping, Signal observation, a Prism module card/inspector and a playable patch. Verification remains outstanding while the testing pause is in force, even if the implementation can be auditioned manually.

## Shared component foundation [SYNTH-ADVANCED-CORE]

### Canonical models and module boundaries [SYNTH-ADVANCED-CORE-MODELS]

- [ ] Declare configuration, prepared resources, persistent DSP state, commands, observations and faults in typeDiagram; generate their Osprey representations.
- [ ] Keep application models, canvas/native widgets, networking, file dialogs and operating-system audio devices outside the library.
- [ ] Author new DSP formulas in Lean and generate the Osprey mathematics; keep scheduling, ownership and error handling in ordinary Osprey modules.
- [ ] Give each component a stable kind/version and each instance an independent identity; never share mutable phase, delay memory, random state or voices between instances.
- [ ] Define prepare, activate, render, reset, release/tail and dispose transitions, including sample-rate/block-size changes.
- [ ] Separate immutable prepared resources from per-instance render state so multiple voices can share sample/table data safely.
- [ ] Expose typed failures for invalid configuration, exhausted capacity, missing resources, incompatible layouts and numerical failures; never fabricate a successful silent result.
- [ ] Remove superseded fixed-demo DSP after callers migrate; do not retain parallel implementations that drift.

### Ports, parameters and automation [SYNTH-ADVANCED-CORE-PORTS]

- [ ] Declare audio, control and timestamped-event ports, channel layouts and physical units for every new component.
- [ ] Define explicit adapters for mono/stereo, hertz/normalised values, bipolar/unipolar signals and control/audio rates.
- [ ] Declare names, units, defaults, minimum/maximum values, linear/logarithmic scaling, modulation semantics and automation eligibility for every parameter.
- [ ] Define modulation as explicit additive, multiplicative or replacement operations; reject ambiguous multiple writers unless a combiner is present.
- [ ] Add reusable smoothing/slew components with declared response and state continuity; avoid hiding unrelated smoothing inside each module.
- [ ] Deliver sample-offset events in deterministic order, including simultaneous note, parameter and trigger events.
- [ ] Define which changes apply at a sample boundary, block boundary or prepared-resource swap; expose pending versus applied values.
- [ ] Preserve surviving state across valid graph changes, and keep the previous running graph if preparation fails.

### Resources and bounded rendering [SYNTH-ADVANCED-CORE-RESOURCES]

- [ ] Define sample/table/impulse resources with identity, content hash, channel count, sample rate, length and version.
- [ ] Decode files, prepare tables, allocate pools and construct transforms outside the audio callback.
- [ ] Set explicit limits for voices, grains, modes, partials, FFT sizes, delay memory, automation queues and resource bytes.
- [ ] Define a capacity policy per resource: reject, steal with a release, or drop with a diagnostic; do not allocate on exhaustion.
- [ ] Specify source-boundary behaviour separately for looping, one-shot, reverse playback and interpolation.
- [ ] Avoid allocation, blocking locks, filesystem/network access, logging formatting and observer callbacks in production render paths.
- [ ] Define denormal handling, nonfinite-input policy, maximum internal levels and output headroom; do not silently clip intermediate graph signals.
- [ ] Declare algorithmic latency and tail length so preparation can align parallel routes and dispose of tails correctly.

### Signals and observers [SYNTH-ADVANCED-CORE-SIGNALS]

- [ ] Publish applied parameter revision and audio sample position with each coherent observation.
- [ ] Expose active voices/grains/modes, envelope state, meters, resource readiness, capacity pressure and typed diagnostics as appropriate to each family.
- [ ] Keep `Signal<T>` observation distinct from audio-rate DSP streams; observers must not execute on the audio thread.
- [ ] Publish through bounded storage; let slow subscribers coalesce snapshots without blocking rendering.
- [ ] Support selected-field subscriptions and independent observations of multiple same-kind instances.
- [ ] Make UI/controller writes typed commands; show whether a request is pending, applied or rejected.

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

Checked items here record reference implementation, reviewed in `src/osprey/packages/synth/src/grains.ospml` and `GrainMath.lean`; they do not satisfy the deferred verification gates. Static check: `osprey src/osprey/packages/synth --check --warnings-as-errors`.

- [x] Implement fractional source addressing with an explicitly documented interpolation mode and quality limitation.
- [x] Support forward, reverse and near-zero playback rates with defined zero-rate behaviour.
- [ ] Implement Hann and configurable attack/sustain/release grain windows; guarantee declared endpoint behaviour.
- [x] Keep grain duration independent from playback rate and source-position scanning.
- [ ] Add per-grain amplitude and equal-power stereo placement; define mono and stereo-source policies explicitly.
- [x] Retain cursor, lifetime and window phase across arbitrary block boundaries.
- [x] Implement a bounded voice pool with stable identities and an explicit full-pool response.
- [ ] Define release/steal behaviour so overload does not abruptly discard a nonzero grain without the chosen policy.
- [ ] Expose active-grain count, requested/applied density, rejected/stolen grains and pool capacity through Signals.
- [x] Document summing/headroom policy, distinguishing peak control from density-dependent loudness compensation.

### Schedulers and sample resources [SYNTH-ADVANCED-GRAINS-SCHEDULING]

- [ ] Implement explicit sample-position grain events, independent of host block size.
- [ ] Add periodic/quasi-synchronous scheduling and seeded asynchronous scheduling.
- [ ] Add independently controlled distributions for onset, duration, source position, pitch and pan.
- [ ] Specify seed persistence, reset and reseed semantics; identical state and events must reproduce the same schedule.
- [ ] Support overlapping grains across loop boundaries without reading outside prepared resources.
- [ ] Add immutable resource replacement: existing grains finish against their owned resource while new grains adopt the new one.
- [ ] Add bounded live capture/freeze with an explicit read/write ownership strategy.
- [ ] Add optional pitch-synchronous scheduling as a separate mode with its own analysis/latency contract.

### Graph and Prism integration [SYNTH-ADVANCED-GRAINS-INTEGRATION]

- [ ] Add granular factories and event/control ports to the shared scheduler; do not make a Prism-only synthesis path.
- [ ] Add Grain Cloud, Grain Scheduler and Sample Source entries to the module browser.
- [ ] Provide source selection, waveform/position display, density, duration, pitch, spread, seed and freeze controls in the existing inspector design.
- [ ] Map keyboard/phone expression to declared parameters rather than hard-coded instrument-specific commands.
- [ ] Ship playable cloud, rhythmic-grain and frozen-texture patches, each with useful initial levels and visible modulation routes.
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

- [ ] Implement deterministic impulse, filtered-noise and external-audio exciters.
- [ ] Implement fractional-delay reads with a stated interpolation response, tuning error and modulation policy.
- [ ] Implement frequency-dependent loss and dispersion as independent components.
- [ ] Implement damped resonators with prepared coefficients and explicit persistent state.
- [ ] Define stable parameter domains and reject invalid coefficients before activation.
- [ ] Define how control changes alter stored energy; prevent abrupt parameter changes from introducing unintended unstable energy.
- [ ] Keep excitation strength and output gain separate so expression remains meaningful.

### Playable instruments [SYNTH-ADVANCED-PHYSICAL-INSTRUMENTS]

- [ ] Compose a plucked-string instrument from excitation, delay and loss primitives.
- [ ] Add strike and pickup position controls with documented effects on the model.
- [ ] Add sustained excitation with a bounded feedback/nonlinear interaction model for bowed/blown-style instruments.
- [ ] Implement modal banks with bounded mode count, independent frequencies, gains and decay times.
- [ ] Add inharmonic mode sets for bells, bars, membranes and metallic bodies; label approximations rather than claiming full mechanical simulation.
- [ ] Provide mode excitation weights derived from a declared strike/pickup-position model.
- [ ] Add polyphonic voice allocation, release tails and explicit voice stealing for physical instruments.
- [ ] Preserve tuning and decay when sample rate changes through a documented re-preparation/state-transfer policy.
- [ ] Treat finite-difference mesh models as a separate bounded component with a stated discretisation/stability domain, not an implicit alternative inside the modal bank.

### Graph and Prism integration [SYNTH-ADVANCED-PHYSICAL-INTEGRATION]

- [ ] Expose Exciter, Resonator, Modal Bank, Fractional Delay and Waveguide modules independently in the palette.
- [ ] Provide pitch, damping, brightness, strike position, pickup position and mode controls through standard parameter descriptors.
- [ ] Show meaningful activity/decay observations rather than decorative waveforms disconnected from audio.
- [ ] Ship plucked-string, struck-metal and externally excited resonator patches with editable routing.
- [ ] Allow granular output, sample playback and VST audio to excite physical models through ordinary compatible audio ports.

## Wavetable and additive synthesis [SYNTH-ADVANCED-TABLES]

- [ ] Add prepared wavetable banks with explicit sample rate, table length, phase alignment and resource ownership.
- [ ] Generate/select bandlimited table levels for the supported pitch range; document residual aliasing and modulation limits.
- [ ] Support continuous phase, table-position interpolation and predictable table replacement.
- [ ] Add table-position, phase, frequency and amplitude modulation through typed ports.
- [ ] Define hard-sync and reverse/near-zero frequency behaviour rather than inheriting accidental oscillator semantics.
- [ ] Implement bounded additive partial banks with independent frequency, phase and amplitude trajectories.
- [ ] Support harmonic and inharmonic partial layouts, smooth partial-count changes and explicit upper-frequency handling.
- [ ] Provide reusable partial-envelope controls without baking one timbre into the oscillator.
- [ ] Add Wavetable and Partial Bank module cards and resource controls in Prism.
- [ ] Ship a morphing-table instrument and a controllable inharmonic additive patch.

## Spectral synthesis and processing [SYNTH-ADVANCED-SPECTRAL]

### Transform and resynthesis foundation [SYNTH-ADVANCED-SPECTRAL-CORE]

- [ ] Implement a reusable prepared transform backend with bounded supported sizes and scratch memory.
- [ ] Define analysis/synthesis windows, hop sizes, normalisation, padding and overlap/add contracts.
- [ ] Specify algorithmic latency and startup/shutdown behaviour, including partial final windows.
- [ ] Make spectral frames a typed representation with channel, bin, frequency, magnitude/phase and timestamp conventions.
- [ ] Define explicit audio↔spectral adapters; do not route spectral frames through ordinary audio ports implicitly.
- [ ] Preserve phase/state across blocks and transport changes using a documented phase policy.
- [ ] Define CPU/memory budgets before exposing large transforms or multiple simultaneous analysis branches.

### Modules and expressive controls [SYNTH-ADVANCED-SPECTRAL-MODULES]

- [ ] Implement spectral freeze with explicit capture and release, avoiding accidental phase discontinuities.
- [ ] Implement spectral morphing with declared magnitude and phase interpolation policies.
- [ ] Add spectral filtering, bin remapping and controlled spectral diffusion/smear as composable transformations.
- [ ] Add pitch/time manipulation only with its own phase/transient handling and latency contract.
- [ ] Expose analysis/resynthesis separately so multiple transformations can share one analysis where compatible.
- [ ] Add spectral displays driven by bounded observer snapshots, not analysis on the UI thread.
- [ ] Ship freeze, cross-synthesis and spectral-motion patches using visible modules and cables.

## Nonlinear, spatial and feedback processing [SYNTH-ADVANCED-NONLINEAR]

- [ ] Implement reusable waveshapers with explicit input range, gain staging and DC behaviour.
- [ ] Add antiderivative antialiasing where the transfer function permits it, including near-equal-input handling.
- [ ] Add prepared oversampling/downsampling stages with explicit factors, filters, latency and resource cost.
- [ ] Compare oversampling and ADAA rather than assuming one method is universally preferable.
- [ ] Implement fractional/modulated delay, diffusion and feedback matrices as separate building blocks.
- [ ] Build a configurable feedback-delay-network reverb with damping, size, diffusion and freeze controls.
- [ ] Define stability domains and feedback gain limits; freeze must have an explicit energy policy.
- [ ] Add frequency shifting, ring modulation and pitch-shifted feedback where supported by the declared algorithms.
- [ ] Keep feedback loops explicit in the graph and require causal delay; bound any nonlinear solver's iteration count.
- [ ] Define failure handling when convergence or stability requirements are not met.
- [ ] Expose Delay, Diffuser, Feedback Matrix, Waveshaper, Frequency Shifter and Reverb in Prism.
- [ ] Ship an evolving feedback texture and a granular-through-resonator/reverb patch with safe initial output gain.

## Phase transformation and experimental oscillators [SYNTH-ADVANCED-PHASE]

- [ ] Separate phase generation, phase transformation, waveform lookup and amplitude shaping into composable components.
- [ ] Implement frequency modulation, phase modulation and phase distortion with explicit units and state continuity.
- [ ] Add phase reset/sync and sample-offset trigger semantics.
- [ ] Implement WPPM as a named research candidate derived from the cited source; preserve its exact parameter definitions and provenance.
- [ ] Define antialiasing/oversampling behaviour for discontinuities and rapidly varying modulation.
- [ ] Support audio-rate modulation where the kernel contract allows it; do not disguise block-rate controls as audio-rate processing.
- [ ] Add phase/operator modules and editable routing in Prism, including modulation depth and scaling adapters.
- [ ] Ship a playable phase-network patch with several useful macro mappings.

## Learned controls and offline discovery [SYNTH-ADVANCED-LEARNED]

- [ ] Expose offline parameter trajectories for the same production DSP operators; avoid a separate incompatible synthesis model.
- [ ] Add parameter fitting with explicit objective functions, constraints, seed and stopping conditions.
- [ ] Record model/data/licence provenance and distinguish training data from evaluation material.
- [ ] Keep learned results editable as ordinary parameters, tables or automation curves.
- [ ] Run training, patch search and expensive inference off the audio callback and outside the library's mandatory runtime dependencies.
- [ ] Define cancellation, progress, resource limits and explicit failure results for long offline jobs.
- [ ] Treat a neural waveform generator as an optional later component requiring its own latency, memory and degradation contract.
- [ ] Add application controls to preview/apply/reject fitted parameters without overwriting a working patch silently.

## Prism and external-controller experience [SYNTH-ADVANCED-PRISM]

- [ ] Preserve the existing Prism logo, original node styling, visible cables, inspector and bottom-docked closable keyboard.
- [ ] Group module browsing by Sources, Grains, Physical, Spectral, Modulators, Effects, Utilities and Output; support search and drag/click insertion.
- [ ] Give every inserted node real ports, useful defaults and an inspector generated from its descriptors.
- [ ] Make cable creation/removal, node movement/removal and Undo/Redo work for every advanced family.
- [ ] Show resource-loading and failure states on the affected node; avoid treating unavailable samples/models/plugins as a silent working instrument.
- [ ] Bind controls to applied parameter Signals and show pending/rejected changes distinctly.
- [ ] Provide macro controls that map to ordinary typed parameters with declared ranges and smoothing.
- [ ] Expose the same control commands to Light Orchestra/phone adapters without adding networking or session state to the synth library.
- [ ] Save complete patches, resources, seeds, module versions, automation and controller mappings; load incompatible/missing resources explicitly.
- [ ] Keep advanced modules available to headless/offline consumers without requiring Prism or the native UI package.

## Deferred acceptance checklist [SYNTH-ADVANCED-VERIFY]

**Run these only after the current automated-testing pause is lifted.** Each family remains unverified until its evidence exists; do not check a family off solely because it compiles or produces one audible note.

### Correctness and stability [SYNTH-ADVANCED-VERIFY-CORRECTNESS]

- [ ] Run loaded end-to-end patch journeys with repeated same-kind instances, fan-out, modulation, removal, undo, save/load and failures.
- [ ] Compare arbitrary block partitions against continuous rendering while preserving event ownership at exact boundaries.
- [ ] Exercise sample rates 44.1/48/96/192 kHz and supported block sizes, including unequal and one-sample partitions.
- [ ] Verify grain windows, wrapping, reverse playback, deterministic schedules, pool exhaustion and resource replacement.
- [ ] Verify physical-model tuning, decay/energy bounds, rate changes and extreme stable parameter values.
- [ ] Verify wavetable/additive spectral behaviour, phase continuity, modulation and partial/table replacement.
- [ ] Verify spectral reconstruction, overlap/add normalisation, latency, freeze/release and transport discontinuities.
- [ ] Compare nonlinear/phase outputs with independent higher-quality references under level/latency matching.
- [ ] Inject nonfinite data, malformed resources, invalid units, stale revisions, overflow and unavailable resources; require explicit failure and preserved prior state.

### Realtime and observer behaviour [SYNTH-ADVANCED-VERIFY-REALTIME]

- [ ] Instrument allocation, locking and unbounded operations in the compiled native callback path.
- [ ] Measure worst observed callback time, queue starvation, CPU, memory and latency on named hardware with declared workloads.
- [ ] Exercise simultaneous grains, modes, spectral transforms, feedback and VST nodes under explicit resource budgets.
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
- [ ] Record the source paper/reference and exact algorithm/version for each research-derived implementation.
- [ ] Link code, specifications, patches and eventual test evidence through the section IDs in this plan.
- [ ] Include reproducible example patches for every delivered family and at least one patch combining grains, physical modelling, spectral processing and modulation.
- [ ] Mark experimental/production status from measured evidence; record remaining limitations without weakening the contracts.
- [ ] Reconcile this checklist with [Synthesis.md](../specs/runtime/Synthesis.md) and the [Prism routing plan](prism-synth-routing.md), preserving their unfinished requirements.
- [ ] Commit work in reviewable module-sized steps, preserve unrelated unfinished work and remove only task-owned temporary processes/artifacts.
