# Osprey synthesis toolkit [SYNTH]

The [advanced synthesis proposal](SynthAdvanced.md#advanced-synthesis-proposal-synth-advanced) specifies neural, granular, physical and combined methods with their Prism interfaces. The [implementation checklist](../../plans/synth/advanced-synthesis.md) tracks delivery and verification.

Status — implementation in progress. The initial package extracts Prism's pure pitch, waveform, modulation, sample and WAV functions. Typed graph validation/scheduling, reactive parameter snapshots and stateful scalar reference oscillators, ADSR and filters now have passing package journeys. The real-time renderer and advanced algorithms remain requirements, not completed features. Research sources and candidate experiments are in [the research guide](SynthDsp.md#sources-and-experiments-synth-research).

`@osprey/synth` is an independent synthesis library for instruments, effects, offline rendering and future plug-in hosting. It contains no UI, native widgets, canvas rendering, networking, session management or operating-system audio-device code. Prism and Light Orchestra are examples using the library. The orchestra's number of phones and instruments must not constrain library design.

## Components and routing [SYNTH-GRAPH]

A component has an instance identity, typed input/output ports, parameter descriptors, explicit persistent DSP state and observable state. A graph composes components by connecting ports, chaining stages, splitting outputs, mixing inputs and routing modulation. Component kinds include oscillators, noise, envelopes, LFOs, filters, amplifiers, panners, mixers, delays, nonlinearities, spectral processors and future external plug-ins. Components are reusable without the graph editor or any UI framework.

Ports distinguish audio, control and timestamped events. Audio ports declare channel layouts; parameter ports declare units, ranges and modulation semantics. Connections cannot silently reinterpret hertz as normalised values, mono as stereo, events as samples or control-rate values as audio-rate signals. Explicit adapters handle rate conversion, channel conversion, scaling and smoothing. Multiple inputs require a declared mixer/combiner rather than an implicit ordering rule.

Graph preparation validates missing/duplicate identities, endpoint existence, port direction, rate/channel compatibility, resource budgets and cycles. Feedback requires an explicit delay with a documented minimum; algebraic loops are rejected unless a dedicated bounded solver component owns them. Preparation determines scheduling, buffer reuse and latency compensation before rendering. Rejected edits preserve the running graph. Graph replacement occurs at a documented boundary with an explicit state-transfer/crossfade policy.

A track patch can hold any number of components and cables that fit in memory. Both the editor and project loader validate endpoint identities, port compatibility and exclusive inlet ownership. Among components ready to run, the scheduler chooses the earliest in project declaration order. A delay publishes its previous sample before consuming its input. The offline clip executor checks the sum of retained output channel widths multiplied by block length against its 16,777,216-sample budget before allocating audio buffers. The same patch can therefore admit a short block when a longer block exceeds available clip storage.

### Channels [SYNTH-GRAPH-CHANNELS]

The channel count is unlimited. A layout is an ordered list of named channels; stereo (left, right) is the default, and 5.1, 7.1, 7.1.4 and any wider layout are equally valid. Every instrument and processor renders whatever layout its output port is prepared with: oscillators, samplers, grains, physical models and effects are written per channel over the layout, never as fixed mono or stereo code paths. Mono is the one-channel layout, not a special case.

Four graph components handle channels, and nothing else changes channel count:

- **Split** turns one N-channel port into N one-channel outputs, in layout order.
- **Join** turns N one-channel inputs into one N-channel port with a declared layout.
- **Route** is a channel matrix: each output channel takes any input channel, or silence. It reorders, duplicates and drops channels explicitly.
- **Mix** sums several ports of one layout into one, with per-input gain. A panner places a mono source across any layout with equal-power gains whose squares sum to 1.

A cable connects ports of identical layout only. Converting stereo to 5.1, downmixing a WAV or upmixing a voice is a visible Route or Mix node in the graph, so no channel is silently dropped. A project track declares its output layout, and Master's layout is the project's. Every node carries the project's layout, stereo by default, from source to Master.

All canonical public data models are declared in typeDiagram and generated into Osprey/host representations. Stored patches include schema and component versions; unsupported versions return an explicit error. No silently substituted component or zero-valued parameter hides a failed load.

### Reference graph replacement [SYNTH-GRAPH-REPLACE]

`Repatch::apply` validates and prepares a complete replacement outside the callback and returns its audio state and applied observation together. Its caller publishes that result at a block boundary. Compatible surviving identities retain oscillator phase, filter integrators, delay memory and valid parameter values; changed processor kinds or port layouts start with fresh state. Removed nodes lose their state and parameters. The sample position is preserved and the observation revision advances, invalidating stale UI commands. Missing surviving DSP state, incomplete audio routes, invalid retained values and stale snapshot positions are explicit errors; the caller retains its previous graph on rejection. This scalar reference path allocates during preparation and is not a real-time callback implementation. Topology edits may change output amplitude abruptly; a crossfade policy remains required before claiming click-free live editing.

## Signals and external interfaces [SYNTH-SIGNALS]

Every component exposes typed Signals for its parameters and observable state, using Osprey's `Signal<T>` mechanism without depending on `@osprey/native`. A consumer can observe one component, selected parameter fields, voice activity, envelope position, meters, transport and diagnostics. Instance identity distinguishes two components of the same type. Parameter descriptors provide names, units, ranges, default values, scaling and automation eligibility, allowing an application to build controls without the synth library drawing them.

Signal reads observe immutable snapshots identified by the applied parameter revision and audio sample position. A UI, recorder, remote controller or headless test can supply the Signal handler. UI writes become typed commands/automation events, not mutation of memory that the audio callback is reading. Accepted parameter changes carry an applied revision/sample position so the interface can distinguish requested values from audible state.

Audio-rate signals are DSP streams; `Signal<T>` observation is the reactive state interface. They are related but not interchangeable. The audio thread publishes through bounded preallocated storage without executing observers. A non-real-time consumer publishes coherent Signal snapshots at a configurable cadence (initial UI target 30–60 Hz); slow subscribers coalesce to the newest snapshot. Meter peaks are accumulated over the interval, so coalescing does not erase overloads. Losing UI frames never loses note-off events or changes DSP timing.

A disconnected UI must not stop audio. With no observers, DSP runs with the same output. Reattachment reads a complete current snapshot. Diagnostics use bounded codes/counters from the callback; formatting, structured logging and delivery occur outside it. Exposing Signals alone does not prove thread safety: publication and ownership require concurrency tests.

## Showcase songs [SYNTH-SHOWCASE]

The shipped daw songs together play every synthesis family the toolkit has, each on its own audible track: subtractive, FM/phase modulation, phase distortion, wavetable, additive, granular, spectral (morph, freeze and formant pitch), and physical (waveguide string, bowed string, modal bar and bell, mesh membrane, resonator), plus the sampled intro. A test opens each song and checks that every family is present, so a family cannot quietly drop out.

## Routed modulation [SYNTH-GRAPH-MODULATION]

A normalized sine LFO is a separate control-rate component with its own phase and frequency. A controlled-gain node takes distinct audio and normalized-control inputs; one control sample broadcasts across all audio channels while audio cables retain their declared width. The transfer functions are authored in `waves.ospml`, and the renderer shares oscillator state handling with audio oscillators. Missing controls and values outside zero through one are explicit errors. Native menu actions and canvas cable actions use the same rate/unit/channel validation.

The current graph clip path renders modulation at sample resolution. This establishes LFO-driven stereo amplitude modulation. Hosted parameter routes are described below; continuous device processing remains delivery work.

## Verification and delivery [SYNTH-VERIFY]

- [x] Extract reusable synthesis from Prism without introducing a UI dependency into the package.
- [x] Run Prism's existing full interaction/voice suite before and after extraction.
- [x] Collect foundational and recent primary research with implementation experiments.
- [x] Add independent package tests and package governance/coverage integration (package journeys and a 90% minimum own-source line coverage gate).
- [x] Implement typed graph construction/validation, routing, chaining and explicit feedback scheduling; the mono reference graph now executes sources, mixing, filters, gain and delay feedback.
- [x] Expose component parameters/state through typed Signals and verify independent component IDs, applied revisions and rejected stale edits; concurrent publication remains pending.
- [x] Implement stateful reference DSP and reproducible sample-accurate block processing, including parameter automation and per-component peak/RMS observations.
- [ ] Verify generated native callback allocation/locking behaviour and target-device deadlines.
- [ ] Establish spectral, stability, continuity, polyphony and overload evidence.
- [ ] Integrate Prism and Light Orchestra through the same reusable engine.
- [ ] Add VST3 hosting after the component/graph boundary is verified.

Test complete instrument journeys with many notes, automation events and routing changes. Use isolated numerical tests where mathematical or real-time isolation is necessary. Compare against independent analytical or high-precision references; record seeds, sample rates, block partitions and tolerances. Keep exact PCM goldens only where cross-target numerical behaviour is defined, and retain listening artefacts under `target/` for review. Screenshot tests belong to consuming applications, not this UI-free library.
