# The synth package [SYNTH]

The [advanced synthesis proposal](#advanced-synthesis-proposal-synth-advanced) specifies the neural, granular, physical and combined methods; the application built on this package is specified in [Perception](Perception.md). The [implementation checklist](../../plans/synth/prism.md) tracks delivery and verification.

Status — implementation in progress. The initial package extracts Perception's pure pitch, waveform, modulation, sample and WAV functions. Typed graph validation/scheduling, reactive parameter snapshots and stateful scalar reference oscillators, ADSR and filters now have passing package journeys. The real-time renderer and advanced algorithms remain requirements, not completed features. Research sources and candidate experiments are in [the research guide](Synth.md#sources-and-experiments-synth-research).

`@osprey/synth` is an independent synthesis library for instruments, effects, offline rendering and future plug-in hosting. It contains no UI, native widgets, canvas rendering, networking, session management or operating-system audio-device code. Perception and Light Orchestra are examples using the library. The orchestra's number of phones and instruments must not constrain library design.

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

## Routed modulation [SYNTH-GRAPH-MODULATION]

A normalized sine LFO is a separate control-rate component with its own phase and frequency. A controlled-gain node takes distinct audio and normalized-control inputs; one control sample broadcasts across all audio channels while audio cables retain their declared width. The transfer functions are authored in `waves.ospml`, and the renderer shares oscillator state handling with audio oscillators. Missing controls and values outside zero through one are explicit errors. Native menu actions and canvas cable actions use the same rate/unit/channel validation.

The current graph clip path renders modulation at sample resolution. This establishes LFO-driven stereo amplitude modulation. Hosted parameter routes are described below; continuous device processing remains delivery work.

## Verification and delivery [SYNTH-VERIFY]

- [x] Extract reusable synthesis from Perception without introducing a UI dependency into the package.
- [x] Run Perception's existing full interaction/voice suite before and after extraction.
- [x] Collect foundational and recent primary research with implementation experiments.
- [x] Add independent package tests and package governance/coverage integration (package journeys and a 90% minimum own-source line coverage gate).
- [x] Implement typed graph construction/validation, routing, chaining and explicit feedback scheduling; the mono reference graph now executes sources, mixing, filters, gain and delay feedback.
- [x] Expose component parameters/state through typed Signals and verify independent component IDs, applied revisions and rejected stale edits; concurrent publication remains pending.
- [x] Implement stateful reference DSP and reproducible sample-accurate block processing, including parameter automation and per-component peak/RMS observations.
- [ ] Verify generated native callback allocation/locking behaviour and target-device deadlines.
- [ ] Establish spectral, stability, continuity, polyphony and overload evidence.
- [ ] Integrate Perception and Light Orchestra through the same reusable engine.
- [ ] Add VST3 hosting after the component/graph boundary is verified.

Test complete instrument journeys with many notes, automation events and routing changes. Use isolated numerical tests where mathematical or real-time isolation is necessary. Compare against independent analytical or high-precision references; record seeds, sample rates, block partitions and tolerances. Keep exact PCM goldens only where cross-target numerical behaviour is defined, and retain listening artefacts under `target/` for review. Screenshot tests belong to consuming applications, not this UI-free library.

## Rendering and real-time contract [SYNTH-RENDER]

Preparation takes sample rate, channel layout, maximum block size, voice/component counts and event capacity. Rendering uses caller-owned/preallocated buffers and explicit component state. The callback performs no allocation/free, GC, blocking locks, file/network I/O, logging, model loading or unbounded work. Osprey compilation to native code by itself does not prove this property; inspect and instrument the generated callback path. If current runtime facilities cannot satisfy it, report the gap and keep that path outside the callback.

Events carry sample offsets and stable ordering within a block. Note ownership, retrigger, sustain, per-note expression, voice stealing and release policy are explicit. Automation supports ramps with declared interpolation, avoiding accidental block-rate steps. Different legal block partitions of one event timeline must yield equivalent sound and final state within a documented numerical tolerance.

Initial production rate targets are 44.1, 48 and 96 kHz, with irregular block sizes including 1, 31, 64, 127, 256 and 1024 samples. A platform may negotiate others only through checked preparation. Rate changes, device loss, transport jumps and reset have defined state transitions. Peak CPU, callback deadline misses, latency, memory and output quality are measured separately on named hardware. No hard-real-time guarantee is implied by a desktop operating system.

Invalid configuration is rejected before activation. Runtime failures return a typed fault and bounded diagnostics. A host may explicitly choose to silence a failed stream, but it must also surface the failure; silence is not a fabricated successful render. Non-finite input, parameter overflow and numerical instability have specified handling and tests.

## Stateful multichannel blocks [SYNTH-RENDER-BLOCKS]

`ClipKernels.block` accepts and returns independent per-channel kernel state alongside interleaved samples at the hosted path's 48 kHz rate. Unequal block partitions retain oscillator/LFO phase, filter integrators and delay history. The component must exactly match its prepared graph entry, and state identities, channel counts and block capacities are checked before processing. The existing whole-clip entry point initializes state and delegates to this same implementation.

`GraphClips.start`, `next` and `replied` schedule those states across complete acyclic graph blocks. A typed pending request owns the frozen graph, current block buffers and kernel states; only its named external node can supply the checked response. The graph clock advances once the entire block completes. MIDI note ownership and the 128-frame automation grid use that absolute clock, including unequal block sizes and positions beyond the bounded whole-clip window. A continuing graph that loses or duplicates required kernel state is rejected.

This is producer-side, allocating reference execution. It does not establish allocation-free device callbacks, persistent VST sessions in Perception, graph-wide streaming scheduling or uninterrupted device playback; those integration steps remain required.


## DSP quality [SYNTH-DSP]

The extracted Perception renderer is a reference/example path: 11,025 Hz, unsigned 8-bit WAV output, naive periodic waves and a fundamental-gain approximation to filtering. It is not the production engine. Production kernels require continuous phase/state, high-resolution floating-point audio, bounded envelopes, smoothing, proper stateful filters and documented antialiasing behaviour. Avoid retaining parallel versions once the production replacement is verified.

Oscillator quality is evaluated under pitch changes, pulse-width modulation, phase modulation, hard sync and negative/near-zero rates where supported. Filters are evaluated under cutoff/resonance automation and feedback. Nonlinear processors require explicit oversampling/ADAA policies and declared latency. Modulation and mixing remain composable rather than being baked into one fixed subtractive voice.

The `Grains` reference kernel [SYNTH-ADVANCED-GRAINS-PLAYBACK] implements immutable mono-source grain pools with stereo summing, periodic linear interpolation, independent lifetime/rate, symmetric Hann or configurable linear ASR windows and equal-power placement. Preparation validates source data and capacity; trigger/render operations reject invalid state and exhausted counters. Its generated models expose coherent sample position, trigger revision and active-count observations through a UI-independent Signal effect. Strict static checking has passed; automated audio verification is paused. `GrainScheduler` now supplies periodic and seeded bounded-jitter onset clocks; `Grains.scheduled` consumes ordered absolute sample events before the corresponding sample, preserving equal-position input order and rejecting events outside the current half-open block. Both clock and player state remain immutable on failure. `Components.granular` and `GrainGraph` now connect that engine to the shared stereo graph renderer with typed retained processor memory. Perception exposes Grain Cloud insertion, its existing parameter inspector, source waveform and keyboard pitch mapping. Independent distributions, resource replacement/file loading, separate scheduler/source nodes, modulation ports and realtime guarantees remain requirements in the advanced plan; manual native insertion/inspector inspection is complete, while audio and automated UI verification remain deferred.

Research candidates include bandlimited/corrected oscillators, wavetable/additive/spectral synthesis, physical/modal models, granular engines, expressive phase shaping and differentiable/learned controls. The detailed implementation and acceptance checklist is the [advanced synthesis plan](../../plans/synth/prism.md) [SYNTH-ADVANCED]. Adoption requires the [research experiments](#research-to-delivery-experiments-synth-research-experiments), not a novelty claim. Training and expensive patch inference remain outside the required audio callback.

### Mathematical sources and proof obligations [SYNTH-DSP-PROOFS]

| Technique | Published result and its application to the library |
| --- | --- |
| Additive synthesis and wavetable oscillators | Puckette derives superposition and describes reading one period of a waveform at a prescribed phase increment. `TableMath.harmonic` supplies the individual partial. A finite sum establishes the spectrum of the constructed period; fractional table reads introduce a separate interpolation error. [Superposing Signals](https://msp.ucsd.edu/techniques/latest/book-html/node13.html) · [The Wavetable Oscillator](https://msp.ucsd.edu/techniques/latest/book-html/node27.html) |
| Mixing and panning | Puckette §1.6 adds two signals sample by sample and expands the square of each sum, so the power of the mix is P{x} + P{y} + 2·COV{x, y}, and scaling a signal by k scales its power by k². Uncorrelated signals add their powers. MIDI RP-036 sets the left gain to cos(π/2·p) and the right to sin(π/2·p); the Pythagorean identity makes the two channel powers sum to one at every position. `ArrangeMath.bused` sends a sample onto a bus at a gain; `panLeft` and `panRight` use p = (pan + 1)/2. [Superposing Signals](https://msp.ucsd.edu/techniques/latest/book-html/node13.html) · [MIDI RP-036 Default Pan Formula](https://midi.org/default-pan-formula) |
| Sample-based granular synthesis | Puckette applies an envelope to a sample reader. In `GrainWindows.hann` the symmetric Hann uses duration minus one in its denominator, so the first and last samples of a grain of length at least three have zero gain. Overall gain depends on the individual onset times and envelopes. [Enveloping samplers](https://msp.ucsd.edu/techniques/latest/book-html/node29.html) |
| Phase modulation and distortion | Smart (DAFx25), equations (1) and (3)–(9), specifies the carrier, ramp, window and two pulse branches used by `PhaseMath`. With both modulation indices zero, equation (9) reduces to the two branches of equation (7) for an interior breakpoint. [Wave Pulse Phase Modulation](https://dafx.de/paper-archive/2025/DAFx25_paper_48.pdf) |
| Physical waveguides | Smith derives equation (7.2) by substituting two oppositely travelling waves into the ideal lossless one-dimensional wave equation. Sampling one spatial step per time step yields the unit-delay propagation rule. The loss filter, dispersion and boundary reflection in Osprey each require additional reasoning. [Digital Waveguide Models, Traveling-Wave Solution](https://www.dsprelated.com/freebooks/pasp/Digital_Waveguide_Models.html) |
| Spectral analysis and resynthesis | Allen and Rabiner derive short-time Fourier synthesis. Smith gives the weighted overlap-add condition. For `SpectralMath.hann` applied twice, the squared periodic Hann windows sum to `3N/(8H)` for hop `H = N/4` or `N/8`. Its reciprocal reconstructs unchanged frames wherever all overlapping windows contribute. Stream boundaries need their own padding or normalization argument. [Allen and Rabiner (1977)](https://web.ece.ucsb.edu/Faculty/Rabiner/ece259/Reprints/122_unified_approach_to_STFT.pdf) · [Smith, Weighted Overlap Add](https://www.dsprelated.com/freebooks/SASP/Weighted_Overlap_Add.html) |
| Nonlinear waveshaping | Parker, Zavalishin and Le Bivic (DAFx16), §2, integrate a memoryless transfer curve along a linearly interpolated input. At distinct sample values this gives the divided difference of an antiderivative used by `Waveshapers.shaped`. The midpoint approximation for nearly equal inputs needs a separate error bound for each curve. [Reducing the Aliasing of Nonlinear Waveshaping Using Continuous-Time Convolution](https://dafx.de/paper-archive/2016/dafxpapers/20-DAFx-16_paper_41-PN.pdf) |

The Osprey kernel checks `Synthesis.weightedOverlapAdd` in `make _test_synthesis_proofs`. It proves the reconstruction direction of Smith’s equation (9.44) for any finite list of overlapping window products whose sum is one. The proof factors the sample out of the sum by distributivity and applies the unit law. The periodic Hann uses half the amplitude of the unscaled raised cosine. For hop N/4, `hannQuarterEnergy` pairs opposite shifts and applies the sum-of-squares identity and the trigonometric unit-circle identity. The four squared unscaled amplitudes sum to six, so the squared Hann amplitudes sum to 6/4. For hop N/8, `hannEighthEnergy` applies the same result separately to even and odd shifts, giving 12/4. The reciprocal gains are respectively 2/3 and 1/3, as in `SpectralStreams.overlapScale`. Both sums assume full window coverage. [Smith, squared-window condition (9.45)](https://www.dsprelated.com/freebooks/SASP/Weighted_Overlap_Add.html) · [NIST DLMF, trigonometric identity (4.21.12)](https://dlmf.nist.gov/4.21.E12)

The same gate checks the following results in exact arithmetic.

- `Synthesis.travelingWavePropagation` follows Smith’s sampled solution (7.3). For arbitrary rail signals at any nonnegative site, a time step carries the right-going value one site right and the left-going value one site left. The time indices have a successor and a predecessor that undoes that successor. This covers integer-grid transport inside an ideal delay line; `DelayLines.tap` also interpolates fractional positions. [Smith, Sampled Traveling-Wave Solution](https://www.dsprelated.com/freebooks/pasp/Digital_Waveguide_Models.html)
- `Synthesis.additivePeriod` follows Puckette’s finite Fourier sum. For any finite set of partials that repeat after a common phase advance, their sum with a constant offset repeats after that advance. This gives the period-preservation step in additive table construction, conditional on each partial repeating. It makes no smallest-period or floating-point equality claim. [Puckette, Periodic Signals](https://msp.ucsd.edu/techniques/latest/book-html/node14.html)
- `overlapNormalizationNecessary` supplies the converse of the reconstruction result above. Feeding the constant unit signal through the overlapping frames forces the sum of their window products to equal one. Together the two proofs give both directions of equation (9.44). [Smith, Weighted Overlap Add](https://www.dsprelated.com/freebooks/SASP/Weighted_Overlap_Add.html)
- `interpolatorFactor`, `interpolatorEndpoints` and `interpolatorConstant` establish Smith’s one-multiply rearrangement of equation (5.1), exact recovery at both endpoints, and constant-sample preservation at any fraction. The graph’s fractional delays and grain reader use this rearrangement in `Interpolation.linear`. [Smith, Linear Interpolation](https://www.dsprelated.com/freebooks/pasp/Linear_Interpolation.html)
- `zeroPulseIndices` substitutes zero for both indices in Smart’s windowed modulation terms. Each product vanishes, leaving the corresponding phase-distortion branch of `PhaseOperators.shaped`. This algebraic result treats each branch’s base phase, window and sinusoid as its evaluated value. [Smart, equations (7)–(9)](https://www.dafx.de/paper-archive/2025/DAFx25_paper_48.pdf)
- `hannEndpoints` substitutes the cosine values at zero and one turn in the raised cosine. With Osprey’s symmetric grain convention the last index is duration minus one. The periodic spectral window instead completes its turn at index N, outside the frame. [Smith, Hann window, equation (4.18)](https://www.dsprelated.com/freebooks/sasp/Hann_Hanning_Raised_Cosine.html)
- `allpassRecurrence` factors the two products in `Allpasses.next` into Smith’s one-multiply difference equation. `allpassMagnitude` shows that on the unit circle the numerator of (5.2) is a unit rotation of the conjugate denominator. Their magnitudes are equal for every frequency and real coefficient, giving unity gain wherever the denominator is nonzero. [Smith, First-Order Allpass Interpolation](https://www.dsprelated.com/freebooks/pasp/First_Order_Allpass_Interpolation.html)
- `antiderivativeAverage` applies the fundamental theorem of calculus to the oriented integral in Parker et al. (8), then reverses both differences to recover (9) as used in `Waveshapers.shaped`. Two corollaries check that adding a constant to the primitive leaves the quotient unchanged (`antiderivativeOffset`), and that a constant transfer curve returns its gain (`antiderivativeConstantCurve`). The integral derivation assumes distinct inputs. The implemented threshold and midpoint fallback require separate numerical analysis. [Parker, Zavalishin and Le Bivic, §2.2–2.3](https://www.dafx.de/paper-archive/2016/dafxpapers/20-DAFx-16_paper_41-PN.pdf)

The FFT proofs use a finite polynomial for each frequency bin. `radixTwoSplit` separates even and odd samples at the squared root of unity; `radixTwoUpper` negates the odd contribution for the upper bin. `realBinConjugate` conjugates each term of a real-input transform, the identity underlying half-spectrum storage. `binAdditive` and `binHomogeneous` establish superposition by distributing bin weights over each input. These proofs cover exact arithmetic. [Smith, decimation in time (A.1)](https://www.dsprelated.com/freebooks/mdft/Decimation_Time.html) · [Radix 2 FFT](https://www.dsprelated.com/freebooks/mdft/Radix_2_FFT.html) · [Symmetry](https://www.dsprelated.com/freebooks/mdft/Symmetry.html) · [Linearity](https://www.dsprelated.com/freebooks/mdft/Linearity.html)

#### Proofs of handwritten Osprey bodies [SYNTH-DSP-PROOFS-PRODUCTION]

The synthesis CI gate parses the shipped `delay_lines.ospml`, `wavetables.ospml`, `table_math.ospml`, `spectral_math.ospml`, `ffts.ospml`, `spectral_frames.ospml`, `strike_combs.ospml`, `allpasses.ospml`, `nonlinear_math.ospml`, `waveshapers.ospml`, `interpolation.ospml`, `phase_math.ospml`, `phase_operators.ospml`, `grain_math.ospml`, `grain_windows.ospml` and `spectral_streams.ospml` from the synth package, plus `fit_math.ospml` and the DAW’s `arrange_math.ospml`, `timeline_geometry.ospml`, `canvas_geometry.ospml`, and `dial_math.ospml`, with the Osprey ML frontend. It translates the canonical syntax trees of `Ffts.added`, `subtracted`, `rotated`, `combined`, `quartet` and `conjugate`, plus `SpectralFrames.scaled`, `turned` and `rotation`, `StrikeCombs.delay`, `StrikeCombs.next`, `Allpasses.next` and the waveshaper’s curve, integral and sample selection, into definitions for the proof kernel. Unqualified helper calls resolve within the source module. The gate rejects a module declaration that shadows a mapped primitive or a call to a local helper whose body was omitted. Unsupported syntax stops the gate. The translator and its mappings for list constructors, records and primitive calls are part of the trust boundary.

| Production proof | Property checked for every input |
| --- | --- |
| `ProductionFft.additiveSample` and `ProductionFft.additivePeriod` | The wavetable builder sums harmonics in source order from its initial sample value. Advancing one cycle preserves the sum under integer-turn sine periodicity and the stated arithmetic laws. CI mutates the phase expression and swaps the phase and amplitude in the actual caller. [Puckette, Periodic Signals](https://msp.ucsd.edu/techniques/latest/book-html/node14.html) |
| `ProductionFft.rotationArithmetic` | The production FFT rotates a bin using the signed twiddle angle and the real and imaginary parts of complex multiplication from `spectral_math.ospml`. Three source mutations check the sign, cross terms, and angle denominator. |
| `ProductionFft.combinedCorrect` | Each paired even/odd bin yields a lower sum and upper difference in input order, with the correct rotation arguments, sign, size and successive indices. Output stops at the shorter input. |
| `ProductionFft.combinedDftBins` | Given the two half-transform bin values, the production combine emits the full-transform bins at roots z and −z. This applies Smith (A.1) to the parsed body under exact scalar algebra and the hypothesis that rotation implements multiplication by z. |
| `heardTimeClamped`, `dialMovement` and `canvasGrabOffset` | The timeline clamps the extrapolated clock to device frame boundaries, the dial uses a vertical two-unit drag threshold, and the canvas recovers the pointer from its grab offset under exact cancellation. |
| `macroLow`, `macroHigh`, `macroMonotone` and `macroInRange` | [PRISM-INSTRUMENT-PROOF] The native `FitMath.macroValue` gives the low end at amount zero and the high end at amount one, never decreases as the amount rises when the shaping power is monotone and the range runs upward, and so stays inside its range for every amount from zero to one, under the stated zero, unit, cancellation and order laws. CI swaps the power's arguments, reverses the span and moves the base to the high end. |
| `ProductionFft.envelopeSustain` and `ProductionFft.envelopeReleased` | The DAW arrangement renderer applies its native envelope to the sample. After attack, a held note passes the sample; after release, it produces zero under the stated scalar laws. Times in seconds are converted using the actual sample rate. |
| `mixSuperposition`, `mixPower`, `panEqualPower` and `panHardLeft` | [SYNTH-DSP-PROOFS-MIXING] A silent bus that receives x at gain g and y at gain h through the native `ArrangeMath.bused` holds g·x + h·y in either arrival order. Over any two windows of equal length, the summed squares of that mix are g²·Σx² + h²·Σy² + 2·gh·Σxy, which is Puckette’s power-of-a-sum equation scaled by the window length; the proof expands each squared sample as Puckette does and assumes commutative-semiring laws for the scalars. The window fold is a proof model of the per-sample kernel, not the shipped list code. Both native pan gains read the same angle π/4·(pan + 1), RP-036’s π/2·p, so under the Pythagorean identity their squares sum to one; at pan −1 the left gain is one and the right zero. Source edits to the send, either pan coefficient or the pan offset break the proofs. |
| `transformSchedule` and `supportedTransformDepths` | The extracted recursive body reaches its base stage after five to eleven calls on each path for the seven entries in the extracted `Ffts.sizes` table, from 64 through 4096. For any input list and either sign, it reads the even and odd strided halves, propagates read errors, and combines lower and upper butterfly bins in order. The proof unfolds source calls with a depth bound and shows the exhaustion value is unreachable. The arithmetic model treats valid nonnegative frame sizes as natural numbers. |
| `negativeFrameRejected`, `advertisedSizesAccepted` and `validatedTransformSchedule` | The production validator rejects every negative integer and accepts each advertised frame size. Every nonnegative size it accepts has a terminating schedule for either transform sign. |
| `ProductionFft.stridedQuartet` | At unsaturated addresses, the base stage reads the four strided bins in order. It returns the quartet when all four are present and an incomplete-frame error when any read is missing. The model uses zero-based list lookup. |
| `ProductionFft.quartetCorrect` | The fused four-sample base case computes two consecutive radix-two stages, including the twiddle arguments and the order of all four outputs. |
| `ProductionFft.conjugateComponents` and `conjugateTwice` | Conjugation preserves the real component and subtracts the imaginary component from zero. Double conjugation recovers the bin under the stated scalar involution law. |
| `realSamplesConjugate` and `addedDftBin` | For real input, conjugating the finite DFT bin polynomial conjugates its root. Pointwise addition of two frames distributes over the same bin polynomial. Both statements use the shipped complex addition and product, with the shipped conjugation in the first. The proofs assume the listed exact scalar identities. [Smith, DFT symmetry](https://www.dsprelated.com/freebooks/mdft/Symmetry.html) and [DFT Linearity](https://www.dsprelated.com/freebooks/mdft/Linearity.html) |
| `ProductionFft.scaledComponents` and `scaledDftBin` | The frame gain multiplies both components by the same gain. Under distributivity and complex-product homogeneity, applying that production gain before or after a DFT gives the same bin. The proof uses the production complex sum and product bodies. [Smith, Linearity](https://www.dsprelated.com/freebooks/mdft/Linearity.html) |
| `rotationConjugate`, `rotationComponents` and `rotationUnitPower` | The inter-frame rotation takes the current bin times the conjugate of its predecessor. The proof follows the native squared magnitude, square root and both component divisions. A nonpositive magnitude selects (1, 0). On the other branch the squared modulus is one under the stated exact square-root and division laws. Finite components whose squares overflow can produce zero after division by an infinite magnitude. |
| `hannEnergyQuarter` and `hannEnergyEighth` | Four squared samples of the native periodic Hann sum to 3/2 under the stated arithmetic and quarter-turn trigonometric identities. Eight samples, grouped by even and odd shifts, sum to 3. Both assume full overlap. Changing the cosine coefficient in the production body breaks the proof. [Smith, squared-window condition (9.45)](https://www.dsprelated.com/freebooks/SASP/Weighted_Overlap_Add.html) |
| `delayPropagation` and `delayFullTraversal` | Each native delay-line write advances the stored waveform one position and preserves capacity. After one full traversal, arbitrary input samples occupy the entire line in arrival order. This covers integer sample transport. Fractional reads, damping and boundary reflections require additional properties. [Smith, digital waveguide models](https://www.dsprelated.com/freebooks/pasp/Digital_Waveguide_Models.html) |
| `ProductionFft.strikeDifference` and `strikePeriodicNull` | The physical string excitation subtracts the delayed sample and retains the new history. Under the library’s post-push convention it reads at one plus the strike delay. A repeating input cancels when this read returns the current sample and self-subtraction gives zero. A failed read returns the original error. Fractional-delay accuracy is outside this statement. [Smith, Equivalent Forms](https://www.dsprelated.com/freebooks/pasp/Equivalent_Forms.html) |
| `allpassStateStep` and `allpassFactoredStep` | The handwritten `Allpasses.next` evaluates the first-order recurrence and stores the current input and computed output. Under distributivity the recurrence has Smith’s one-multiply form. |
| `allpassTransfer` and `allpassSteadyMagnitude` | Substituting delayed steady-state phasors into that body gives (1 + aq)y = (a + q)x. For a real coefficient, unit-circle delay and nonzero denominator, Smith (5.2) gives equal input and output magnitudes in exact complex arithmetic. [Smith, First-Order Allpass Interpolation](https://www.dsprelated.com/freebooks/pasp/First_Order_Allpass_Interpolation.html) |
| `shaperCurvePair`, `shaperBypass`, `shaperMidpoint` and `shaperQuotient` | All four curve choices dispatch to their matching primitive. The production waveshaper selects the current curve when antialiasing is off, the midpoint curve below the absolute-difference threshold 0.00001, and the divided difference otherwise. |
| `upperClipRails`, `tanhUpperPlateau`, `hardClipPositiveInterval` and `wavefolderPair` | Hard and cubic clipping return +1 above their upper thresholds. The tanh approximation is constant above +20. On the positive hard-clip rail, the antialiased output is +1 for separated samples under the stated cancellation and division laws. The wavefolder and its primitive use the same angle, with the exact binary64 coefficients read from Osprey. [Waveshaper proofs](../../../src/toolchain/benchmarks/proof/tools/ProductionWaveshaper.lean.in) |
| `shaperSymmetricInterval` | For any of the four native transfer curves, two opposite samples on the divided-difference branch produce zero under the stated sign, subtraction and division laws. The proof uses the even symmetry of each production antiderivative. |
| `shaperIntervalAverage`, `shaperPrimitiveOffset` and `shaperConstantSegment` | On the divided-difference branch, the fundamental theorem of calculus gives the interval average when the selected primitive integrates the selected curve. A common primitive offset cancels; a segment with constant gain returns that gain. The fallback’s approximation error depends on curve curvature. [Parker et al., equations (8)–(9)](https://www.dafx.de/paper-archive/2016/dafxpapers/20-DAFx-16_paper_41-PN.pdf) |
| `interpolationEndpoints`, `interpolationInterior`, `interpolationWeights` and `interpolationConstant` | The actual sample reader returns the selected input unchanged at fractions zero and one. Other fractions evaluate the one-multiply form. Smith’s weighted form follows under exact scalar algebra; a constant pair stays constant under the stated zero laws. At fraction one, interpolating from −1 to 10⁻¹⁷ now returns 10⁻¹⁷; binary64 evaluation of the previous expression returned zero. [Smith, Linear Interpolation](https://www.dsprelated.com/freebooks/pasp/Linear_Interpolation.html) |
| `windowedModulation` and `windowedZeroIndex` | The windowed phase operator uses the shipped sine window and the modulation angle from Smart’s equations (5)–(6). A zero index recovers the incoming phase under the stated zero-multiplication and addition laws. CI changes the window to cosine and changes the order of the angle multiplications to exercise both dependencies. [Smart, Wave Pulse Phase Modulation](https://www.dafx.de/paper-archive/2025/DAFx25_paper_48.pdf) |
| `phasePulse`, `wavePulseSegments` and `wavePulseZeroIndices` | The handwritten phase operator uses the controls and local ramp for each pulse in Smart’s equations (7)–(9). With both indices zero, zero multiplication and addition reduce both branches to phase distortion. CI swaps each branch’s controls in turn and requires rejection. [Smart, Wave Pulse Phase Modulation](https://www.dafx.de/paper-archive/2025/DAFx25_paper_48.pdf) |
| `grainAsrBoundarySamples` and `grainAsrInterior` | The shipped attack–sustain–release grain window gives zero at the first and last samples under the stated cast and comparison laws. Interior samples follow the linear attack or release ramp, with unity on the plateau. The production tests exercise unequal fractions, both transition samples and a one-sample peak. [Grain window journeys](../../../src/osprey/packages/synth/tests/grain_journeys.ospml) |
| `grainHannEndpoints`, `grainHannInterior` and `grainHannBoundarySamples` | The shipped grain window returns zero at its boundaries and evaluates the symmetric Hann expression on interior samples. The boundary-sample theorem connects natural sample indices to the endpoint checks under its conversion hypotheses. CI changes the denominator and requires rejection. |
| `overlapNormalization`, `overlapReconstructs` and `overlapNecessary` | The streaming accumulator reconstructs an unchanged sample when the effective window products sum to one and the scalar operations satisfy the stated arithmetic laws. Conversely, if the accumulator reconstructs every constant sample, applying it to the unit sample proves the effective gains sum to one. The gain and the accumulator come from the production source. The theorem assumes full normalized coverage and leaves frame scheduling and FFT inversion as separate obligations. [Smith, Weighted Overlap Add, equation (9.44)](https://www.dsprelated.com/freebooks/SASP/Weighted_Overlap_Add.html) |

These proofs quantify over scalar operations, including scalar arithmetic and trigonometric primitives. They check the same sequence of calls even when those operations round. The recursive schedule is checked for supported sizes. The compiled [FFT journey](../../../src/osprey/packages/synth/tests/fft_journeys.ospml) also compares each forward bin against the direct Fourier sum of three impulses and checks every inverse sample, with absolute tolerance 1e-12 on that frame at each of the seven sizes. A complete end-to-end numerical error bound, equivalence of every output to the direct DFT, and correctness of native code generation remain separate obligations. CI checks every production certificate under native GC, native ARC and WebAssembly. It regenerates the body after fifty-one production-source mutations and requires the corresponding theorem to fail. It also replays the original butterfly certificate against a well-typed addition substituted for subtraction in the exported environment. All three Osprey kernels must reject that stale certificate with a type mismatch. [Smith, radix-two butterfly](https://www.dsprelated.com/freebooks/mdft/Radix_2_FFT.html)

To inspect a production theorem locally, run `make _test_synthesis_proofs`, then choose **Notation → Open proof library** in the Workbench. Open the theorem’s `.ndjson` file in `target/proof/production-fft/` and select its `ProductionFft` declaration to see the statement and checked axioms.

Each checked proof must identify its source result, hypotheses, corresponding library operation and CI entry point. A real-arithmetic derivation establishes the ideal operation; claims about rendered samples also require a bound or equivalence for the implemented arithmetic.

## Spectral processing [SYNTH-SPECTRAL]

The spectral node analyses audio with a Hann-windowed STFT (size 64–4,096, hop size/4 or size/8), transforms each frame, and resynthesises by overlap-add. Its latency is size − 1 samples. A second, side input feeds the transforms that need two sounds. Every transform's formulas are in `spectral_math.ospml`.

- **Morphing ([SYNTH-SPECTRAL-MORPH]).** Morph carries one sound into another by an amount from 0 (the main input) to 1 (the side input), and the amount can be automated. There are three modes, and the [literature and design notes](#spectral-morphing-methods-design-and-limits-synth-spectral-morph-method) justify them. *Blend* interpolates log magnitudes, so a morph midway sounds midway, not like a crossfade. *Envelope* is cross-synthesis: one sound's spectral envelope on the other's fine structure. *Partial* matches spectral peaks between the sounds and interpolates their frequencies and amplitudes, so a morph between two pitches passes through the pitches between them. Phase stays continuous (phase-locked to each peak) through every mode. Today only a linear magnitude blend exists, with phase taken from the dominant input.
- **Pitch and formant ([SYNTH-ADVANCED-SPECTRAL-FORMANT]).** Pitch moves by a ratio from 0.25 to 4, and the spectral envelope moves by an independent formant ratio. Formant 1 keeps the formants in place.
- **FFT pitch-shift effect ([SYNTH-SPECTRAL-PITCH-EFFECT]).** The pitch shifter is an effect: it takes any audio input and outputs that audio transposed. It is played and saved like any other effect module, not as an instrument. It is controlled in musical units: **semitones** from −12 to +12 (default 0) and **cents** from −100 to +100 (default 0) for fine tuning. The ratio is 2^((semitones + cents / 100) / 12), and that formula lives in `SpectralMath.semitoneRatio`. Formant preservation is a separate on/off control with its own amount, off by default. Off, the effect is a plain shift. On, the formants stay where they were. The engine is the peak-locked phase vocoder: each spectral peak moves with its window lobe, its phase advances at the shifted frequency, and the bins around it keep their phase offset from it. Its latency is size − 1 samples and is declared, so the daw can compensate for it. Quality is guaranteed within one octave either way, and that is the range the controls offer. A shift of more than an octave is not a supported use: the transform smears transients and thins the sound there. Acceptance: on sines, plucks and a voice-like source, at ±1, ±5, ±7 and ±12 semitones and ±1, ±10 and ±50 cents, the output lands within 1 cent of the target, keeps its level within 1 dB, shows no amplitude ripple above 1 dB and keeps its transients.
- **Autotune ([SYNTH-SPECTRAL-AUTOTUNE]).** Real-time pitch correction is an effect on an audio input, such as a vocal or an instrument track. In every analysis hop it detects the fundamental the input is singing (a YIN-style difference function with a confidence threshold), picks the nearest target note and shifts the input there with the FFT pitch shifter. Formants are preserved, so a voice does not turn into a chipmunk. The target is the nearest note of a chosen key and scale (chromatic, major or minor) or, in the daw, the note the timeline plays at that moment. Controls: key, scale, **retune speed** (0 ms is instant and robotic; 50–400 ms is natural), correction **amount** (0–100%), and **humanise**, which leaves held notes slightly loose. A frame that is unvoiced or below the confidence threshold is not corrected; it passes through dry. It runs in real time: bounded work per hop and no allocation in the render path ([SYNTH-RENDER]). Its latency is one analysis window and is declared. The formulas (difference function, parabolic peak, note snapping, ratio, glide) belong in native Osprey. Acceptance: a sung or synthetic vibrato line 30 cents flat lands within 5 cents of the target at speed 0. Retune speed sets the glide time within 10%. Unvoiced noise passes unchanged. Correction holds across arbitrary block splits.
- **Freeze, band, shift and smear.** Freeze holds a frame and keeps rotating its phases. Band passes a frequency range, shift moves bins with hop-coherent phase, and smear smooths magnitudes over time. Stateless steps can share one analysis in a series of up to eight.

## Formula melodies [SYNTH-FORMULA-NOTES]

A formula melody is a note source like the MIDI note component: no inputs, one `Events/Notes` output. Its notes come from a formula the user types. The formula is evaluated once per step, with `t` set to the step number (0, 1, 2 and so on). Each value becomes one MIDI note. `Components.formulaNotes(identity, formula, velocity, stepFrames, gateFrames, steps)` builds one, and `Components.formulaMelody(identity, formula)` uses Perception's defaults: 16 steps of 6,000 frames (125 ms at 48 kHz), each held for 4,800 frames, at velocity 0.8. The formula is part of the processor (`FormulaNotes { formula }` in [graph.td](../../../src/osprey/packages/synth/graph.td)), and the formula's syntax tree is declared in [formula.td](../../../src/osprey/packages/synth/formula.td).

| Formula | Notes for t = 0, 1, 2, 3 |
| --- | --- |
| `60 + t` | 60, 61, 62, 63 (C4 rising by semitones) |
| `60 + t * 7 % 12` | 60, 67, 62, 69 (fifths folded into one octave) |
| `48 + t % 4 * 12` | 48, 60, 72, 84 (an arpeggio in octaves) |
| `60 + 12 * sin(t * 3.14159 / 2)` | 60, 72, 60, 48 |
| `60 / (t - 1)` | 0, rest, 60, 30 |

The formula language has these parts:

- **Values:** decimal numbers and the step `t`.
- **Operators**, from loosest to tightest binding:
  - `+` and `-`.
  - `*`, `/` and `%`.
  - Unary `-`.
  - `^`, which is right-associative, so `-2^2` is −4 and `2^3^2` is 2⁹.
- **Parentheses** for grouping.
- **Functions:** `sin`, `cos`, `abs`, `floor`, `round` and `sqrt`.
- **Remainder:** `%` is floored, so its result takes the sign of the divisor: `(t - 5) % 12` gives 7, 8, 9, 10, 11.

Names are case-sensitive and a formula is 1 to 256 characters. `Formulas.parse` returns the syntax tree or an `InvalidConfiguration` fault. The fault names the first column it cannot read, for example `Formula: unexpected ')' at column 6.`

Evaluation is total and performs no `Arith` request. Division uses IEEE `floatDiv`, and `pow`, `sqrt` and the trigonometric functions follow IEEE rules.

Each step's value becomes a note as follows:

- A value that is not finite, such as a division by zero, a remainder by zero or the root of a negative number, is a **rest**.
- A value farther than 1,000,000 from zero is also a rest.
- Any other value is rounded to the nearest whole pitch.
- A pitch above 127 or below 0 is moved by whole octaves into 0–127, so it keeps its pitch class.

Step `n` sends its note-on at frame `n × stepFrames`. Its note-off follows `gateFrames` later; a gate longer than the step is cut to the step. When one step's note-off and the next step's note-on fall on the same frame, the note-off comes first, so a repeated pitch re-attacks. `MidiClips.timeline` windows formula notes exactly as it windows a single MIDI note.

The parameter limits are:

| Parameter | Range | Whole numbers only |
| --- | --- | --- |
| `steps` | 1–256 | yes |
| `stepFrames` | 16–65,535 | yes |
| `gateFrames` | 1–65,535 | yes |
| `velocity` | 0–1 | no |

A missing parameter or a value outside its limits is an error, and so is a formula that does not parse.


## Sources and experiments [SYNTH-RESEARCH]

Research gathered on 2026-11-14 for the independent `@osprey/synth` toolkit. Light Orchestra and Perception are consumers, not the architecture. The [library contract](Synth.md) separates delivered functionality from proposed capabilities. This is a curated starting collection, not a systematic review or a claim of novelty.

Entries link to primary papers, author publications, institutional records or official engineering documentation. Summaries describe the cited work; **Osprey experiments** are our proposed applications, not claims made by those authors. Publication dates below come from the work or its bibliographic record, rather than search-engine crawl dates. Full papers and third-party implementations retain their own licences; this collection contains links and brief annotations, not redistributed paper PDFs.

### Reading path [SYNTH-RESEARCH-PATH]

Start with Puckette for signals and synthesis, Chowning for modulation, Välimäki/Pekonen/Nam for oscillator aliasing, and Zavalishin for stateful filters. Then study waveshaping antialiasing, spectral and physical models. Read DDSP alongside its review and newer parameter-discovery work to distinguish useful learnable controls from expensive waveform generation. Keep real-time callback constraints beside every algorithm.

For morphing between two analysed sounds, the [spectral morphing section](#spectral-morphing-methods-design-and-limits-synth-spectral-morph-method) compares the literature's methods and justifies the Blend, Envelope and Partial modes [SYNTH-SPECTRAL-MORPH].

The architectural opportunity is a common graph and parameter language that supports hand-designed, physically modelled and learned instruments, with interchangeable offline/reference and real-time renderers. This is a design hypothesis to test, not an established research breakthrough.

### Signals, spectra and foundational synthesis [SYNTH-RESEARCH-FOUNDATIONS]

#### Puckette — The Theory and Technique of Electronic Music (2007)

[Author's online book, HTML and PDF](https://msp.ucsd.edu/techniques.htm). A foundation for digital oscillators, amplitude control, modulation, delay-based processing and Fourier techniques. Use the author's worked material to establish units, phase conventions and analytical reference cases before optimising kernels. **Osprey experiment:** render a fixed patch in one block and many irregular blocks; compare samples and final state. This is a book, not a new synthesis paper.

#### Chowning — The Synthesis of Complex Audio Spectra by Means of Frequency Modulation (1973)

[Journal of the Audio Engineering Society 21(7), 526–534](https://aes.org/publications/elibrary/elibrary-page/?id=1954). Demonstrates economical control over evolving spectra through modulation. **Osprey experiment:** a composable FM/phase-modulation graph with explicit modulation units; measure sidebands and aliasing as index and carrier/modulator ratio change. Publisher bibliographic record and abstract verified; full-text access may depend on access rights.

#### Moorer — The Synthesis of Complex Audio Spectra by Means of Discrete Summation Formulas (1976)

[JAES 24(9), 717–727](https://aes.org/publications/elibrary/elibrary-page/?id=2590). Discrete summation gives economical spectral construction with explicit partial-count control. **Osprey experiment:** compare bounded additive banks with closed-form evaluation across near-singular parameters and Nyquist boundaries. Bibliographic record/abstract verified; obtain the full derivation before implementing it.

#### Serra and Smith — Spectral Modeling Synthesis (1990)

[Author institution's paper record](https://mtg.upf.edu/node/251) and [MTG's publications and tools](https://www.upf.edu/web/mtg/sms-tools). Models a sound through time-varying sinusoidal components plus a stochastic residual. **Osprey experiment:** additive partial banks and filtered noise as independently routable nodes; preserve phase continuity while changing pitch and spectral envelopes. Transient handling and analysis errors need separate evaluation.

### Oscillators, filters and nonlinearities [SYNTH-RESEARCH-ANTIALIASING]

#### Välimäki, Pekonen and Nam — Perceptually informed synthesis of bandlimited classical waveforms using integrated polynomial interpolation (2012)

[Institutional record, JASA 131(1), 974–986](https://research.aalto.fi/fi/publications/perceptually-informed-synthesis-of-bandlimited-classical-waveform/). A key source for polynomial-based alias reduction in classical waveforms. Read alongside [Pekonen's doctoral thesis, Filter-Based Oscillator Algorithms for Virtual Analog Synthesis](https://research.aalto.fi/en/publications/filter-based-oscillator-algorithms-for-virtual-analog-synthesis/), which describes bandlimited impulse/step approaches and their approximations. **Osprey experiment:** compare naive, polynomial-corrected and high-rate reference oscillators under static pitch, PWM, hard sync and modulation; report residual alias energy and CPU separately. A polynomial correction is not a universal alias-free guarantee.

#### Zavalishin — The Art of VA Filter Design, revision 2.1.2 (2020)

[Author's book hosted by Native Instruments](https://www.native-instruments.com/fileadmin/ni_media/downloads/pdf/VAFilterDesign_2.1.2.pdf). Reference for topology-preserving transforms, integrators and feedback in virtual-analogue filters. **Osprey experiment:** an actual stateful filter with explicit integrator state, checked cutoff/resonance, modulation trajectories and impulse/step tests. Perception's existing scalar attenuation at the fundamental is not a stateful low-pass filter and cannot substitute for one.

#### Parker, Zavalishin and Le Bivic — Reducing the aliasing of nonlinear waveshaping using continuous-time convolution (DAFx 2016)

[Conference paper, PDF](https://www.dafx.de/paper-archive/2016/dafxpapers/20-DAFx-16_paper_41-PN.pdf). Develops antiderivative-based treatment of nonlinear waveshaping. **Osprey experiment:** compare direct waveshaping, oversampling and ADAA on a common input, with matched level and latency; test nearly equal adjacent input values and transitions through zero. Do not assume a memoryless technique transfers unchanged into a feedback loop.

#### Holters — Combined Derivative/Antiderivative Antialiasing (DAFx 2022)

[Conference paper, PDF](https://www.dafx.de/paper-archive/2022/papers/DAFx20in22_paper_7.pdf). Investigates derivative samples combined with ADAA. The abstract explicitly reports that the method compares unfavourably with oversampled ADAA. **Osprey experiment:** retain negative-result baselines; compare quality per cost rather than selecting an algorithm because it is newer.

#### Smart — Wave Pulse Phase Modulation: Hybridising Phase Modulation and Phase Distortion (DAFx 2025)

[Conference paper, PDF](https://www.dafx.de/paper-archive/2025/DAFx25_paper_48.pdf). Introduces WPPM, combining phase-shaping approaches into a synthesis technique. **Osprey experiment:** represent phase transformation as a component, audition its range and stress its discontinuities against oversampled references. Include it as a research candidate after basic phase/modulation contracts are stable; publication does not establish our implementation's real-time or aliasing performance.

### Grains and physical models [SYNTH-RESEARCH-MODELS]

#### Truax — Granular Synthesis (author's overview)

[Simon Fraser University author page](https://www.sfu.ca/~truax/gran.html). Explains dense collections of short grains and the author's real-time implementation history. **Osprey experiment:** fixed-capacity grain pools with explicit windows, seeded randomness, pitch/time controls and a defined overload policy. Validate window sums and density-dependent level, not just whether a cloud sounds interesting. This source is an overview, not a newly published paper.

#### Roads — From grains to forms (author-hosted article)

[UCSB author PDF](https://clang.mat.ucsb.edu/clang/articles_files/From%20grains%20to%20forms.pdf). Connects granular techniques with larger musical structures. **Osprey experiment:** expose grain distributions and reproducible event schedules as composable control signals rather than one opaque texture preset. Exact publication metadata should be taken from the full text before a formal bibliography citation; the author-hosted document is the stable reading link.

#### Smith — A Basic Introduction to Digital Waveguide Synthesis

[Author's paper hosted in university course materials](https://courses.cs.washington.edu/courses/cse467/05wi/pdfs/swgt.pdf). Explains digital-waveguide modelling of musical systems. **Osprey experiment:** fractional delay, loss filters, excitation and resonators as separate components, with measured tuning and decay. The broader [Physical Audio Signal Processing book](https://ccrma.stanford.edu/~jos/pasp/) is supplementary reading; the Stanford page blocked automated retrieval during this collection, so its contents were not freshly verified.

#### Bilbao — Numerical Sound Synthesis: Finite Difference Schemes and Simulation in Musical Acoustics (2009)

[University bibliographic record](https://www.research.ed.ac.uk/en/publications/numerical-sound-synthesis-finite-difference-schemes-and-simulatio/) and [author's companion page](https://www2.ph.ed.ac.uk/~sbilbao/nss.html). A physical-modelling foundation distinct from simple oscillator emulation. **Osprey experiment:** modal and finite-difference resonators with stated discretisation limits, energy/decay checks and resource budgets. Stability restrictions belong in preparation-time validation, not an undocumented preset convention.

### Differentiable and learned synthesis [SYNTH-RESEARCH-LEARNED]

#### Engel, Hantrakul, Gu and Roberts — DDSP: Differentiable Digital Signal Processing (ICLR 2020)

[Paper](https://arxiv.org/abs/2001.04643) and [author project explanation](https://magenta.tensorflow.org/ddsp). Combines interpretable DSP elements with learned controls. **Osprey experiment:** keep synthesis operators and parameter trajectories explicit, with an offline differentiable counterpart and parity tests against the production renderer. A differentiable model is not automatically causal, low latency or safe for an audio callback.

#### Shan, Hantrakul, Chen, Avent and Trevelyan — Differentiable Wavetable Synthesis (2021 preprint)

[Paper](https://arxiv.org/abs/2111.10003) and [author audio supplement](https://lamtharnhantrakul.github.io/diffwts.github.io/). Learns a compact dictionary of periodic waveforms. **Osprey experiment:** offline table learning and validation, immutable playback assets, spectral mip levels and reproducible table morphing. Learning a waveform does not remove interpolation or pitch-dependent aliasing requirements.

#### Caillon and Esling — RAVE: A variational autoencoder for fast and high-quality neural audio synthesis (2021 preprint)

[Paper](https://arxiv.org/abs/2111.05011) and [IRCAM project](https://acids-ircam.github.io/RAVE/). Uses a learned latent representation and multiband waveform processing for efficient neural synthesis. **Osprey experiment:** a separately benchmarked neural node with explicit history, latency, memory and supported block sizes. The authors' reported speed on their setup is not an Osprey performance result; throughput alone does not establish interactive latency.

#### Hayes, Shier, Fazekas, McPherson and Saitis — A review of differentiable digital signal processing for music and speech synthesis (2024)

[Author-institution PDF](https://comma.eecs.qmul.ac.uk/assets/pdf/Hayes_DDSP_Review.pdf), DOI `10.3389/frsip.2023.1284100`; [2023 preprint](https://arxiv.org/abs/2308.15422). Surveys operators, applications and optimisation/robustness difficulties. **Osprey experiment:** use this to choose tractable targets and test gradient behaviour, parameter identifiability and out-of-distribution input, not just reconstruction on training examples.

#### Yutani, Yamamoto, Nakatani and Terasawa — Wavetable Synthesis Using CVAE for Timbre Control Based on Semantic Label (APSIPA ASC 2024)

[Paper/preprint](https://arxiv.org/abs/2410.18628). Connects semantic timbre labels with wavetable generation through a conditional VAE. **Osprey experiment:** optional semantic control that produces ordinary inspectable parameters/assets; retain manual control and evaluate ambiguity across listeners and instruments. It belongs outside the required sample-rendering path.

#### Mitcheltree, Tan and Reiss — Modulation Discovery with Differentiable Digital Signal Processing (WASPAA 2025)

[Paper/preprint](https://arxiv.org/abs/2510.06204). Estimates structured modulation rather than only opaque framewise controls; the authors provide audio, code and plug-ins. **Osprey experiment:** infer editable envelopes/LFO routings from sound, then render them through the same deterministic graph used for hand-authored patches. Measure the trade-off between interpretable controls and matching accuracy.

#### Takemoto, Nakamura and Saruwatari — DDSP Mixture Model-Guided Diffusion for Synthesis Parameter Estimation from Harmonic Sound Mixtures (2026)

[September 2026 preprint](https://arxiv.org/abs/2609.14427), whose record reports acceptance at APSIPA ASC 2026. Adds temporal plausibility to synthesis-parameter estimation from mixtures through diffusion-guided estimation. **Osprey experiment:** offline extraction of editable ensemble parts. This is a recent analysis/estimation candidate, not evidence that diffusion belongs in a low-latency instrument callback. Full independent reproduction is pending.

### Engineering references [SYNTH-RESEARCH-ENGINEERING]

[PortAudio's callback guidance](https://portaudio.com/docs/v19-doxydocs/writing_a_callback.html) identifies allocation, I/O, mutexes and other unbounded operations as callback hazards. This informs the real-time boundary even if a platform uses another audio API. Preparing graphs and publishing Signals must not introduce these operations into the renderer.

[Steinberg's official VST3 SDK](https://github.com/steinbergmedia/vst3sdk) is the future **hosting** reference: loading existing plug-ins, audio/event buses, parameter automation, state and latency. [CLAP's official API](https://github.com/free-audio/clap) is a useful comparison for plug-in boundary design. Neither adapter is implemented by collecting these references, and VST3 hosting is explicitly deferred from the first library extraction.

### Research-to-delivery experiments [SYNTH-RESEARCH-EXPERIMENTS]

| Track | Candidate deliverable | Evidence required before adoption |
| --- | --- | --- |
| Oscillators | Corrected periodic sources, tables, modulation and sync | Frequency/phase tests, pitch sweeps, coherent spectral measurements against a higher-quality reference, alias/CPU trade-off |
| Stateful DSP | Envelopes, smoothing, filters and delay | Exact state continuity across block splits; impulse/step responses; reset, retrigger, rate-change and extreme-parameter behaviour |
| Nonlinear DSP | ADAA/oversampled waveshaping and feedback models | Level/latency-matched comparisons, near-equal inputs, bounded solver iterations, finite output and measured residual aliasing |
| Graph engine | Typed rates/channels/ports, explicit feedback, prepared schedule | Invalid-route rejection, deterministic event order, buffer lifetime proof/checks, allocation and lock instrumentation |
| Physical/granular | Resonators, excitation and grain clouds | Stable energy/decay, tuning, seeded replay, fixed capacity and overload policy |
| Learned control | Parameter fitting, learned tables and interpretable modulation | Held-out evaluation, manual editability, model/data provenance and offline/production parity |
| Neural audio | Optional learned waveform node | Actual target-device callback deadlines, latency, memory and degradation behaviour |
| Signals/UI bridge | Observable parameters, voices, meters and diagnostics | Consistent revisions, bounded publication, no callback blocking, detached/slow subscriber tests |
| VST3 hosting | Third-party plug-in graph nodes | Host lifecycle tests, sample-offset events, latency compensation, state restoration and process-failure handling |

For each experiment retain its source revision, dataset/patch inputs, sample rate, block schedule, compiler flags, hardware, audio output and result tables. Pair objective measurements with level-matched listening comparisons. An attractive demo or a paper's headline result is insufficient to label a component production-ready.

## Spectral morphing: methods, design and limits [SYNTH-SPECTRAL-MORPH-METHOD]

The literature behind the three modes of `SpectralMorph { amount mode }` ([SYNTH-SPECTRAL-MORPH]): *Blend* (log-magnitude interpolation), *Envelope* (cross-synthesis) and *Partial* (matched-peak interpolation). Summaries describe the cited work; the design is ours, not a claim by those authors. Bibliographic details marked *unverified* could not be re-checked against a publisher's record and must be confirmed before formal citation.

### The literature

**Magnitude domain.** Linear magnitude interpolation is dominated by the louder input, so a midway setting sounds like a crossfade rather than a timbre between. Interpolating magnitudes in a logarithmic (dB) domain — the geometric mean at the midpoint — equalises the two contributions; this is standard practice in timbre morphing (Tellman, Haken and Holloway 1995; Slaney, Covell and Buckhet 1996) and follows from loudness being roughly logarithmic in amplitude. The cost is a floor: blending any partial with silence leaves a ghost at half the floor depth (−60 dB for a −120 dB floor).

**Source–filter morphing.** Decompose each magnitude spectrum into a smooth spectral envelope (filter, vocal tract) and a fine-structure excitation, then interpolate the parts separately. Cepstral smoothing originates with Oppenheim (1969; with Schafer 1968), linear-predictive envelopes with Markel and Gray (1976), iterative *true envelope* estimation with Röbel (2005, DAFx; *unverified*). Cross-synthesis — one sound's envelope on another's excitation — is as old as the channel vocoder (Dudley 1939) and entered computer-music practice with Moorer's phase-vocoder survey (1978). Osprey's peak-envelope estimator is the cheapest member of this family: a bounded max-filter, no transform, but it follows resolved partial peaks rather than a smooth formant curve.

**Sinusoidal partial morphing.** McAulay and Quatieri (1986) analyse and resynthesize speech as tracked sinusoids; Serra's SMS (Serra 1989 thesis; Serra and Smith 1990) adds a stochastic residual for everything the partials miss. Tellman, Haken and Holloway (1995, the Lemur work) morph timbres by matching spectral peaks between two analyses, interpolating each matched pair's frequency linearly in Hz and its amplitude in dB, and crossfading unmatched peaks — their contribution is handling unequal peak counts. Slaney, Covell and Buckhet (1996) automate morphing by aligning features and interpolating in a perceptual domain. Bonada, Loscos, Cano, Serra and de Boer (ICMC 2001; *unverified*) apply SMS-style morphing to singing voice. The Loris package of Fitz and Haken continues this line (software; no single paper verified).

**Phase.** The phase vocoder (Flanagan and Golden 1966; Dolson's 1986 tutorial) accumulates synthesis phase from measured instantaneous frequency; naive magnitude morphs carrying either input's phase sound phasy and beat. Phase locking — every bin under a peak shares that peak's rotation (Puckette 1995; Laroche and Dolson 1999 identity locking) — keeps a partial's bins coherent; Osprey's `locked` already implements this. Transients are the known casualty of phase-vocoder manipulation; Verma and Meng (ICMC 2000; *unverified*) model them explicitly, and SMS keeps them in the residual.

**Automation and perception.** An automated morph amount changes the modified STFT frame by frame, and overlap-add then reconstructs a signal whose frames are mutually inconsistent (Griffin and Lim 1984), so fast sweeps ripple. Perceptually, listeners weigh spectral envelope and pitch heavily — the motivation for Slaney et al.'s alignment — and a morph is convincing when prominent features *move* continuously rather than crossfade.

### Methods compared

| Method | Interpolates | Phase | Strengths | Weaknesses |
| --- | --- | --- | --- | --- |
| Linear magnitude blend (current) | amplitude, linearly | dominant input | trivial, stateless | midway ≈ the louder input; crossfade character |
| dB magnitude blend | amplitude, geometrically | dominant input, locked | midway is perceptually midway; stateless | floor ghosting; partials stay put |
| Source–filter morph (cepstrum/LPC/true envelope) | envelope and excitation separately | dominant input, locked | independent formant and excitation trajectories | estimator cost/bias; envelope–pitch confusion |
| Peak-envelope cross-synthesis | envelope of A onto excitation of B, by amount | side input, locked | classic vocoder timbre; reuses `envelope` | degenerates when f0 exceeds the envelope radius |
| MQ/SMS-style partial morph | matched-peak frequency (Hz) and amplitude (dB) | dominant pair member, locked | partials genuinely travel between pitches | needs peak matching; noise and transients unmodelled |
| Automatic morphing (Slaney et al.) | aligned features, perceptual scale | analysis-driven | hands-free | complex; offline in practice |

### Built design

Three modes on `SpectralMorph { amount mode }` (81552760a), all inside the existing STFT node, with formulas in `spectral_math.ospml`. Every mode keeps one contract: amount 0 is the main input and 1 is the side.

**Blend** (`logBlend`). Per bin, magnitude is `exp ((1 − t) ln (a + ε) + t ln (b + ε)) − ε` with ε = 10⁻⁹, the geometric path. Phase comes from the main input up to amount 0.4 and the side input from 0.6, and between them it turns along the shorter arc, weighted by the two levels so a silent bin never donates its phase (`donorWeight`, `arcPhase`). Partials stay at their own frequencies, so this is a morph in timbre, not pitch. A sine at 0.5 blended with one at 0.05 measures 0.158 halfway (a crossfade would give 0.275), and 0.281 and 0.089 at the quarters, all within 5%.

**Envelope** (`envelopeMorph`). Cross-synthesis keeps the main input's harmonics and phase, and rescales each bin by `(E_side / E_main) ^ t`. Both envelopes use the formant shifter's peak envelope (about 150 Hz), the gain is capped at 16, and a silent own-envelope gives a gain of 1. Unlike the cross-synthesis row above, which makes the side input the carrier, the main input stays the carrier, so 0 is the main input as in every other mode. A vowel with a 1 kHz resonance takes on the side input's 1.875 kHz resonance at amount 1, with no energy between its harmonics.

**Partial** (`SpectralPartials`). Peaks within 26 dB (5%) of the frame's loudest bin are kept. The threshold sits above the Hann window's −31 dB sidelobes, so a sidelobe is never matched as a partial. Each peak is centred by parabolic interpolation of its log levels (`peakOffset`, `logLevel`). Main and side peaks pair in frequency order, so harmonic k pairs with harmonic k. A pair glides in *log* frequency and log level (`logBlend`). The geometric midpoint between two pitches is the musical one (220 → 330 Hz at 0.5 is 269.4 Hz, the equal-tempered midpoint; linear Hz would give 275). An unpaired peak fades where it stands. Each glided peak's five-bin lobe is redrawn with the centre's accumulated synthesis phase, and each neighbour sits π further on (`lobePhase`), as a Hann-windowed sinusoid's bins do. Without that phase lock the output level fell to 0.32. With it, a unit fundamental comes out at 0.9997, and the 1/2 and 1/3 harmonics at 0.4996 and 0.333. Partial morphing is stateful because it carries synthesis phase, so a spectral series refuses it.

Tests (`wave_journeys.ospml`): Blend at five amounts. Envelope at three amounts, with both resonances checked. Partial over ten glides: rising, falling, fifths, an octave and amounts from 0 to 1. Each harmonic k = 1–3 must land at k × the glide with level 1/k within 20%, and each source pitch must sit 30 dB below it.

### Validation

The acceptance test renders at 48 kHz, size 4,096 and hop 1,024, and measures from hop 8, past the 4,095-frame latency and one full window.

| Test | Acceptance |
| --- | --- |
| Steadiness: amount held at 0.5 on 220 Hz and 330 Hz six-harmonic tones in every mode, and on a 500 Hz sine against one 20 dB quieter in Blend | every hop's rms within 0.5 dB of the mean |
| Automation: amount ramps 0 → 1 over 0.5 s, in Blend and Partial with a 20 dB level step, in Envelope on the harmonic tones, and in Blend on a 500 Hz sine against itself 90° and 160° out of phase | no hop-to-hop step more than 2 dB from the ramp's mean step; the total change within 1 dB of the level step |
| Partial orphans: 220 Hz against 277 Hz at amount 0.25, with six main and three side harmonics, then three main and six side | the three pairs at k × the geometric glide within 20%; each unmatched harmonic at 0.75 (main) or 0.25 (side) of its level within 1 dB |

### Failure modes

- **Floor ghosting** (Blend, Partial amplitudes): a partial blended with silence leaves a −floor/2 dB image (−60 dB at 0.5). Keep the floor at −120 dB and never raise true silence above it; the bound is measured, not hidden.
- **Envelope radius bias** (Envelope): fundamentals above ≈150 Hz, or partials spaced wider than the radius, collapse the peak envelope onto the partial peaks, and Envelope mode degenerates toward Blend. Cepstral or true-envelope smoothing (Oppenheim 1969; Röbel 2005) is the upgrade at FFT cost.
- **Match churn** (Partial): inharmonic or noisy spectra give different order-based pairings each frame → warble. No cross-frame tracking in v1; MQ-style tracking is the literature fix. Peak-count and magnitude thresholds bound the damage.
- **Transients and noise** (Partial): matched peaks cannot represent attacks or breathiness (the SMS residual; Verma and Meng), so percussive material smears over the window; Blend is the fallback mode.
- **Mid-amount phase incoherence** (Blend): bins inherited from one input carry a phase turned toward the other's, so partial cancellation can occur; identity locking mitigates it within one peak, not across inputs.
- **Exact antiphase** (Blend): a bin whose two inputs are exactly π apart has no shorter arc, so its turn can flip direction between frames and a sweep dips there (4 dB on a sine against its own inverse). Any other offset turns smoothly: the hard switch at 0.5 it replaced dipped 2.4 dB at 90° and 8 dB at 180°, and the crossfade keeps 90° and 160° within 0.6 dB.
- **Overlap-add inconsistency under fast automation** (all modes): frame-to-frame changes make the modified STFT inconsistent (Griffin and Lim 1984); bound amount sweeps to ≲ one per window.

### References

Bibliographic records; links only where a stable public source is known. Entries without links were cited from the bibliographic record and not re-verified online.

- Flanagan, J. L. and Golden, R. M., *Phase Vocoder*, Bell System Technical Journal 45(9), 1966.
- Dolson, M., *The Phase Vocoder: A Tutorial*, Computer Music Journal 10(4), 1986.
- Puckette, M., *Phase-Locked Vocoder*, ICMC 1995; and the author's [book](https://msp.ucsd.edu/techniques.htm) (2007).
- Laroche, J. and Dolson, M., *New Phase-Vocoder Techniques for Pitch-Shifting, Harmonizing and Other Exotic Effects*, IEEE WASPAA 1999.
- Allen, J. B. and Rabiner, L. R., *A Unified Approach to Short-Time Fourier Analysis and Synthesis*, Proceedings of the IEEE 65(11), 1977.
- Griffin, D. W. and Lim, J. S., *Signal Estimation from Modified Short-Time Fourier Transform*, IEEE TASSP 32(2), 1984.
- Dudley, H., *The Vocoder*, Bell Laboratories Record 17, 1939 (*pages unverified*).
- Oppenheim, A. V. and Schafer, R. W., *Homomorphic Analysis of Speech*, IEEE Transactions on Audio and Electroacoustics AU-16(2), 1968; Oppenheim, *Speech Analysis–Synthesis System Based on Homomorphic Filtering*, JASA 45(2), 1969.
- Markel, J. D. and Gray, A. H., *Linear Prediction of Speech*, Springer, 1976.
- Moorer, J. A., *The Use of the Phase Vocoder in Computer Music Applications*, JAES 26(1/2), 1978.
- McAulay, R. J. and Quatieri, T. F., *Speech Analysis/Synthesis Based on a Sinusoidal Representation*, IEEE TASSP 34(4), 1986.
- Serra, X., *A System for Sound Analysis/Transformation/Synthesis based on a Deterministic plus Stochastic Decomposition*, PhD thesis, Stanford, 1989; Serra, X. and Smith, J. O., *Spectral Modeling Synthesis*, Computer Music Journal 14(4), 1990 ([MTG record](https://mtg.upf.edu/node/251), [SMS tools](https://www.upf.edu/web/mtg/sms-tools)).
- Tellman, E., Haken, L. and Holloway, B., *Timbre Morphing of Sounds with Unequal Numbers of Features*, JAES 43(11), 1995 (*issue/pages unverified*).
- Slaney, M., Covell, M. and Buckhet, B., *Automatic Audio Morphing*, IEEE ICASSP 1996 (*record only*).
- Bonada, J., Loscos, A., Cano, P., Serra, X. and de Boer, M., *Singing Voice Morphing Based on Spectral Synthesis Techniques*, ICMC 2001 (*unverified*).
- Röbel, A., *Efficient Spectral Envelope Estimation and its Application to Pitch Shifting and Envelope Preservation*, DAFx 2005 (*unverified*).
- Verma, T. S. and Meng, T. H. Y., *Transient Modeling Synthesis: A Flexible Analysis/Synthesis Tool for Transient Signals*, ICMC 2000 (*unverified*).

## VST3 hosting [SYNTH-HOST-VST]

VST3 **hosting** is an active requirement of the [Perception expansion](../../plans/synth/prism.md). The isolated worker now scans, describes and processes real plug-ins, and Perception can create and route their typed graph nodes, then audition a real instrument/effect chain as stereo PCM. Continuous device rendering remains in progress. The purpose is to load existing VST3 instruments/effects into a synthesis graph. Producing Osprey VST3 plug-ins is a separate capability and is not substituted for hosting.

An external node declares audio buses, event buses, parameter identifiers, sample-accurate automation support, latency, tail length and saved state. Host adapters map these to the same graph ports and parameter Signals as built-in components. Plug-in metadata/parameter IDs remain stable across graph saves; missing binaries or changed layouts produce visible errors rather than loading a different plug-in silently.

Scanning, loading, activation and state restoration run outside the callback. The design must account for unsupported layouts, dynamic latency, bypass, crash/hang handling and untrusted plug-in code. Scan out of process; evaluate process isolation for playback with explicit latency/cost measurements. No claim that an arbitrary third-party plug-in respects Osprey's memory-safety or callback contract. Host integration includes actual DAW-style lifecycle and state/automation tests. Use the [official SDK](https://github.com/steinbergmedia/vst3sdk) as the compatibility source.

## Prepared grain samples [SYNTH-SAMPLES-IMPORT]

`SampleResources.decodeWave(name, rootNote, bytes)` is a pure preparation API returning an immutable `SampleResource` or an explicit error. It preserves source sample rate and original channel count, converts the caller's root MIDI pitch to a root frequency and validates finite normalised samples. The bounded WAV parser supports little-endian RIFF PCM 8/16/24/32 and float32, one or two channels, 8–192 kHz, 2–1048576 frames and at most 8 MiB. The returned buffer holds interleaved frames in their original channel order. The mono sampler and grain kernels use an equal-weight stereo mix. Resource identity includes both original sample words. Unknown chunks are skipped within checked lengths and padding, duplicate format/data chunks and truncated data are rejected, and compressed/extensible/RF64/RIFX formats are unsupported. This allocating reference importer runs outside rendering and has no filesystem or UI dependency.

`Components.granular` takes the resource rather than an unlabelled sample list. Its default playback rate compensates for the source rate at the graph's current 48 kHz output rate. Transposition uses source/output sample-rate ratio multiplied by target/root frequency ratio, independent of file length. `SampleResources.playbackRate` returns an explicit failure when the requested transposition exceeds the supported grain rate. The built-in periodic source declares its exact 46.875 Hz root. Resource identity/content hashes, persistence, asynchronous file selection and live swaps remain unimplemented; compile/static evidence does not establish numerical import correctness or realtime readiness. See [SYNTH-ADVANCED-GRAINS-IMPORT].

## Built-in oscillator MIDI input [SYNTH-HOST-MIDI-OSCILLATOR]

An oscillator node may declare an optional mono `Events/Notes` input; the built-in `Components.sine` factory stays input-free and an unwired oscillator uses its frequency parameter. In the shared block graph renderer, a wired oscillator is initially silent and consumes note events before their sample offset: positive-velocity note-on selects pitch and amplitude, while matching note-off or zero-velocity note-on closes the gate. Pitch uses equal temperament with A4 at 440 Hz. The reference implementation is monophonic, last-note priority without fallback to previously held notes. Note ownership includes channel, pitch and event identity; an unrelated note-off cannot silence the selected note. Oscillator phase and note ownership persist between blocks. Matching note-off starts a retained linear release of a fixed 240 frames (5 ms at 48 kHz); there is no public per-voice duration control yet. The first release sample retains full velocity and the last is exactly zero; release progresses across blocks. Repeated note-offs do not restart it, and unrelated note-offs leave it unchanged. A new positive-velocity note-on cancels the tail, retains oscillator phase and adopts the new note; there is no attack/crossfade or polyphonic fallback, so this is not a fully click-free voice implementation. Perception currently applies parameter edits after playback restart. Silent voices retain rather than advance their oscillator phase.

The scalar reference renderer accepts unwired oscillators but explicitly rejects connected event graphs; use `GraphClips` for MIDI rendering. Switching between wired and unwired modes requires a freshly prepared graph state; live topology state migration remains a separate routing requirement. An earlier build exposed a cyan MIDI socket on the original oscillator card and routed the existing MIDI note generator to it; manual native interaction connected `5:0 → 0:0` and completed an audition, and the device receipt reported four empty-queue callbacks, so sustained reliable playback is not established. Strict compilation and the native build passed; automated verification remains paused.

## Worker audio transport [SYNTH-HOST-STREAM]

`HostPackets` encodes checked interleaved float32 audio, MIDI and float64 normalized automation into the worker's little-endian protocol. Clips are split into at most 256-frame blocks; event offsets remain sample-accurate. The transport bounds channels, frame counts, notes and parameters before encoding. `HostResponses` checks the exact expected channel layout and frame count, finite samples, stable bounded latency and the absence of trailing data before exposing audio to the graph. Files and packets share `PcmWords`, and `WaveFiles.interleaved` records channel-aligned IEEE float32 PCM with a frame-count fact chunk.

The native worker accepts binary persistent streams or bounded hexadecimal clips over stdin/stdout. The text path has exact-PCM parity with binary processing. It is suitable for reference previews; it is not a realtime device callback implementation. The GPUI process-input adapter bounds transport memory and child lifetime independently of output EOF.

## Hosted graph clips [SYNTH-HOST-GRAPH]

`GraphClips` evaluates a frozen acyclic graph in dependency order, sharing the reference oscillator, filter, gain, mixer and delay kernels through `ClipKernels`. A hosted node suspends pure execution with its checked audio, MIDI and parameter request; the consuming application supplies a validated worker response before scheduling downstream nodes. Extra ports, missing cables, malformed cached buffers and excessive resources are errors. Explicit mono-to-stereo conversion is a component; a cable never changes channel count. This bounded offline path rejects feedback and nonzero plug-in latency rather than claiming realtime feedback or latency compensation.

## MIDI generators [SYNTH-HOST-MIDI]

A MIDI note component exposes whole-number pitch, normalized velocity and an exact gate length in frames. `MidiClips.window` assigns note-on and note-off to half-open sample windows, preserving ownership at block boundaries. Fractional pitches/gates, invalid velocities and out-of-range windows return errors. Event outputs connect only to compatible event inputs; audio outputs remain separate. Sequencing, polyphonic generators and continuous live note control remain delivery work.

## Hosted parameter modulation [SYNTH-HOST-MODULATION]

`HostModulation.connect` attaches an explicitly normalized control input to an automatable VST parameter and connects it transactionally. Parameters use stable native IDs, and incompatible sources are rejected without changing the previous graph. Disconnecting the cable retains the port for later routing and restores the configured parameter value. Non-automatable controls are displayed as unavailable for editing in this host; editing them is an explicit error rather than a silent no-op.

The clip renderer samples connected control streams on a 128-frame automation grid. Default values are sent once at frame zero; connected parameters receive exact offsets and normalized values at subsequent grid points. A clip has a bounded total of 1024 points, and excess automation returns an error. This is a declared control update interval, not a promise that every plug-in interpolates identically. The native JX10 journey captures the exact filter-frequency points and compares the resulting audio against a replay with only the initial control setting retained.

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

The 2026-11-17 proposal defines package behavior and Perception UI requirements. Numerical defaults below are initial engineering choices to validate. The [source collection](#sources-neural-granular-and-physical-synthesis-synth-research-focused) supplies the sources. The [delivery plan](../../plans/synth/prism.md#advanced-synthesis-delivery-plan-prism-advanced-plan) records dependencies and acceptance evidence. For current implementation and verification status, see the [active synthesis checklist](../../plans/synth/prism.md).

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

### Acceptance contract [SYNTH-ADVANCED-VERIFY]

Extend existing package and Perception journeys. Record fixtures, model hashes, toolchain versions, tolerances and hardware. At declared rates exercise blocks of 1, 17, 64, 127, 128, 257 and 2048 frames and irregular partitions. Compare final states and PCM. Fixed-backend deterministic paths require identical event traces and relative/absolute PCM tolerance of 1e-6 against reference. Measure neural cross-backend tolerances per model.

Granular acceptance covers empty/truncated assets, overflow dimensions, nonfinite samples, one-frame sources, reverse boundaries, freeze/replace during tails, zero/max density, seeded replay, ring wrap, overload counters and cancellation. Require pitch error below 2 cents on calibrated sinusoids. Compare alias energy at transposition extremes to a high-quality reference and establish the quality threshold before promoting a resampler.

Physical acceptance covers impulse spectra, nodal cancellations, pitch/decay sweeps, solver limits, rates, extreme geometry and automation. Target isolated linear-mode frequency error below 2 cents and fitted T60 error below 5 percent. Verify the energy inequality during unforced decays and coupled transitions, including hidden state, against physical and numerical references with known assumptions.

Use genuine licensed fixtures for all four neural capabilities. Cover channel/hop adaptation, pitch confidence, manifest/tensor corruption, codebook indices, resampling, replacement, deadlines, worker loss, cancellation and stale results. Report reconstruction/spectral metrics and level-matched listening by held-out monophonic, polyphonic, transient and noise class. Mocks establish lifecycle evidence only.

Record contact traces, controller inputs, energy debits and mode trajectories for each hybrid ablation. Use held-out bodies and textures, objective onset/decay metrics and randomized loudness-matched listening. Preserve negative results. Adoption requires repeatable controllability or perceptual value within budget. Novelty claims require broader prior-art review.


## Sources: neural, granular and physical synthesis [SYNTH-RESEARCH-FOCUSED]

Collected 2026-11-17. The following annotations describe primary publications and manufacturer documentation. Proposed applications are design decisions for Osprey. Product descriptions establish documented behavior, with listening, source-level reproduction and target-device benchmarks remaining future work. Source versions are identified where available.

### Neural models and deployed tools [SYNTH-RESEARCH-NEURAL]

| Source | Technical contribution and consequence for the design |
| --- | --- |
| Caillon and Esling, *RAVE* (2021 preprint) | A variational autoencoder uses a compact latent representation, multiband synthesis and two-stage training. Expose model-specific latent controls and measure buffering, lookahead and processing separately. The paper's throughput result does not establish Perception latency. [Paper](https://arxiv.org/abs/2111.05011) |
| Engel et al., *DDSP* (ICLR 2020) | Differentiable signal-processing operators let learned models control structured synthesis. Pitch/loudness-conditioned harmonic and noise resynthesis suggests an editable timbre-transfer module. Pitch-estimation errors and polyphonic inputs need explicit evaluation. [Paper](https://arxiv.org/abs/2001.04643) |
| Défossez et al., *High Fidelity Neural Audio Compression* (2022 preprint) | EnCodec learns an encoder/decoder and quantized audio representation. Codec tokens require model-specific metadata and compatible codebooks. Reconstructing compressed audio alone supplies no text conditioning or interpretable timbre axis. [Paper](https://arxiv.org/abs/2210.13438), [implementation](https://github.com/facebookresearch/encodec) |
| Liu et al., *AudioLDM* (ICML 2023) | Text-conditioned latent diffusion generates audio using CLAP representations. Use an asynchronous asset-generation job with seed, model revision and output provenance, followed by ordinary sample loading. [Paper](https://arxiv.org/abs/2301.12503), [proceedings](https://proceedings.mlr.press/v202/liu23f.html) |
| Magenta DDSP-VST | Provides neural timbre-transfer effects and a MIDI-playable synthesizer. This motivates separate Audio and MIDI input modes, with visible pitch tracking and level control. [Developer repository](https://github.com/magenta/ddsp-vst), [official plugin catalog](https://magenta.withgoogle.com/plugins) |
| IRCAM `nn~` | Hosts exported neural models in Max and Pure Data, including RAVE models. Model methods and channel dimensions form an integration contract. Perception must obtain these from a validated manifest. [Developer repository](https://github.com/acids-ircam/nn_tilde) |
| Neutone FX | The published catalog includes DDSP instrument models and RAVE timbre-transfer models with declared input/output layouts. A model browser should expose those capabilities and provenance before insertion. [Official model catalog](https://neutone.jp/fx/models) |

### Granular instruments [SYNTH-RESEARCH-GRANULAR-PRODUCTS]

Truax's author overview describes dense clouds of short events and links his *Real-Time Granular Synthesis with a Digital Signal Processor* (Computer Music Journal 12(2), 1988). This supplies the scheduling/windowing foundation. The proposed 2–500 ms control range is a musical design choice extending beyond the overview's typical short-grain examples. [Author overview and bibliography](https://www.sfu.ca/~truax/gran.html)

| Instrument and inspected source | Documented approach | Proposed Perception application |
| --- | --- | --- |
| Pigments, manual 5.0 | Granulation is part of the Sample engine. The indexed official manual establishes that relationship. Full PDF retrieval exceeded the browser limit, so detailed control parity remains unverified. | A sample waveform with position, grain size and density in the selected module inspector. [Official manual](https://dl.arturia.net/products/pigments/manual/pigments_Manual_5_0_0_EN.pdf) |
| Portal, Grain Controls help page | Captures incoming audio into a buffer, with grain density, length, count, pitch, reverse, offset, shape and feedback. Timing controls can use rhythmic relationships. | An explicit live-input mode with capture/freeze, tempo synchronization and reproducible randomness. [Manufacturer technical guide](https://support.output.com/en/articles/10297851-grain-controls) |
| Quanta 2, manufacturer product documentation | A sample-based grain voice combines grain rate, pitch, direction, shape, length and position with synthesis sources and modulation. | Per-note grain voices and modulation destinations with visible resource limits. [Manufacturer documentation](https://www.audiodamage.com/collections/granular-synthesis/products/ad055-quanta-2) |
| Tasty Chips GR-1, manual 3.4.b | Hardware granular instrument organized around samples, scan position and direct grain controls. | Waveform scanning, frozen-position performance and direct numeric control alongside gestures. [Manufacturer manual](https://www.tastychips.nl/downloads/GR-1/UserManual/User-manual-GR-1-v3.4.b.pdf) |
| Make Noise Morphagene, manufacturer manual | Organizes material into Reels, Splices and Genes. Gene Size, Slide, Morph and Vari-Speed control microsound playback. Recording and playback can operate independently. | Region selection and grain scanning with independently specified capture, transport and pitch behavior. [Manufacturer manual](https://www.makenoisemusic.com/wp-content/uploads/2024/03/morphagene-manual.pdf) |

### Physical instruments and numerical methods [SYNTH-RESEARCH-PHYSICAL-PRODUCTS]

Bilbao et al., *Physical modeling, algorithms and sound synthesis — The NESS Project*, Computer Music Journal 43(2–3), 15–30, is recorded by Edinburgh as published in 2020. It discusses numerical simulation and the relationship between physical systems and synthesis algorithms. Use energy behavior and numerical stability as design obligations for strings, tubes, membranes and coupled bodies. The source's publication record takes precedence over search crawl dates. [Institutional full text](https://www.pure.ed.ac.uk/ws/portalfiles/portal/121715237/cmj_ness_aam.pdf)

| Instrument | Documented approach | Proposed Perception application |
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
