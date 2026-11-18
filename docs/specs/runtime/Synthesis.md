# Osprey synthesis toolkit [SYNTH]

Status — implementation in progress. The initial package extracts Prism's pure pitch, waveform, modulation, sample and WAV functions. Typed graph validation/scheduling, reactive parameter snapshots and stateful scalar reference oscillators, ADSR and filters now have passing package journeys. The real-time renderer and advanced algorithms remain requirements, not completed features. Research sources and candidate experiments are in [the research guide](../../research/synthesis/README.md).

`@osprey/synth` is an independent synthesis library for instruments, effects, offline rendering and future plug-in hosting. It contains no UI, native widgets, canvas rendering, networking, session management or operating-system audio-device code. Prism and Light Orchestra are examples using the library. The orchestra's number of phones and instruments must not constrain library design.

## Components and routing [SYNTH-GRAPH]

A component has an instance identity, typed input/output ports, parameter descriptors, explicit persistent DSP state and observable state. A graph composes components by connecting ports, chaining stages, splitting outputs, mixing inputs and routing modulation. Component kinds include oscillators, noise, envelopes, LFOs, filters, amplifiers, panners, mixers, delays, nonlinearities, spectral processors and future external plug-ins. Components are reusable without the graph editor or any UI framework.

Ports distinguish audio, control and timestamped events. Audio ports declare channel layouts; parameter ports declare units, ranges and modulation semantics. Connections cannot silently reinterpret hertz as normalised values, mono as stereo, events as samples or control-rate values as audio-rate signals. Explicit adapters handle rate conversion, channel conversion, scaling and smoothing. Multiple inputs require a declared mixer/combiner rather than an implicit ordering rule.

Graph preparation validates missing/duplicate identities, endpoint existence, port direction, rate/channel compatibility, resource budgets and cycles. Feedback requires an explicit delay with a documented minimum; algebraic loops are rejected unless a dedicated bounded solver component owns them. Preparation determines scheduling, buffer reuse and latency compensation before rendering. Rejected edits preserve the running graph. Graph replacement occurs at a documented boundary with an explicit state-transfer/crossfade policy.

All canonical public data models are declared in typeDiagram and generated into Osprey/host representations. Stored patches include schema and component versions; unsupported versions return an explicit error. No silently substituted component or zero-valued parameter hides a failed load.

### Reference graph replacement [SYNTH-GRAPH-REPLACE]

`Repatch::apply` validates and prepares a complete replacement outside the callback and returns its audio state and applied observation together. Its caller publishes that result at a block boundary. Compatible surviving identities retain oscillator phase, filter integrators, delay memory and valid parameter values; changed processor kinds or port layouts start with fresh state. Removed nodes lose their state and parameters. The sample position is preserved and the observation revision advances, invalidating stale UI commands. Missing surviving DSP state, incomplete audio routes, invalid retained values and stale snapshot positions are explicit errors; the caller retains its previous graph on rejection. This scalar reference path allocates during preparation and is not a real-time callback implementation. Topology edits may change output amplitude abruptly; a crossfade policy remains required before claiming click-free live editing.

## Signals and external interfaces [SYNTH-SIGNALS]

Every component exposes typed Signals for its parameters and observable state, using Osprey's `Signal<T>` mechanism without depending on `@osprey/native`. A consumer can observe one component, selected parameter fields, voice activity, envelope position, meters, transport and diagnostics. Instance identity distinguishes two components of the same type. Parameter descriptors provide names, units, ranges, default values, scaling and automation eligibility, allowing an application to build controls without the synth library drawing them.

Signal reads observe immutable snapshots identified by the applied parameter revision and audio sample position. A UI, recorder, remote controller or headless test can supply the Signal handler. UI writes become typed commands/automation events, not mutation of memory that the audio callback is reading. Accepted parameter changes carry an applied revision/sample position so the interface can distinguish requested values from audible state.

Audio-rate signals are DSP streams; `Signal<T>` observation is the reactive state interface. They are related but not interchangeable. The audio thread publishes through bounded preallocated storage without executing observers. A non-real-time consumer publishes coherent Signal snapshots at a configurable cadence (initial UI target 30–60 Hz); slow subscribers coalesce to the newest snapshot. Meter peaks are accumulated over the interval, so coalescing does not erase overloads. Losing UI frames never loses note-off events or changes DSP timing.

A disconnected UI must not stop audio. With no observers, DSP runs with the same output. Reattachment reads a complete current snapshot. Diagnostics use bounded codes/counters from the callback; formatting, structured logging and delivery occur outside it. Exposing Signals alone does not prove thread safety: publication and ownership require concurrency tests.

## Rendering and real-time contract [SYNTH-RENDER]

Preparation takes sample rate, channel layout, maximum block size, voice/component counts and event capacity. Rendering uses caller-owned/preallocated buffers and explicit component state. The callback performs no allocation/free, GC, blocking locks, file/network I/O, logging, model loading or unbounded work. Osprey compilation to native code by itself does not prove this property; inspect and instrument the generated callback path. If current runtime facilities cannot satisfy it, report the gap and keep that path outside the callback.

Events carry sample offsets and stable ordering within a block. Note ownership, retrigger, sustain, per-note expression, voice stealing and release policy are explicit. Automation supports ramps with declared interpolation, avoiding accidental block-rate steps. Different legal block partitions of one event timeline must yield equivalent sound and final state within a documented numerical tolerance.

Initial production rate targets are 44.1, 48 and 96 kHz, with irregular block sizes including 1, 31, 64, 127, 256 and 1024 samples. A platform may negotiate others only through checked preparation. Rate changes, device loss, transport jumps and reset have defined state transitions. Peak CPU, callback deadline misses, latency, memory and output quality are measured separately on named hardware. No hard-real-time guarantee is implied by a desktop operating system.

Invalid configuration is rejected before activation. Runtime failures return a typed fault and bounded diagnostics. A host may explicitly choose to silence a failed stream, but it must also surface the failure; silence is not a fabricated successful render. Non-finite input, parameter overflow and numerical instability have specified handling and tests.

## DSP quality [SYNTH-DSP]

The extracted Prism renderer is a reference/example path: 11,025 Hz, unsigned 8-bit WAV output, naive periodic waves and a fundamental-gain approximation to filtering. It is not the production engine. Production kernels require continuous phase/state, high-resolution floating-point audio, bounded envelopes, smoothing, proper stateful filters and documented antialiasing behaviour. Avoid retaining parallel versions once the production replacement is verified.

Oscillator quality is evaluated under pitch changes, pulse-width modulation, phase modulation, hard sync and negative/near-zero rates where supported. Filters are evaluated under cutoff/resonance automation and feedback. Nonlinear processors require explicit oversampling/ADAA policies and declared latency. Modulation and mixing remain composable rather than being baked into one fixed subtractive voice.

Research candidates include bandlimited/corrected oscillators, wavetable/additive/spectral synthesis, physical/modal models, granular engines, expressive phase shaping and differentiable/learned controls. The detailed implementation and acceptance checklist is the [advanced synthesis plan](../../plans/advanced-synthesis.md) [SYNTH-ADVANCED]. Adoption requires the [research experiments](../../research/synthesis/README.md#research-to-delivery-experiments-synth-research-experiments), not a novelty claim. Training and expensive patch inference remain outside the required audio callback.

## VST3 hosting [SYNTH-HOST-VST]

VST3 **hosting** is an active requirement of the [Prism expansion](../../plans/prism-synth-routing.md). The isolated worker now scans, describes and processes real plug-ins, and Prism can create and route their typed graph nodes, then audition a real instrument/effect chain as stereo PCM. Continuous device rendering remains in progress. The purpose is to load existing VST3 instruments/effects into a synthesis graph. Producing Osprey VST3 plug-ins is a separate capability and is not substituted for hosting.

An external node declares audio buses, event buses, parameter identifiers, sample-accurate automation support, latency, tail length and saved state. Host adapters map these to the same graph ports and parameter Signals as built-in components. Plug-in metadata/parameter IDs remain stable across graph saves; missing binaries or changed layouts produce visible errors rather than loading a different plug-in silently.

Scanning, loading, activation and state restoration run outside the callback. The design must account for unsupported layouts, dynamic latency, bypass, crash/hang handling and untrusted plug-in code. Scan out of process; evaluate process isolation for playback with explicit latency/cost measurements. No claim that an arbitrary third-party plug-in respects Osprey's memory-safety or callback contract. Host integration includes actual DAW-style lifecycle and state/automation tests. Use the [official SDK](https://github.com/steinbergmedia/vst3sdk) as the compatibility source.

## Worker audio transport [SYNTH-HOST-STREAM]

`HostPackets` encodes checked interleaved float32 audio, MIDI and float64 normalized automation into the worker's little-endian protocol. Clips are split into at most 256-frame blocks; event offsets remain sample-accurate. The transport bounds channels, frame counts, notes and parameters before encoding. `HostResponses` checks the exact expected channel layout and frame count, finite samples, stable bounded latency and the absence of trailing data before exposing audio to the graph. Files and packets share `PcmWords`, and `WaveFiles.interleaved` records channel-aligned IEEE float32 PCM with a frame-count fact chunk.

The native worker accepts binary persistent streams or bounded hexadecimal clips over stdin/stdout. The text path has exact-PCM parity with binary processing. It is suitable for reference previews; it is not a realtime device callback implementation. The GPUI process-input adapter bounds transport memory and child lifetime independently of output EOF.

## Hosted graph clips [SYNTH-HOST-GRAPH]

`GraphClips` evaluates a frozen acyclic graph in dependency order, sharing the reference oscillator, filter, gain, mixer and delay kernels through `ClipKernels`. A hosted node suspends pure execution with its checked audio, MIDI and parameter request; the consuming application supplies a validated worker response before scheduling downstream nodes. Extra ports, missing cables, malformed cached buffers and excessive resources are errors. Explicit mono-to-stereo conversion is a component; a cable never changes channel count. This bounded offline path rejects feedback and nonzero plug-in latency rather than claiming realtime feedback or latency compensation.

Prism snapshots the graph and plug-in bindings when an audition begins. Editing or deleting a live node cannot change the running snapshot; stale/duplicate process completions cannot advance it. The native journey drives a real mda JX10 through two real ADelay instances and stereo playback, independently checking every effect's PCM against the preceding output. This proves hosted graph playback, not continuous device callbacks or persistence across auditions.

## Stateful multichannel blocks [SYNTH-RENDER-BLOCKS]

`ClipKernels.block` accepts and returns independent per-channel kernel state alongside interleaved samples at the hosted path's 48 kHz rate. Unequal block partitions retain oscillator/LFO phase, filter integrators and delay history. The component must exactly match its prepared graph entry, and state identities, channel counts and block capacities are checked before processing. The existing whole-clip entry point initializes state and delegates to this same implementation.

`GraphClips.start`, `next` and `replied` schedule those states across complete acyclic graph blocks. A typed pending request owns the frozen graph, current block buffers and kernel states; only its named external node can supply the checked response. The graph clock advances once the entire block completes. MIDI note ownership and the 128-frame automation grid use that absolute clock, including unequal block sizes and positions beyond the bounded whole-clip window. A continuing graph that loses or duplicates required kernel state is rejected.

This is producer-side, allocating reference execution. It does not establish allocation-free device callbacks, persistent VST sessions in Prism, graph-wide streaming scheduling or uninterrupted device playback; those integration steps remain required.

## MIDI generators [SYNTH-HOST-MIDI]

A MIDI note component exposes whole-number pitch, normalized velocity and an exact gate length in frames. `MidiClips.window` assigns note-on and note-off to half-open sample windows, preserving ownership at block boundaries. Fractional pitches/gates, invalid velocities and out-of-range windows return errors. Event outputs connect only to compatible event inputs; audio outputs remain separate. Sequencing, polyphonic generators and continuous live note control remain delivery work.

## Routed modulation [SYNTH-GRAPH-MODULATION]

A normalized sine LFO is a separate control-rate component with its own phase and frequency. A controlled-gain node takes distinct audio and normalized-control inputs; one control sample broadcasts across all audio channels while audio cables retain their declared width. The transfer functions are authored in `Waves.lean`, and the renderer shares oscillator state handling with audio oscillators. Missing controls and values outside zero through one are explicit errors. Native menu actions and canvas cable actions use the same rate/unit/channel validation.

The current graph clip path renders modulation at sample resolution. This establishes LFO-driven stereo amplitude modulation. Hosted parameter routes are described below; continuous device processing remains delivery work.

## Hosted parameter modulation [SYNTH-HOST-MODULATION]

`HostModulation.connect` attaches an explicitly normalized control input to an automatable VST parameter and connects it transactionally. Parameters use stable native IDs, and incompatible sources are rejected without changing the previous graph. Disconnecting the cable retains the port for later routing and restores the configured parameter value. Non-automatable controls are displayed as unavailable for editing in this host; editing them is an explicit error rather than a silent no-op.

The clip renderer samples connected control streams on a 128-frame automation grid. Default values are sent once at frame zero; connected parameters receive exact offsets and normalized values at subsequent grid points. A clip has a bounded total of 1024 points, and excess automation returns an error. This is a declared control update interval, not a promise that every plug-in interpolates identically. The native JX10 journey captures the exact filter-frequency points and compares the resulting audio against a replay with only the initial control setting retained.

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


### Reference continuous device execution [SYNTH-DEVICE-QUEUE]

Prism owns one bounded audio-device session. Completed graph blocks are encoded outside the callback and transferred as canonical 48 kHz mono/stereo float32 WAV. macOS queues them through AudioQueue and hosts VST sessions alongside the device. Windows converts the float samples to 16-bit PCM for WinMM and reclaims buffers as the device returns them. Both adapters bound the queue and accept 1–8192 frames with finite samples within -1..1. Invalid layouts, lengths, amplitudes, queue timeouts and mismatched acknowledgments are explicit failures.

Prism transfers 2048 graph frames at a time, while VST processing packets remain at most 256 frames. A typed pending device record retains the exact graph, sample clock and component states until the device acknowledges the transfer. Audio slices are reused across blocks. Audition drains every accepted frame; continuous Play schedules further blocks until Stop. Stop during opening first adopts the returned handle and then releases it, preventing a late-open resource leak. Host shutdown also releases owned sessions.

The reference producer allocates and runs outside the device callback. A native loaded-binary journey streamed 985088 frames through JX10, two ADelay instances, LFO modulation and the real device with zero queue-empty observations, then verified worker disposal. Queue-empty counters record the device's observations. Prism applies graph edits at restart. Transactional live replacement and host latency compensation remain outstanding.

Queue-empty observations must be monotonic within a device session. The controller rejects decreasing counters and retains a visible warning for a nonzero count through the audition completion. The backend emits a structured `audio.queue_empty` diagnostic when that count increases, outside the audio callback. A deliberate producer pause exercises this path on the real device.

Latest verification (2026-11-16): the final sustained native reruns failed the unchanged zero-empty-queue assertion, including a rerun without concurrent test compilation. Earlier successful 985088-frame evidence is not reproducible reliability. Exact finite VST-chain PCM still passes. Continuous playback is experimental pending producer-throughput work; do not treat this branch as release-ready.
