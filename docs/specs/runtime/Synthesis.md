# Osprey synthesis toolkit [SYNTH]

Status — implementation in progress. The initial package extracts Prism's pure pitch, waveform, modulation, sample and WAV functions. Typed graph validation/scheduling, reactive parameter snapshots and stateful scalar reference oscillators, ADSR and filters now have passing package journeys. The real-time renderer and advanced algorithms remain requirements, not completed features. Research sources and candidate experiments are in [the research guide](../../research/synthesis/README.md).

`@osprey/synth` is an independent synthesis library for instruments, effects, offline rendering and future plug-in hosting. It contains no UI, native widgets, canvas rendering, networking, session management or operating-system audio-device code. Prism and Light Orchestra are examples using the library. The orchestra's number of phones and instruments must not constrain library design.

## Components and routing [SYNTH-GRAPH]

A component has an instance identity, typed input/output ports, parameter descriptors, explicit persistent DSP state and observable state. A graph composes components by connecting ports, chaining stages, splitting outputs, mixing inputs and routing modulation. Component kinds include oscillators, noise, envelopes, LFOs, filters, amplifiers, panners, mixers, delays, nonlinearities, spectral processors and future external plug-ins. Components are reusable without the graph editor or any UI framework.

Ports distinguish audio, control and timestamped events. Audio ports declare channel layouts; parameter ports declare units, ranges and modulation semantics. Connections cannot silently reinterpret hertz as normalised values, mono as stereo, events as samples or control-rate values as audio-rate signals. Explicit adapters handle rate conversion, channel conversion, scaling and smoothing. Multiple inputs require a declared mixer/combiner rather than an implicit ordering rule.

Graph preparation validates missing/duplicate identities, endpoint existence, port direction, rate/channel compatibility, resource budgets and cycles. Feedback requires an explicit delay with a documented minimum; algebraic loops are rejected unless a dedicated bounded solver component owns them. Preparation determines scheduling, buffer reuse and latency compensation before rendering. Rejected edits preserve the running graph. Graph replacement occurs at a documented boundary with an explicit state-transfer/crossfade policy.

All canonical public data models are declared in typeDiagram and generated into Osprey/host representations. Stored patches include schema and component versions; unsupported versions return an explicit error. No silently substituted component or zero-valued parameter hides a failed load.

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

Research candidates include bandlimited/corrected oscillators, wavetable/additive/spectral synthesis, physical/modal models, granular engines, expressive phase shaping and differentiable/learned controls. Adoption requires the [research experiments](../../research/synthesis/README.md#research-to-delivery-experiments-synth-research-experiments), not a novelty claim. Training and expensive patch inference remain outside the required audio callback.

## Future VST3 hosting [SYNTH-HOST-VST]

VST3 **hosting** is an early extension, explicitly deferred from the initial library work. The purpose is to load existing VST3 instruments/effects into a synthesis graph. Producing Osprey VST3 plug-ins is a separate capability and is not substituted for hosting.

An external node declares audio buses, event buses, parameter identifiers, sample-accurate automation support, latency, tail length and saved state. Host adapters map these to the same graph ports and parameter Signals as built-in components. Plug-in metadata/parameter IDs remain stable across graph saves; missing binaries or changed layouts produce visible errors rather than loading a different plug-in silently.

Scanning, loading, activation and state restoration run outside the callback. The design must account for unsupported layouts, dynamic latency, bypass, crash/hang handling and untrusted plug-in code. Scan out of process; evaluate process isolation for playback with explicit latency/cost measurements. No claim that an arbitrary third-party plug-in respects Osprey's memory-safety or callback contract. Host integration includes actual DAW-style lifecycle and state/automation tests. Use the [official SDK](https://github.com/steinbergmedia/vst3sdk) as the compatibility source.

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
