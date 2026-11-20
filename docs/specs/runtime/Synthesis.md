# Osprey synthesis toolkit [SYNTH]

The [advanced synthesis proposal](#advanced-synthesis-proposal-synth-advanced) specifies neural, granular, physical and combined methods with their Prism interfaces. Its requirements are planned work.

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

Research candidates include bandlimited/corrected oscillators, wavetable/additive/spectral synthesis, physical/modal models, granular engines, expressive phase shaping and differentiable/learned controls. Adoption requires the [research experiments](../../research/synthesis/README.md#research-to-delivery-experiments-synth-research-experiments), not a novelty claim. Training and expensive patch inference remain outside the required audio callback.

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

The macOS Prism host now owns one bounded audio-device session alongside its VST sessions. Completed graph blocks are encoded outside the callback and enqueued as canonical 48 kHz mono/stereo float32 WAV transfers. Four preallocated AudioQueue buffers bound device memory; the callback only returns buffer ownership and updates lock-free counters. Input must contain 1–8192 frames with finite samples within -1..1. Invalid layouts, lengths, amplitudes, queue timeouts and mismatched acknowledgments are explicit failures.

Prism transfers 2048 graph frames at a time, while VST processing packets remain at most 256 frames. A typed pending device record retains the exact graph, sample clock and component states until the device acknowledges the transfer. Audio slices are reused across blocks. Audition drains every accepted frame; continuous Play schedules further blocks until Stop. Stop during opening first adopts the returned handle and then releases it, preventing a late-open resource leak. Host shutdown also releases owned sessions.

The reference producer allocates and runs outside the device callback. It is not an allocation-free low-latency renderer. A native loaded-binary journey streamed 985088 frames through JX10, two ADelay instances, LFO modulation and the real device with zero queue-empty observations, then verified worker disposal. Queue-empty callback counters are diagnostics, not acoustic measurements. Graph edits are currently captured on restart; transactional live replacement, host latency compensation and non-macOS device implementations remain outstanding.

Queue-empty observations must be monotonic within a device session. The controller rejects decreasing counters and retains a visible warning for a nonzero count through the audition completion. The backend emits a structured `audio.queue_empty` diagnostic when that count increases, outside the audio callback. A deliberate producer pause exercises this path on the real device.

Latest verification (2026-11-16): the final sustained native reruns failed the unchanged zero-empty-queue assertion, including a rerun without concurrent test compilation. Earlier successful 985088-frame evidence is not reproducible reliability. Exact finite VST-chain PCM still passes. Continuous playback is experimental pending producer-throughput work; do not treat this branch as release-ready.

## Advanced synthesis proposal [SYNTH-ADVANCED]

Status — proposed, 2026-11-17. This section defines future package behavior and Prism UI requirements. The [research collection](../../research/synthesis/README.md#neural-granular-and-physical-synthesis-synth-research-focused) supplies sources and product comparisons. The [delivery plan](../../plans/prism-synth-routing.md#advanced-synthesis-delivery-plan-prism-advanced-plan) identifies dependencies, experiments and acceptance evidence. Numerical defaults below are initial engineering choices to validate. Algorithm and UI implementation remain pending.

### Shared package contract [SYNTH-ADVANCED-CONTRACT]

Extend the existing typeDiagram graph, render and host schemas with versioned processor variants, processor-specific state, assets and faults. Reuse `Graphs`, `Parameters`, `Observations`, `Repatch`, `Renderer` and `GraphClips`. The current `KernelState` fields cannot represent grain pools, modal banks or neural histories faithfully. Introduce explicit state variants and reject a state whose processor kind, instance, asset generation or prepared format differs. Preserve older processor tags, parameter meanings and patch behavior during schema migration.

Preparation accepts sample rate, maximum block size, channel layout, voice/event limits and memory budget. It validates assets and allocates all required storage. Initial validation covers 44.1, 48 and 96 kHz and blocks from 1 to 2048 frames. Model-specific constraints may narrow that set and must be visible. Replace the fixed 48000 rate in `ClipKernels.rendered` with prepared format information when implementing these processors. Rate changes require a new preparation transaction. Existing playback continues until the replacement is accepted at a documented boundary.

Processing receives absolute sample position, an ordered event window, immutable parameter revisions and retained state. Events at the end of a block belong to the next half-open window. Same-frame order uses a stable sequence number. Zero-length blocks are rejected explicitly. Splitting a render into irregular blocks must preserve its event schedule and seeded random sequence. Processor observations include applied revision, audio position, resource use, latency, underruns and a typed lifecycle state.

The package owns DSP state and typed asset/model descriptions. Host effects own file selection, decoding, model execution, job scheduling, persistence and devices. Decode JSON into typed data at those boundaries. Canonical formulas, windows, resamplers, damping, mode evolution, force projection and neural output parameterization are Lean definitions under the package source root, with generated Osprey modules in `.osprey/math/`. Pin source equations and Mathlib statements where applicable. Extend the Lean reader for supported constructs required by these definitions.

All byte/frame/channel products use checked arithmetic before allocation or indexing. Validate finite parameters, decoded samples and model outputs. Assets have stable content hashes, frame count, sample rate, channel layout, license/provenance and generation. Readers retain an asset generation through their last use. Model loading accepts a validated tensor/operator format with bounded dimensions and operator allowlists. Executable deserialization and arbitrary model-side imports are rejected. Faults identify the node and operation, including invalid asset, incompatible model, capacity, deadline, nonfinite state and unavailable backend.

No device callback performs model loading, inference scheduling, allocation, disk access or blocking synchronization. Producer execution and callback execution have separate measured budgets. A failed producer returns a typed fault and publishes a diagnostic. The device adapter applies an explicitly reported fade/mute policy for missing audio and counts lost frames. The error remains visible through Stop and can be cleared after acknowledgement. Cancellation releases workers and assets exactly once, and stale replies cannot revive removed nodes.

### Granular processor [SYNTH-ADVANCED-GRANULAR]

Provide a sample instrument and a live-input processor sharing one grain kernel. The instrument takes timestamped notes and an immutable asset. The live processor takes audio into a bounded capture ring. Both output stereo audio with explicit mono conversion when needed. A grain retains asset generation, start frame, fractional read position, read increment, age, duration, envelope, pan and amplitude. Grain decisions use a per-node seed and absolute spawn index.

| Parameter | Initial contract |
| --- | --- |
| Position and region | Position 0–1 within a nonempty validated region, default 0.5. Display corresponding sample time. |
| Size | 2–500 ms, default 50 ms, at least two output frames after conversion. |
| Density | 0–200 grains/second, default 20. Zero stops births while active grains finish. |
| Pitch | −24 to +24 semitones, default 0, with sample-rate conversion included in the read increment. |
| Scan | −4 to +4 region lengths/second, default 0. Scan changes future birth positions independently of grain pitch. |
| Scatter | Position spread 0–1, pitch spread 0–12 semitones, defaults 0. Each draw is bounded and seeded. |
| Direction and window | Forward, reverse or seeded mixture. Hann is the default window. Other windows require their own level and boundary tests. |
| Pan and level | Spread 0–1, default 0.5, output gain 0–1, default 0.25. Stereo balance preserves source channels. |
| Capacity | Default 128 active grains per instance across at most 16 note voices. Prepared limits are displayed. |

Apply birth parameters when each grain starts. Gain and mix use sample-rate smoothing. Density changes preserve fractional scheduler phase. Tempo sync stores a rational musical duration and uses the transport tempo to schedule future births. Tempo changes retain active grains. Explicit manual grain events use the same capacity accounting as scheduled births.

Wrap, reflect and stop are explicit sample-boundary policies, with wrap the initial default. Every interpolation tap obeys the selected policy, including reverse traversal and one-frame assets. A bandlimited resampling path must be selected and measured before claiming production pitch shifting. Grain envelopes reach zero at their endpoints. Window-overlap normalization uses a bounded denominator and maximum gain, with that mathematical rule included in the Lean oracle.

Capacity exhaustion drops new births in deterministic order, increments a visible dropped-grain counter and returns a capacity observation. It preserves active tails. Note-off stops that voice's births and releases its existing grains. Voice stealing releases the oldest voice through a fixed 5 ms ramp. Maximum capacity is immutable during processing and changes only through preparation.

Freeze in live mode pins a bounded snapshot of the selected capture region. In sample mode it holds the scan position. Both continue grain evolution. Live grains retain their source data until completion through bounded retention storage prepared in advance. If storage cannot retain a new grain's source, reject that birth visibly. Capture boundaries and minimum read lag prevent reading uninitialized or overwritten data. Loading a replacement sample preserves old grain references until their tails end. Invalid loads retain the last accepted asset.

### Physical processors [SYNTH-ADVANCED-PHYSICAL]

Provide Exciter, Modal Body, String and Tube processor kinds. The first release of Modal Body includes bar, plate and membrane models with explicit boundary conditions, mode ratios and mode shapes. Each model records its source equations and assumptions. A user-defined modal table supplies finite frequency, decay and input/output weights. Unsupported geometry or excitation combinations return a preparation error.

Exciters support strike, pluck, bow and blow gestures with note velocity, pressure, contact position and external excitation audio. Strike/pluck pulses use bounded duration and energy. Bow friction and reed/airflow excitation require feedback with their physical body, implemented inside a bounded processor with explicit state. The combined processor owns any solver and its maximum iterations. Solver exhaustion is a typed failure. A generic graph cycle remains subject to the existing delay rule.

Modal Body retains displacement and velocity for each mode. Frequency and damping determine stable damped evolution, with input and pickup weights derived from contact and observation positions. Include a normalized energy representation and an exact damped-transition reference for each fixed parameter interval. Modes approaching Nyquist fade out using a documented transition band, initially 0.45–0.49 times sample rate. Static pole stability alone does not establish stability during time-varying parameter changes. Energy changes caused by material morphs are measured and constrained explicitly.

String uses a fractional-delay waveguide with loss, dispersion, excitation and pickup position. Tube uses an explicitly open/open or open/closed waveguide and declares its termination and radiation models. Pitch calibration includes filter and fractional-delay phase delay. Membrane/plate mode sets use documented physical boundary conditions. All models expose frequency and decay controls, with a capability descriptor constraining their valid ranges.

| Control | Initial contract |
| --- | --- |
| Pitch | MIDI note input or 20–4000 Hz numeric input, default 220 Hz, intersected with each model's prepared range. |
| Decay | T60 of 0.02–30 seconds, default 2 seconds, with frequency-dependent damping exposed separately. |
| Brightness/material | Normalized 0–1 macros, default 0.5, with the underlying physical mapping documented per model. |
| Contact/pickup | 0–1, defaults 0.25 and 0.5. Nodal positions may legitimately cancel particular modes. |
| Modes/voices | Initial maximum 64 modes per voice and 16 voices. Resource changes require preparation. |
| Coupling | 0–1 macro, default 0, mapped to a validated passive connection between bodies. |

Coupled bodies exchange energy through a defined bidirectional physical connection with a common clock and joint state. Show the coupling topology and stability limits in the descriptor. Prove or numerically bound the discrete energy balance under the supported parameter range, including damping, excitation and automation. An output limiter supplies a final output safeguard, while stability acceptance examines the internal state before limiting.

### Neural processors and generated assets [SYNTH-ADVANCED-NEURAL]

Expose four capabilities with separate lifecycle contracts. A RAVE-style timbre-transfer node maps audio through a model-specific latent representation. A DDSP voice maps analyzed pitch/loudness or MIDI controls to harmonic and noise synthesis. A neural codec encoder/decoder transforms audio and a typed encoded asset. A text-to-sound job produces an immutable sample asset for audition and insertion. Completion requires a real supported model path for each capability.

A model manifest includes content hash, format/runtime versions, model family, input/output channels, sample rate, input normalization, supported frame/hop sizes, lookahead, receptive history, latent/codebook dimensions, parameter names/ranges, state-reset behavior, maximum output frames, memory budget, license and training-data provenance where available. Missing provenance is shown explicitly. Reject incompatible model/codec pairs. Codebook indices and latent tensors are distinct typed values, and interpolation is allowed only where the model defines it.

Preparation loads and warms the model, validates outputs, measures target-device execution and establishes declared latency. Chunk adapters retain context and overlap state across graph blocks. Wet/dry mixing delays the dry path by the declared algorithmic latency. Model switches prepare independently and commit at a block boundary using a bounded crossfade. Unavailable runtimes and unsupported models retain an actionable fault and disable Play for that node.

DDSP Audio mode exposes pitch confidence and voiced/unvoiced status. Low confidence uses a documented voiced-gain release while retaining the model's noise path. MIDI mode bypasses pitch analysis and uses note expression. Pitch shift, harmonic/noise balance and input gain have model-declared ranges. RAVE latent axes use manifest labels or numbered dimensions. Their names must not imply a verified physical meaning without evidence. Preserving pitch during arbitrary RAVE transformations is an evaluation target.

Codec import/export records codebook version, quantizer count, sample rate, frame length and channel layout. Discrete tokens receive bounds checks before lookup. Changing bandwidth selects a supported configuration and triggers preparation. Decoding failures preserve the previous accepted asset and report the failed job. Playback uses the same validated sample asset lifecycle as recorded and generated audio.

Text generation accepts prompt, seed, duration, model revision and model-specific guidance parameters. Initial duration is 1–30 seconds, default 5, capped by backend capability. States are Idle, Queued, Running, Cancelling, Ready and Failed with stable job identity. Cancellation and timeouts make late results ineligible for publication. Ready enables Preview and Insert sample as separate actions. Save exact rendered audio and generation metadata, since seeds can reproduce differently across backends. External services, if supported later, require an explicit configured backend and visible upload scope.

### Combined grain and body method [SYNTH-ADVANCED-HYBRID]

Working description — learned grain contacts on a resonant body. This is a proposed synthesis experiment. Existing neural granular synthesis, differentiable modal synthesis, integrated granular/modal haptics and neural event-driven instruments constrain claims of originality. The specific hypothesis is that body-state-dependent grain contacts provide useful control over attack texture and resonant memory within one evolving instrument. [Prior-art comparison](../../research/synthesis/README.md#combining-the-methods-and-assessing-originality-synth-research-hybrid)

Represent each grain as a finite contact gesture with time, duration, location, latent descriptor, force envelope and energy allowance. A grain can select a recorded microtexture or decode a learned microtexture into prepared bounded storage. Its contact location projects force into the body's spatial modes. All active grains act on the same displacement/velocity state, and the pickup reads that state. The controller receives the previous control tick's modal energy summaries and contact history, so ringing changes subsequent grain selection and timing.

The candidate renderer uses this explicit sequence. At a fixed 128-frame control tick, summarize the body state and evaluate a bounded controller using those summaries, the player's gesture, a material descriptor and the seeded event index. Validate its proposed event times, contact positions, durations and energy allocations. At each sample, accumulate active contact forces through modal input weights, advance the damped body state and read the pickup. Retain the absolute control clock across arbitrary block splits. A one-tick feedback delay resolves the controller dependency.

Use nonnegative normalized allocation weights for a finite per-tick excitation energy allowance. A proposed force's discrete work includes its interaction with current velocity. Select its amplitude through a bounded calculation that satisfies the allowed energy increment, and debit that increment from the remaining budget. Positive energy introduced by changing modal frequency, mass or basis is debited too. If no valid update fits the budget, retain the previous body parameters and report a constrained update. Source PCM amplitude alone is insufficient to establish a physical work bound. The exact update and energy inequality must be authored and checked in Lean before implementing the optimized renderer.

Training occurs offline using licensed contact recordings and synthetic bodies. Learn grain-selection/latent trajectories and contact parameters through the differentiable reference renderer. Keep train/validation/test splits disjoint by both source recording and body geometry. Losses cover multiresolution spectra, onset timing, decay envelopes, pitch and physical constraint violations. A fixed analytic controller remains an explicit selectable mode for comparisons. Runtime failure of a learned controller is reported and stops its event production.

Three integration experiments share this representation. First, learned contact scheduling uses body energy to choose how and where the next grain excites it. Second, an offline inverse model maps a reference sound to both a grain dictionary and physical material parameters that remain editable. Third, a text-conditioned generator proposes a bounded contact/material trajectory, rendered and inspected as an asset before performance. These experiments share state or parameter inference. Each has its own model manifest, latency contract and failure state.

Initial hybrid limits are 64 active contacts, 64 modes and one body voice, with stereo pickups. Controls are Texture, Contact rate, Contact size, Material, Body memory, Interaction, Energy and Seed. Interaction continuously scales the body-state features supplied to the controller. Zero supplies the defined open-loop baseline. Body memory maps to T60, and Energy maps to the per-tick work budget. Every macro exposes its actual units or underlying mapping in the inspector.

Required comparisons include ordinary grain output through a resonator, open-loop grain contacts, analytic state-dependent contacts, learned contacts without energy features and the full proposed method. Match source assets, seeds, modal count, excitation budget, latency and output loudness. Change only the initial body state and show a resulting change in subsequent event/contact traces with Interaction enabled. At Interaction zero, those event traces must remain identical. This demonstrates the intended state coupling independently of a subjective sound claim.

### Prism module interfaces [PRISM-ADVANCED-UI]

Extend the current single graph studio, palette, selected-module inspector, keyboard and transport. Package descriptors provide parameter units, limits and availability. Prism's `editor.td` owns selection, job identity and view state, while typed Signals supply applied DSP observations. Stable node IDs distinguish duplicate modules. Native accessible controls and keyboard routing actions accompany canvas interactions. UI refresh reads bounded snapshots at a maximum of 30 Hz and never schedules synthesis from an animation frame.

| Module | Node card | Selected inspector and interaction |
| --- | --- | --- |
| Granular | Source name, active/capacity count, Size, Density, Freeze, typed input/output ports | Waveform with region handles, scan cursor and bounded grain markers. Controls expose pitch, scan, direction, spread, seed and boundary mode. Load sample, Capture, Freeze and Clear capture have distinct effects. Keyboard adjusts region endpoints numerically. |
| Physical | Model, pitch, decay, excitation mode and energy meter | Exciter and Body sections, contact/pickup positions and mode spectrum. Strike button and keyboard send timestamped gestures. Material morph displays the applied constrained value. Coupling editor selects bodies and shows the physical connection. |
| Neural transfer/DDSP | Model identity, status, input mode, wet mix and measured latency | Model picker shows channels, rate, license, provenance and availability. Latent controls or harmonic/noise controls follow capabilities. Confidence, input/output meters and deadline errors remain visible. Model loading supports cancellation. |
| Neural codec | Codec identity, bandwidth and asset status | Encode/Decode controls, validated metadata, progress, Cancel and Preview. Token edits use bounded typed operations supplied by that codec. |
| Text-to-sound | Job status and resulting asset name | Prompt, duration, seed, model, Generate, Cancel, Preview and Insert sample. Regeneration keeps the previous accepted result available. Late/cancelled outputs cannot insert themselves. |
| Combined method | Texture, Material, Interaction and body energy | Contact map, grain timeline and mode-energy view share the same selected body. Display energy allowance, constrained updates, controller mode and exact seed. Offer the analytic and learned modes with declared capability. |

Each module has Empty, Preparing, Ready, Running and Failed presentations where applicable. Model/sample errors show the cause and a relevant retry or replacement action. The palette can describe unavailable capabilities, but an unavailable processor cannot start playback. Persist versioned parameters, model/asset hashes, seed and intended controller mode. Loading missing resources produces an unresolved node that can be relinked. Undo/redo affects accepted edits and asset bindings, with pending job replies invalidated by revision changes.

Controls show configured and applied values when they differ. Every graphic has an accessible label and a numeric/text alternative, including contact location and modal energy. Focus order proceeds from source/model selection through parameters, routing and actions. Dragging, resizing, zooming and keyboard navigation must preserve usable routing ports and unclipped controls. Windows uses the existing WinUI host and per-app sandbox contract. Required file/model capabilities are documented for each supported host.

### Acceptance contract [SYNTH-ADVANCED-VERIFY]

Extend existing package and Prism journeys before implementation. Record fixtures, model hashes, toolchain versions, tolerances and hardware with every result. Run DSP checks at the declared sample rates using blocks of 1, 17, 64, 127, 128, 257 and 2048 frames, plus irregular partitions. Compare final states as well as PCM. Fixed-backend deterministic paths require identical event traces and relative/absolute PCM tolerance of 1e-6 against their reference. Neural cross-backend tolerances must be measured and recorded per model before acceptance.

Granular acceptance covers empty/truncated assets, overflow dimensions, nonfinite samples, one-frame sources, reverse boundaries, freeze/replace during tails, 0/max density, seeded replay, live-ring wrap, overload counters and cancellation. Measure pitch error below 2 cents on calibrated sinusoidal fixtures and alias energy at the specified transposition extremes against a high-quality resampling reference. Establish the quality threshold from that comparison before promoting a resampler.

Physical acceptance covers impulse spectra, nodal cancellations, pitch/decay sweeps, bowed/blown solver limits, rates, extreme geometry and automation. For isolated linear modes, target frequency error below 2 cents and fitted T60 error below 5 percent. Verify the declared energy inequality over unforced decays and coupled-body transitions, including hidden internal state. Use physical and numerical reference cases with known assumptions.

Neural acceptance uses genuine licensed model fixtures for all four capabilities, covering channel/hop adaptation, pitch-confidence failure, manifest and tensor corruption, invalid codebook indices, resampling, model replacement, deadlines, worker loss, cancellation and stale results. Include held-out monophonic, polyphonic, transient and noise inputs. Report reconstruction/spectral metrics and level-matched listening results by class. Behavioral mocks establish lifecycle tests only. Model availability and real inference evidence remain separate acceptance requirements.

Hybrid acceptance records contact traces, controller inputs, energy debits and mode trajectories for every ablation in [SYNTH-ADVANCED-HYBRID]. Compare held-out bodies and textures with both objective onset/decay metrics and randomized, loudness-matched listening. Preserve negative results. Adopt the method only if it adds repeatable controllability or perceptual value within its declared resource budget. Any claim of research novelty requires a broader prior-art review.

Prism acceptance drives insertion, two same-kind instances, input selection, editing, modulation, keyboard playing, drag/keyboard routing, freeze, model switch, cancellation, undo/redo, save/reload and missing-resource recovery through normal entry points. Headless and windowed journeys compare committed screenshot goldens, assert accessible names/focus order, and verify actual rendered audio or asset changes. Continuous playback requires a 10-minute target-device run with zero queue starvation at the declared maximum workload, measured latency and callback allocation/lock instrumentation. Run on Windows and every other advertised host. Existing queue-starvation failures remain an explicit dependency.
