# Advanced synthesis: grains, physical, neural

Part of the [synthesis toolkit](Synthesis.md).
## Grain envelopes [SYNTH-ADVANCED-GRAINS-WINDOWS]

`GrainControls.window` is a generated `GrainWindow`: `HannWindow` or `AsrWindow(attack, release)`. ASR fractions are finite, positive and sum to at most one; they divide the `duration - 1` sample interval into linear attack, unity sustain and linear release. Both shapes explicitly return zero at the first and last sample. Discrete short grains may miss the continuous peak. Each voice retains its shape and fractions for its lifetime, so graph control changes affect subsequent triggers only. Graph selectors accept exactly 0 (Hann) or 1 (ASR); invalid settings return errors before block rendering. Strict compilation is evidence of integration only; rendered envelope and boundary verification remains pending.

## Asynchronous grain timing [SYNTH-ADVANCED-GRAINS-TIMING]

`GrainScheduler.prepare(position, timing, seed)` accepts generated `PeriodicTiming(period, jitter)` or `ExponentialTiming(mean, maximum)` policies. Exponential mean is finite in [1, 1048576] output samples; maximum is an integer in [1, 1048576]. Each onset advances the retained Park–Miller seed once and samples `ceil(-mean * ln(1 - seed / 2147483647))`, clamped to [1, maximum]. This deliberately bounded, discretised renewal process has a changed mean and probability mass at the cap; it is not an exact Poisson process. The first onset is immediate, no two scheduler onsets share a sample, and each block emits at most 2048 onsets. Switching policy preserves the pending onset and seed; only subsequent gaps use the new policy. Invalid policies and exhausted sample clocks return errors without publishing partial state. Grain Cloud uses the same scheduler and publishes its applied policy through `GrainCloudObservation`. Compilation and source review do not establish statistical or audio correctness; verification remains pending.

## Grain overload and observations [SYNTH-ADVANCED-GRAINS-OVERLOAD]

The grain pool uses reject-new admission. `Grains.trigger` returns `Result<GrainAdmission, ...>` with the next pool and an explicit `accepted` flag. Valid requests at capacity return `accepted=false` without replacing, truncating or restarting any existing grain. Scheduled rendering consumes rejected events, counts them and advances normally; invalid controls/state or exhausted counters still fail the whole immutable operation. Valid requests increment revision; accepted requests also increment the identity counter, so rejected count is their difference. The pool invariant requires revision to be at least the identity counter.

`GrainObservation` exposes active count, capacity, cumulative requested/accepted/rejected counts, zero stolen count, and lifetime-average requested/accepted events per rendered frame. Before the first rendered frame density is zero, while counts remain available. Consumers multiply by the host sample rate for grains/second; these are observed averages, not the configured scheduler rate or an instantaneous estimate. The existing Signal effect publishes this coherent snapshot outside audio rendering. Source review and compilation do not prove audible overload recovery; rendered evidence remains pending.

## Independent grain variation [SYNTH-ADVANCED-GRAINS-DISTRIBUTIONS]

`GrainDistribution` holds ordered minimum/maximum playback controls with identical window settings. Both ends must pass source/control validation. `GrainSeeds` retains one checked seed per source position, playback rate, duration, amplitude and pan. `GrainDistributions.events` accepts at most 2048 ordered nonnegative sample positions, advances each stream once per event, and returns the complete event batch plus next seeds. Empty batches do not advance randomness; fixed ranges still do. Duration is inclusive discrete uniform; other values use bounded linear uniform interpolation. Playback-rate variation is not a semitone-uniform pitch distribution. Equal initial seeds intentionally correlate streams, and finite generator bins have quantisation bias.

`GrainClouds.variedBlock` composes onset scheduling, variation and existing grain rendering as one immutable operation. The returned `GrainCloudState` owns the pool, onset clock and five parameter seeds; caller publication is atomic at this boundary; any error returns neither partial result. Rejected onsets consume their draws and are counted by the player. Existing grains retain their previously sampled controls. Library and graph integration have passed strict compilation; native editor and rendered statistical/replay evidence remain pending.

Graph variation [SYNTH-ADVANCED-GRAINS-DISTRIBUTIONS-GRAPH]: `positionSpread`, `rateSpread`, `durationSpread`, `gainSpread` and `panSpread` default to zero and define symmetric widths clipped to the graph parameter domains. Duration width is an exact integer. `variationSeed` initialises five offset seed states only when preparing a new cloud; subsequent blocks retain those states. The inspector consumes these ordinary descriptors without a second control schema. Changing widths does not reseed or alter active grains. Source-position spread is limited to [0, sourceSize−1], matching the graph center control. Invalid bounds/seed configuration returns an error. Native interaction and audio remain unverified for this integration.

## Advanced synthesis proposal [SYNTH-ADVANCED]

The 2026-11-17 proposal defines package behavior and Prism UI requirements. Numerical defaults below are initial engineering choices to validate. The [source collection](#sources-neural-granular-and-physical-synthesis-synth-research-focused) supplies the sources. The [delivery plan](../../plans/synth/advanced-synthesis.md#advanced-synthesis-delivery-plan-prism-advanced-plan) records dependencies and acceptance evidence. For current implementation and verification status, see the [active synthesis checklist](../../plans/synth/advanced-synthesis.md).

### Shared package contract [SYNTH-ADVANCED-CONTRACT]

Extend the existing typeDiagram graph, render and host schemas with versioned processor variants, processor-specific state, assets and faults. Reuse `Graphs`, `Parameters`, `Observations`, `Repatch`, `Renderer` and `GraphClips`. Reject state whose processor kind, instance, asset generation or prepared format differs. Preserve older processor tags, parameter meanings and patch behavior during migration.

Preparation accepts sample rate, maximum block size, channel layout, voice/event limits and memory budget. It validates assets and allocates all required storage. Initial validation covers 44.1, 48 and 96 kHz and blocks from 1 to 2048 frames. Model-specific constraints may narrow that set and must be visible. Use the prepared format throughout scalar and graph rendering, including `ClipKernels.rendered`. Rate changes require a new preparation transaction. Existing playback continues until the replacement is accepted at a documented boundary.

Every module refusal is a typed `Fault` (`dsp.td`): `InvalidConfiguration` (a setting, state or request outside its declared range), `ExhaustedCapacity` (a buffer, clock or rate limit that cannot hold the request), `MissingResource` (an absent node, parameter, state, input, or a missing or malformed sample file), `IncompatibleLayout` (ports, channels, outputs or retained state that do not fit) or `NumericalFailure` (a nonfinite or out-of-range value). Each carries the sentence a host shows, read with `Faults.detail`. Graph validation keeps its own `GraphFault` union. Hosts convert a fault to text only at their display boundary.

The reference lifecycle is `Lifecycles` ([SYNTH-ADVANCED-CORE-MODELS], `Stage`, `Transition` and `Instance` in `render.td`). `prepared` validates the rate (8,000 to 384,000 Hz) and largest block (1 to 8,192 frames) and returns a Ready instance with silent state at frame 0. Activate moves Ready to Running. ReleaseTail moves Running to Releasing with the graph's tail: every node's declared tail and latency summed, rescaled from 48 kHz to the prepared rate and rounded up. A releasing block renders at most the remaining tail and returns to Ready when it is spent. Reset returns any live stage to Ready with silent state. Prepare re-prepares a Ready instance at a new rate or block, silent, and is refused while Running or Releasing. Dispose ends any live stage and drops its state. Every other transition, and a block of 0 frames or over the prepared size, is an explicit error that leaves the caller's instance unchanged.

Processing receives absolute sample position, an ordered event window, immutable parameter revisions and retained state. Events at the end of a block belong to the next half-open window. Same-frame order uses a stable sequence number. Reject zero-length blocks explicitly. Irregular block splits must preserve the event schedule and seeded random sequence. Observations include applied revision, audio position, resource use, latency, underruns and a typed lifecycle state.

The package owns DSP state and typed asset/model descriptions. Host effects own file selection, decoding, model execution, job scheduling, persistence and devices. Decode JSON into typed data at those boundaries. Define canonical windows, resamplers, damping, mode evolution, force projection and neural output parameterization as native Osprey functions under the synth package source root. Pin the source equations and applicable Mathlib statements, and derive the definitions used by the Lean proofs from the production Osprey syntax trees.

Check byte/frame/channel products before allocation or indexing. Validate finite parameters, decoded samples and model outputs. Assets carry a stable content hash, frame count, sample rate, channel layout, license/provenance and generation. Readers retain a generation through their last use. Accept validated tensor/operator formats with bounded dimensions and operator allowlists. Reject executable deserialization and arbitrary model-side imports. Faults identify the node and operation, including invalid asset, incompatible model, capacity, deadline, nonfinite state and unavailable backend.

Device callbacks exclude model loading, inference scheduling, allocation, disk access and blocking synchronization. Measure producer and callback budgets separately. A failed producer returns a typed fault and publishes a diagnostic. The device reports its fade/mute policy and counts lost frames. Keep errors visible through Stop until acknowledgement. Cancellation releases workers and assets exactly once. Reject stale replies for removed nodes.

### Granular processor [SYNTH-ADVANCED-GRANULAR]

Use one grain kernel for a timestamped-note sample instrument and a live-input processor with a bounded capture ring. Both output stereo with explicit mono conversion when needed. Each grain retains asset generation, start frame, fractional read position, read increment, age, duration, envelope, pan and amplitude. Grain decisions use a per-node seed and absolute spawn index. Existing window, timing and overload behavior is specified above.

| Parameter | Proposed musical controls and defaults |
| --- | --- |
| Position and region | 0–1 within a nonempty validated region, default 0.5. Display corresponding sample time. |
| Size | 2–500 ms, default 50 ms, at least two output frames. |
| Density | 0–200 grains/second, default 20. Zero stops births while active grains finish. |
| Pitch | −24 to +24 semitones, default 0. Include sample-rate conversion in the read increment. |
| Scan | −4 to +4 region lengths/second, default 0. Scan changes future birth positions independently of pitch. |
| Scatter | Position spread 0–1, pitch spread 0–12 semitones, defaults 0. Bound and seed each draw. |
| Direction and window | Forward, reverse or seeded mixture. Default Hann. Verify level and boundaries for each additional window. |
| Pan and level | Spread 0–1, default 0.5. Output gain 0–1, default 0.25. Stereo balance preserves source channels. |
| Capacity | Default 128 active grains per instance across at most 16 note voices. Display prepared limits. |

Apply birth parameters at grain start. Smooth gain and mix at sample rate. Density changes preserve fractional scheduler phase. Tempo sync stores a rational musical duration and uses transport tempo for future births. Active grains survive tempo changes. Manual events share scheduled-event capacity accounting.

A grain reads its source inside a region `[first, limit)` under one of two policies ([SYNTH-ADVANCED-GRAINS-BOUNDARY]); wrap is the default. **Loop** (`LoopSource`, wrap): the cursor advances by the rate and wraps into the region, forward or reverse (`GrainMath.regionCursor`), and the interpolation tap after the last frame is the region's first frame. **One-shot** (`OneShotSource`, stop): the cursor advances without wrapping, the tap after the last frame repeats the last frame, and a grain whose cursor leaves the region ends. **Reverse** is a negative rate under either policy. **Interpolation** is linear between the two taps (`Interpolation.linear`), not bandlimited. Reflect is not implemented. Every interpolation tap obeys the policy, including reverse traversal and one-frame regions; `regionJourney` covers loop and one-shot, forward, reverse and held. Select and measure a bandlimited resampler before claiming production pitch shifting. Grain envelopes reach zero at both endpoints. Define the bounded denominator and maximum gain for window-overlap normalization in the Lean oracle.

Drop new births in deterministic order at capacity, increment a visible counter and publish a capacity observation. Preserve active tails. Note-off stops births for that voice and releases existing grains. Voice stealing releases the oldest voice through a fixed 5 ms ramp. Capacity changes require preparation.

Live Freeze pins a bounded snapshot of the selected region. Sample Freeze holds the scan position. Both continue grain evolution. Prepare bounded storage for live grains to retain their source until completion. Reject a birth visibly if retention fails. Enforce capture boundaries and minimum read lag against uninitialized or overwritten data. Sample replacement preserves old references through their tails. Invalid loads retain the last accepted asset.

### Physical processors [SYNTH-ADVANCED-PHYSICAL]

Provide Exciter, Modal Body, String and Tube processor kinds. The proposed Modal Body set includes bar, plate and membrane models with explicit boundary conditions, mode ratios and shapes. Record source equations and assumptions. A user-defined modal table supplies finite frequency, decay and input/output weights. Reject unsupported geometry or excitation at preparation.

Support strike, pluck, bow and blow gestures with note velocity, pressure, contact position and external audio. Bound strike/pluck pulse duration and energy. Bow friction and reed/airflow excitation feed back through the body. The combined processor owns the bounded solver and its maximum iterations. Report exhaustion as a typed failure. Generic graph cycles retain the existing delay rule.

Modal Body retains displacement and velocity per mode. Contact and observation positions determine input and pickup weights. Provide normalized energy and an exact damped-transition reference for fixed parameter intervals. Fade modes near Nyquist, initially across 0.45–0.49 times sample rate. Measure and constrain energy changes during material morphs as well as checking static poles.

String uses a fractional-delay waveguide with loss, dispersion, excitation and pickup position. Tube declares open/open or open/closed terminations and its radiation model. Pitch calibration includes filter and fractional-delay phase. Record membrane/plate boundary conditions and expose prepared frequency and decay ranges in a capability descriptor.

| Control | Initial contract |
| --- | --- |
| Pitch | MIDI or 20–4000 Hz, default 220 Hz, intersected with the model's prepared range. |
| Decay | T60 of 0.02–30 seconds, default 2 seconds. Expose frequency-dependent damping separately. |
| Brightness/material | 0–1, default 0.5. Document the physical mapping per model. |
| Contact/pickup | 0–1, defaults 0.25 and 0.5. Nodal positions can cancel modes. |
| Modes/voices | Initial maximum 64 modes per voice and 16 voices. Resource changes require preparation. |
| Coupling | 0–1, default 0, mapped to a validated passive connection. |

Coupled bodies exchange energy bidirectionally with a common clock and joint state. Expose topology and stability limits. Prove or numerically bound the discrete energy balance across damping, excitation and automation. Examine internal state before the final output limiter.

### Neural processors and generated assets [SYNTH-ADVANCED-NEURAL]

A neural waveform generator is optional and later: no instrument, patch or test depends on one, and none ships until it declares its own latency, memory budget and degradation behaviour (what plays when the model is missing, late or fails) as a separate contract.

Define separate lifecycles for RAVE-style audio-to-latent timbre transfer, DDSP harmonic/noise voices driven by pitch/loudness or MIDI, neural codec encoding/decoding of typed assets, and text-to-sound jobs producing immutable samples. Each capability needs a real supported model path.

A model manifest carries content hash, format/runtime versions, family, input/output channels, sample rate, input normalization, frame/hop sizes, lookahead, receptive history, latent/codebook dimensions, parameter names/ranges, reset behavior, maximum output frames, memory budget, license and available training-data provenance. Show missing provenance. Reject incompatible model/codec pairs. Distinguish discrete codebook indices from latent tensors and allow interpolation only where the model defines it.

Preparation loads and warms the model, validates outputs, measures target-device execution and establishes latency. Chunk adapters retain context and overlap across graph blocks. Delay the dry path by algorithmic latency for wet/dry mixing. Prepare model replacements independently and commit with a bounded block-boundary crossfade. Unavailable runtimes and unsupported models expose an actionable fault and disable Play for that node.

DDSP Audio mode exposes pitch confidence and voiced/unvoiced status. At low confidence apply the documented voiced-gain release while preserving the noise path. MIDI mode uses note expression and bypasses analysis. Manifest ranges govern pitch shift, harmonic/noise balance and input gain. RAVE axes use manifest labels or numbered dimensions. Physical meanings and pitch preservation need evaluation.

Codec import/export retains codebook version, quantizer count, sample rate, frame length and channel layout. Bounds-check tokens before lookup. Bandwidth changes select a supported configuration and trigger preparation. Failed decoding reports the job and preserves the previous asset. Decoded, recorded and generated samples share the validated playback lifecycle.

Text generation takes prompt, seed, duration, model revision and model-specific guidance. Initial duration is 1–30 seconds, default 5, capped by backend capability. Track Idle, Queued, Running, Cancelling, Ready and Failed by stable job ID. Cancellation and timeout invalidate late results. Ready enables separate Preview and Insert sample actions. Save exact audio and generation metadata because backends can differ for a given seed. External services require explicit configuration and visible upload scope.

### Combined grain and body method [SYNTH-ADVANCED-HYBRID]

The hypothesis is that body-state-dependent grain contacts give the player control over attack texture and resonant memory within one evolving instrument. Neural granular synthesis, differentiable modal synthesis, integrated granular/modal haptics and neural event-driven instruments constrain originality claims. [Prior-art comparison](#combining-the-methods-and-assessing-originality-synth-research-hybrid)

Represent each grain as a finite contact gesture with time, duration, location, latent descriptor, force envelope and energy allowance. Select a recorded microtexture or decode a learned one into prepared bounded storage. Contact location projects force into spatial modes. Every grain acts on the same displacement/velocity state, which the pickup reads. Previous-tick modal energy summaries and contact history influence subsequent selection and timing.

At a fixed 128-frame control tick, summarize the body state. Evaluate a bounded controller using the summaries, player's gesture, material descriptor and seeded event index. Validate event times, positions, durations and energy allocations. At each sample, accumulate contact forces through modal input weights, advance the damped state and read the pickup. Retain the absolute control clock across block splits. One tick of feedback delay resolves the dependency.

Allocate the finite per-tick excitation budget using nonnegative normalized weights. Compute discrete work against current velocity. Choose force amplitude through a bounded calculation and debit the allowed energy increment, including positive work from changes in modal frequency, mass or basis. If no valid update fits, retain prior body parameters and report a constrained update. Check the energy inequality in Lean against the native Osprey update before optimizing.

Train offline on licensed contact recordings and synthetic bodies. Learn grain-selection/latent trajectories and contact parameters through the differentiable reference renderer. Keep train/validation/test splits disjoint by recording and body geometry. Use multiresolution spectral, onset, decay, pitch and constraint-violation losses. Retain a selectable fixed analytic baseline. Report learned-controller failure and stop its event production.

Evaluate three experiments independently: energy-conditioned contact scheduling, offline inverse estimation of an editable grain dictionary and physical material from reference sound, and text-conditioned generation of a bounded contact/material trajectory for asset inspection before performance. Each has its own manifest, latency contract and failure state.

Initial limits are 64 active contacts, 64 modes, one body voice and stereo pickups. Expose Texture, Contact rate, Contact size, Material, Body memory, Interaction, Energy and Seed, with each mapping or unit visible. Interaction scales body-state features continuously to the open-loop baseline at zero. Body memory maps to T60 and Energy to the per-tick work budget.

Compare ordinary grains routed through a resonator, open-loop contacts, analytic state-dependent contacts, learned contacts without energy features and the full method. Match assets, seeds, modes, excitation budget, latency and loudness. Changing only the initial body state must change subsequent contact traces with Interaction enabled and leave them identical at zero.

### Prism module interfaces [PRISM-ADVANCED-UI]

Use the single graph studio, palette, selected inspector, keyboard and transport. Package descriptors give units, limits and availability. Prism's `editor.td` owns selection, job identity and view state. Typed Signals supply applied observations. Stable IDs distinguish duplicate nodes. Provide accessible native controls and keyboard routing. Read bounded snapshots at most 30 Hz. Animation frames never schedule synthesis.

| Module | Node card | Inspector and interaction |
| --- | --- | --- |
| Granular | Source, active/capacity, Size, Density, Freeze, typed ports | Waveform, numeric region endpoints and handles, scan cursor, bounded grain markers, pitch, scan, direction, spread, seed and boundary mode. Distinguish Load sample, Capture, Freeze and Clear capture. |
| Physical | Model, pitch, decay, excitation and energy | Exciter and Body sections, contact/pickup positions and mode spectrum. Strike and keyboard send timestamped gestures. Show applied constrained morph values and selected coupling topology. |
| Neural transfer/DDSP | Model, status, input mode, wet mix and measured latency | Show channels, rate, license, provenance, availability, latent or harmonic/noise controls, confidence, meters and deadline errors. Support load cancellation. |
| Neural codec | Codec, bandwidth and asset status | Encode, Decode, metadata, progress, Cancel and Preview. Bound typed token edits by codec. |
| Text-to-sound | Job status and asset name | Prompt, duration, seed, model, Generate, Cancel, Preview and Insert sample. Retain the previous result during regeneration. Prevent late insertion. |
| Combined method | Texture, Material, Interaction and energy | Shared-body contact map, grain timeline and mode-energy view. Show allowance, constrained updates, seed and controller mode. Offer analytic and learned modes by capability. |

Show Empty, Preparing, Ready, Running and Failed where applicable. Show model/sample errors with retry or replacement. An unavailable processor cannot start playback. Persist versioned parameters, asset/model hashes, seed and intended controller mode. Load missing resources as relinkable unresolved nodes. Undo/redo affects accepted edits and bindings and invalidates pending replies by revision.

Show configured and applied values when they differ. Give every graphic an accessible label and numeric/text alternative, including contact location and modal energy. Order focus from source/model through parameters, routing and actions. Preserve usable ports and unclipped controls during drag, resize, zoom and keyboard navigation. Windows uses WinUI and the per-app sandbox contract. Document file/model capabilities per host.

### Acceptance contract [SYNTH-ADVANCED-VERIFY]

Extend existing package and Prism journeys. Record fixtures, model hashes, toolchain versions, tolerances and hardware. At declared rates exercise blocks of 1, 17, 64, 127, 128, 257 and 2048 frames and irregular partitions. Compare final states and PCM. Fixed-backend deterministic paths require identical event traces and relative/absolute PCM tolerance of 1e-6 against reference. Measure neural cross-backend tolerances per model.

Granular acceptance covers empty/truncated assets, overflow dimensions, nonfinite samples, one-frame sources, reverse boundaries, freeze/replace during tails, zero/max density, seeded replay, ring wrap, overload counters and cancellation. Require pitch error below 2 cents on calibrated sinusoids. Compare alias energy at transposition extremes to a high-quality reference and establish the quality threshold before promoting a resampler.

Physical acceptance covers impulse spectra, nodal cancellations, pitch/decay sweeps, solver limits, rates, extreme geometry and automation. Target isolated linear-mode frequency error below 2 cents and fitted T60 error below 5 percent. Verify the energy inequality during unforced decays and coupled transitions, including hidden state, against physical and numerical references with known assumptions.

Use genuine licensed fixtures for all four neural capabilities. Cover channel/hop adaptation, pitch confidence, manifest/tensor corruption, codebook indices, resampling, replacement, deadlines, worker loss, cancellation and stale results. Report reconstruction/spectral metrics and level-matched listening by held-out monophonic, polyphonic, transient and noise class. Mocks establish lifecycle evidence only.

Record contact traces, controller inputs, energy debits and mode trajectories for each hybrid ablation. Use held-out bodies and textures, objective onset/decay metrics and randomized loudness-matched listening. Preserve negative results. Adoption requires repeatable controllability or perceptual value within budget. Novelty claims require broader prior-art review.

Drive insertion, two same-kind instances, input selection, editing, modulation, keyboard playing, pointer/keyboard routing, freeze, model switch, cancellation, undo/redo, save/reload and missing-resource recovery through Prism. Headless and windowed journeys compare committed goldens, assert accessible names/focus order and verify actual audio or asset changes. Require a 10-minute maximum-workload device run with zero starvation, measured latency and callback allocation/lock instrumentation on Windows and every other advertised host. Retain existing queue-starvation failures as a delivery dependency.

## Sources: neural, granular and physical synthesis [SYNTH-RESEARCH-FOCUSED]

Collected 2026-11-17. The following annotations describe primary publications and manufacturer documentation. Proposed applications are design decisions for Osprey. Product descriptions establish documented behavior, with listening, source-level reproduction and target-device benchmarks remaining future work. Source versions are identified where available.

### Neural models and deployed tools [SYNTH-RESEARCH-NEURAL]

| Source | Technical contribution and consequence for the design |
| --- | --- |
| Caillon and Esling, *RAVE* (2021 preprint) | A variational autoencoder uses a compact latent representation, multiband synthesis and two-stage training. Expose model-specific latent controls and measure buffering, lookahead and processing separately. The paper's throughput result does not establish Prism latency. [Paper](https://arxiv.org/abs/2111.05011) |
| Engel et al., *DDSP* (ICLR 2020) | Differentiable signal-processing operators let learned models control structured synthesis. Pitch/loudness-conditioned harmonic and noise resynthesis suggests an editable timbre-transfer module. Pitch-estimation errors and polyphonic inputs need explicit evaluation. [Paper](https://arxiv.org/abs/2001.04643) |
| Défossez et al., *High Fidelity Neural Audio Compression* (2022 preprint) | EnCodec learns an encoder/decoder and quantized audio representation. Codec tokens require model-specific metadata and compatible codebooks. Reconstructing compressed audio alone supplies no text conditioning or interpretable timbre axis. [Paper](https://arxiv.org/abs/2210.13438), [implementation](https://github.com/facebookresearch/encodec) |
| Liu et al., *AudioLDM* (ICML 2023) | Text-conditioned latent diffusion generates audio using CLAP representations. Use an asynchronous asset-generation job with seed, model revision and output provenance, followed by ordinary sample loading. [Paper](https://arxiv.org/abs/2301.12503), [proceedings](https://proceedings.mlr.press/v202/liu23f.html) |
| Magenta DDSP-VST | Provides neural timbre-transfer effects and a MIDI-playable synthesizer. This motivates separate Audio and MIDI input modes, with visible pitch tracking and level control. [Developer repository](https://github.com/magenta/ddsp-vst), [official plugin catalog](https://magenta.withgoogle.com/plugins) |
| IRCAM `nn~` | Hosts exported neural models in Max and Pure Data, including RAVE models. Model methods and channel dimensions form an integration contract. Prism must obtain these from a validated manifest. [Developer repository](https://github.com/acids-ircam/nn_tilde) |
| Neutone FX | The published catalog includes DDSP instrument models and RAVE timbre-transfer models with declared input/output layouts. A model browser should expose those capabilities and provenance before insertion. [Official model catalog](https://neutone.jp/fx/models) |

### Granular instruments [SYNTH-RESEARCH-GRANULAR-PRODUCTS]

Truax's author overview describes dense clouds of short events and links his *Real-Time Granular Synthesis with a Digital Signal Processor* (Computer Music Journal 12(2), 1988). This supplies the scheduling/windowing foundation. The proposed 2–500 ms control range is a musical design choice extending beyond the overview's typical short-grain examples. [Author overview and bibliography](https://www.sfu.ca/~truax/gran.html)

| Instrument and inspected source | Documented approach | Proposed Prism application |
| --- | --- | --- |
| Pigments, manual 5.0 | Granulation is part of the Sample engine. The indexed official manual establishes that relationship. Full PDF retrieval exceeded the browser limit, so detailed control parity remains unverified. | A sample waveform with position, grain size and density in the selected module inspector. [Official manual](https://dl.arturia.net/products/pigments/manual/pigments_Manual_5_0_0_EN.pdf) |
| Portal, Grain Controls help page | Captures incoming audio into a buffer, with grain density, length, count, pitch, reverse, offset, shape and feedback. Timing controls can use rhythmic relationships. | An explicit live-input mode with capture/freeze, tempo synchronization and reproducible randomness. [Manufacturer technical guide](https://support.output.com/en/articles/10297851-grain-controls) |
| Quanta 2, manufacturer product documentation | A sample-based grain voice combines grain rate, pitch, direction, shape, length and position with synthesis sources and modulation. | Per-note grain voices and modulation destinations with visible resource limits. [Manufacturer documentation](https://www.audiodamage.com/collections/granular-synthesis/products/ad055-quanta-2) |
| Tasty Chips GR-1, manual 3.4.b | Hardware granular instrument organized around samples, scan position and direct grain controls. | Waveform scanning, frozen-position performance and direct numeric control alongside gestures. [Manufacturer manual](https://www.tastychips.nl/downloads/GR-1/UserManual/User-manual-GR-1-v3.4.b.pdf) |
| Make Noise Morphagene, manufacturer manual | Organizes material into Reels, Splices and Genes. Gene Size, Slide, Morph and Vari-Speed control microsound playback. Recording and playback can operate independently. | Region selection and grain scanning with independently specified capture, transport and pitch behavior. [Manufacturer manual](https://www.makenoisemusic.com/wp-content/uploads/2024/03/morphagene-manual.pdf) |

### Physical instruments and numerical methods [SYNTH-RESEARCH-PHYSICAL-PRODUCTS]

Bilbao et al., *Physical modeling, algorithms and sound synthesis — The NESS Project*, Computer Music Journal 43(2–3), 15–30, is recorded by Edinburgh as published in 2020. It discusses numerical simulation and the relationship between physical systems and synthesis algorithms. Use energy behavior and numerical stability as design obligations for strings, tubes, membranes and coupled bodies. The source's publication record takes precedence over search crawl dates. [Institutional full text](https://www.pure.ed.ac.uk/ws/portalfiles/portal/121715237/cmj_ness_aam.pdf)

| Instrument | Documented approach | Proposed Prism application |
| --- | --- | --- |
| Mutable Instruments Rings | Modal resonator, sympathetic strings and a dispersive string model, with excitation supplied externally or internally. | Independently routable excitation and body, plus compact material, brightness, damping and position controls. [Original-author manual](https://pichenettes.github.io/mutable-instruments-documentation/modules/rings/manual/) |
| Mutable Instruments Elements | Modal voice with blow, bow and strike exciters, geometry and material controls. | Excitation choices with physically meaningful controls and visible body response. [Original-author manual](https://pichenettes.github.io/mutable-instruments-documentation/modules/elements/manual/) |
| Rings/Elements clones | The original documentation supplies a reference design. Clone hardware and alternate firmware may change modes, controls and behavior. | Record exact firmware revision and hardware before parity testing. No clone-wide compatibility claim follows from the original manual. [Original source repository](https://github.com/pichenettes/eurorack) |
| AAS Chromaphone 3 | Couples resonant objects so they exchange energy, alongside excitation and resonator controls. | Coupling belongs inside a bounded coupled-body processor with a documented energy model. [Manufacturer manual, Coupling of Resonators](https://www.applied-acoustics.com/chromaphone-3/manual/) |
| Ableton Collision, Live 12 manual | Mallet/noise excitation and resonator sections expose the construction of a physical percussion voice. | A strike-to-body workflow and material presets with editable parameters. [Official instrument reference](https://www.ableton.com/en/live-manual/12/live-instrument-reference/#collision) |
| Ableton Tension, Live 12 manual | Models string excitation, damping, termination, pickup and body. Exciters include bow, hammer, bouncing hammer and plectrum. | Separate playing gesture from string/body properties, with conditional controls for each exciter. [Official instrument reference](https://www.ableton.com/en/live-manual/12/live-instrument-reference/#tension) |

The earlier Washington-hosted Smith paper now redirects to authentication, and Stanford's book blocks automated retrieval. Keep those bibliographic leads with that access limitation. The accessible NESS paper and original instrument manuals support this proposal's physical-model requirements.

### Combining the methods and assessing originality [SYNTH-RESEARCH-HYBRID]

| Prior art | Consequence for the proposed combined method |
| --- | --- |
| *Neural Granular Sound Synthesis* (2020 preprint, revised 2021) | Learned grain representations and trajectories already exist. A neural grain generator alone is established prior art. [Paper](https://arxiv.org/abs/2008.01393) |
| Castle, Adcock and Barrass, *Integrated Modal and Granular Synthesis of Haptic Tapping and Scratching Sounds* (Eurohaptics 2002) | Combines granular scratching and modal tapping for interactive objects. The author publication page and indexed paper abstract establish this precedent. Full mirrored PDF retrieval failed. [Author publication entry](https://sonification.com/2002/10/10/haptic-scratching-and-tapping/), [paper mirror](https://citeseerx.ist.psu.edu/document?doi=8bb69ce847735352895718172f138fae43e3a5f2&repid=rep1&type=pdf) |
| *Differentiable Modal Synthesis for Physical Modeling of Planar String Sound and Motion Simulation* (NeurIPS 2024) | Differentiable physical modal models already connect learned control and instrument physics. [Conference publication](https://papers.nips.cc/paper_files/paper/2024/hash/0232cafe8d1909a01019abe8af32f3e1-Abstract-Conference.html), [preprint](https://arxiv.org/abs/2407.05516) |
| *Stable Differentiable Modal Synthesis for Learning Nonlinear Dynamics* (2026) | Stability-aware learned modal dynamics are also prior art. Evaluate its assumptions before selecting a stability parameterization. [Paper](https://arxiv.org/abs/2601.10453), [authors' implementation](https://github.com/victorzheleznov/jaes-modal-node) |
| sonicLAB SSNN | The manufacturer describes a spiking neural network driving multiple engines including granular and modal engines. Shared neural event triggering is already a product design. [Manufacturer description](https://www.sonic-lab.com/ssnn-page) |

Our proposed experiment gives each grain a contact position, duration, force budget and latent descriptor within one evolving physical body. The body's previous modal state influences subsequent grain selection and timing. A learned controller proposes these interactions, and a checked physical update constrains them. The potential contribution is this particular state-dependent interaction and its controllable behavior. Originality remains a hypothesis after this bounded search. The [combined-method contract](#combined-grain-and-body-method-synth-advanced-hybrid) defines the algorithm and comparisons needed to evaluate it.
