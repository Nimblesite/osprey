# Synthesis DSP, spectral processing and proofs

Part of the [synthesis toolkit](Synthesis.md).
## DSP quality [SYNTH-DSP]

The extracted Prism renderer is a reference/example path: 11,025 Hz, unsigned 8-bit WAV output, naive periodic waves and a fundamental-gain approximation to filtering. It is not the production engine. Production kernels require continuous phase/state, high-resolution floating-point audio, bounded envelopes, smoothing, proper stateful filters and documented antialiasing behaviour. Avoid retaining parallel versions once the production replacement is verified.

Oscillator quality is evaluated under pitch changes, pulse-width modulation, phase modulation, hard sync and negative/near-zero rates where supported. Filters are evaluated under cutoff/resonance automation and feedback. Nonlinear processors require explicit oversampling/ADAA policies and declared latency. Modulation and mixing remain composable rather than being baked into one fixed subtractive voice.

The `Grains` reference kernel [SYNTH-ADVANCED-GRAINS-PLAYBACK] implements immutable mono-source grain pools with stereo summing, periodic linear interpolation, independent lifetime/rate, symmetric Hann or configurable linear ASR windows and equal-power placement. Preparation validates source data and capacity; trigger/render operations reject invalid state and exhausted counters. Its generated models expose coherent sample position, trigger revision and active-count observations through a UI-independent Signal effect. Strict static checking has passed; automated audio verification is paused. `GrainScheduler` now supplies periodic and seeded bounded-jitter onset clocks; `Grains.scheduled` consumes ordered absolute sample events before the corresponding sample, preserving equal-position input order and rejecting events outside the current half-open block. Both clock and player state remain immutable on failure. `Components.granular` and `GrainGraph` now connect that engine to the shared stereo graph renderer with typed retained processor memory. Prism exposes Grain Cloud insertion, its existing parameter inspector, source waveform and keyboard pitch mapping. Independent distributions, resource replacement/file loading, separate scheduler/source nodes, modulation ports and realtime guarantees remain requirements in the advanced plan; manual native insertion/inspector inspection is complete, while audio and automated UI verification remain deferred.

Research candidates include bandlimited/corrected oscillators, wavetable/additive/spectral synthesis, physical/modal models, granular engines, expressive phase shaping and differentiable/learned controls. The detailed implementation and acceptance checklist is the [advanced synthesis plan](../../plans/synth/advanced-synthesis.md) [SYNTH-ADVANCED]. Adoption requires the [research experiments](#research-to-delivery-experiments-synth-research-experiments), not a novelty claim. Training and expensive patch inference remain outside the required audio callback.

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

A formula melody is a note source like the MIDI note component: no inputs, one `Events/Notes` output. Its notes come from a formula the user types. The formula is evaluated once per step, with `t` set to the step number (0, 1, 2 and so on). Each value becomes one MIDI note. `Components.formulaNotes(identity, formula, velocity, stepFrames, gateFrames, steps)` builds one, and `Components.formulaMelody(identity, formula)` uses Prism's defaults: 16 steps of 6,000 frames (125 ms at 48 kHz), each held for 4,800 frames, at velocity 0.8. The formula is part of the processor (`FormulaNotes { formula }` in [graph.td](../../../src/osprey/packages/synth/graph.td)), and the formula's syntax tree is declared in [formula.td](../../../src/osprey/packages/synth/formula.td).

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

In Prism, choose **Add component → Formula melody**. The inspector's **Formula over step t** field applies a formula when you press Return. A formula that does not parse leaves the patch unchanged, and the editor message names the column. Under the field, the inspector lists the first 16 notes the formula plays, by name, for example `Plays C4 G4 D4 A4 …`. A saved project stores the formula in the patch node's `asset` with the component `formulaNotes`. A stored formula that no longer parses refuses the track with the same fault.

## Sources and experiments [SYNTH-RESEARCH]

Research gathered on 2026-11-14 for the independent `@osprey/synth` toolkit. Light Orchestra and Prism are consumers, not the architecture. The [library contract](Synthesis.md) separates delivered functionality from proposed capabilities. This is a curated starting collection, not a systematic review or a claim of novelty.

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

[Author's book hosted by Native Instruments](https://www.native-instruments.com/fileadmin/ni_media/downloads/pdf/VAFilterDesign_2.1.2.pdf). Reference for topology-preserving transforms, integrators and feedback in virtual-analogue filters. **Osprey experiment:** an actual stateful filter with explicit integrator state, checked cutoff/resonance, modulation trajectories and impulse/step tests. Prism's existing scalar attenuation at the fundamental is not a stateful low-pass filter and cannot substitute for one.

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

**Blend** (`logBlend`). Per bin, magnitude is `exp ((1 − t) ln (a + ε) + t ln (b + ε)) − ε` with ε = 10⁻⁹, the geometric path. Phase comes from the dominant input. Partials stay at their own frequencies, so this is a morph in timbre, not pitch. A sine at 0.5 blended with one at 0.05 measures 0.158 halfway (a crossfade would give 0.275), and 0.281 and 0.089 at the quarters, all within 5%.

**Envelope** (`envelopeMorph`). Cross-synthesis keeps the main input's harmonics and phase, and rescales each bin by `(E_side / E_main) ^ t`. Both envelopes use the formant shifter's peak envelope (about 150 Hz), the gain is capped at 16, and a silent own-envelope gives a gain of 1. Unlike the cross-synthesis row above, which makes the side input the carrier, the main input stays the carrier, so 0 is the main input as in every other mode. A vowel with a 1 kHz resonance takes on the side input's 1.875 kHz resonance at amount 1, with no energy between its harmonics.

**Partial** (`SpectralPartials`). Peaks within 26 dB (5%) of the frame's loudest bin are kept. The threshold sits above the Hann window's −31 dB sidelobes, so a sidelobe is never matched as a partial. Each peak is centred by parabolic interpolation of its log levels (`peakOffset`, `logLevel`). Main and side peaks pair in frequency order, so harmonic k pairs with harmonic k. A pair glides in *log* frequency and log level (`logBlend`). The geometric midpoint between two pitches is the musical one (220 → 330 Hz at 0.5 is 269.4 Hz, the equal-tempered midpoint; linear Hz would give 275). An unpaired peak fades where it stands. Each glided peak's five-bin lobe is redrawn with the centre's accumulated synthesis phase, and each neighbour sits π further on (`lobePhase`), as a Hann-windowed sinusoid's bins do. Without that phase lock the output level fell to 0.32. With it, a unit fundamental comes out at 0.9997, and the 1/2 and 1/3 harmonics at 0.4996 and 0.333. Partial morphing is stateful because it carries synthesis phase, so a spectral series refuses it.

Tests (`wave_journeys.ospml`): Blend at five amounts. Envelope at three amounts, with both resonances checked. Partial over ten glides: rising, falling, fifths, an octave and amounts from 0 to 1. Each harmonic k = 1–3 must land at k × the glide with level 1/k within 20%, and each source pitch must sit 30 dB below it.

### Failure modes

- **Floor ghosting** (Blend, Partial amplitudes): a partial blended with silence leaves a −floor/2 dB image (−60 dB at 0.5). Keep the floor at −120 dB and never raise true silence above it; the bound is measured, not hidden.
- **Envelope radius bias** (Envelope): fundamentals above ≈150 Hz, or partials spaced wider than the radius, collapse the peak envelope onto the partial peaks, and Envelope mode degenerates toward Blend. Cepstral or true-envelope smoothing (Oppenheim 1969; Röbel 2005) is the upgrade at FFT cost.
- **Match churn** (Partial): inharmonic or noisy spectra give different order-based pairings each frame → warble. No cross-frame tracking in v1; MQ-style tracking is the literature fix. Peak-count and magnitude thresholds bound the damage.
- **Transients and noise** (Partial): matched peaks cannot represent attacks or breathiness (the SMS residual; Verma and Meng), so percussive material smears over the window; Blend is the fallback mode.
- **Mid-amount phase incoherence** (Blend): bins inherited from the non-dominant input carry the dominant's phase, so partial cancellation can occur; identity locking mitigates it within one peak, not across inputs.
- **The 0.5 phase-donor switch** (Blend): the dominant-input rule flips at amount 0.5, a phase seam when sweeping through. Envelope avoids it (main phase throughout). Crossfading donors near 0.5 is the open refinement.
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
