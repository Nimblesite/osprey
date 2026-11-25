# Advanced synthesis: methods

Part of the [advanced synthesis plan](advanced-synthesis.md).
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
- [x] Add optional pitch-synchronous scheduling as a separate mode with its own analysis and latency contract. `PitchMarks.block` accepts a bipolar reference with one rising crossing per period and needs zero lookahead. `pitchMarkJourney` verifies absolute marks at frames 29 and 33, a crossing across a block boundary, identical terminal state and invalid reference rejection under GC and ARC. [Scheduler contract](../../../src/osprey/packages/synth/README.md) · [Journey](../../../src/osprey/packages/synth/tests/grain_journeys.ospml)

### Real sample loading and ownership [SYNTH-ADVANCED-GRAINS-IMPORT]

The first import path is bounded local WAV loading, not a general codec framework. File selection and reading belong to the application; decoding and sample validation are pure library operations. No file access, decoding or allocation may be moved into the audio callback. Initial supported encodings and limits must be visible in the application and package documentation.

- [ ] Declare generated models for decoded sample metadata, prepared sample identity, channel policy and typed import errors. Partial: decoded metadata is `WaveSamples`/`WaveFormat` (`wave.td`), prepared identity is `ResourceIdentity` (`graph.td`, fb56cac8d) and import errors are typed `Fault`s (a malformed WAV is `MissingResource`). Import keeps the original interleaved lanes. A mono consumer requests an equal-weight mix. A generated channel-policy model remains open.
- [x] Define the first supported WAV encodings, channel counts, sample rates, file-byte and decoded-frame limits before implementing the decoder.
- [x] Implement bounded RIFF/WAVE chunk parsing with checked lengths, odd-byte padding, unknown-chunk handling and explicit duplicate/truncated-chunk rejection.
- [x] Validate encoding, frame alignment, byte rate, sample count and finite sample values; reject unsupported formats rather than guessing. Evidence: "WAV import refuses corrupt sizes, headers, chunks, formats and out-of-range samples" in [wave_journeys.ospml](../../../src/osprey/packages/synth/tests/wave_journeys.ospml) refuses 8 unsupported encodings/widths/channels/rates/alignments/byte rates and NaN, infinite and out-of-range floats with their exact messages; synth suite 56/56 under ARC on 21 Nov.
- [x] Define stereo handling explicitly: preserve channels where supported or perform a documented downmix; do not silently discard a channel.
- [x] Add native file selection for the selected Grain Cloud; cancellation leaves the patch unchanged. Evidence: `sampleFileJourney` in [history.ospml](../../../src/osprey/examples/daw/test/history.ospml), daw suite 24/24 on 21 Nov.
- [x] Read a bounded amount of data outside rendering and reject oversized resources before unbounded allocation or decoding. Evidence: `sampleFileJourney` in [history.ospml](../../../src/osprey/examples/daw/test/history.ospml), daw suite 24/24 on 21 Nov.
- [x] Guard asynchronous import completion with request identity and node identity/revision so an old result cannot overwrite a newer choice or resurrect a removed node. Evidence: `sampleFileJourney` in [history.ospml](../../../src/osprey/examples/daw/test/history.ospml), daw suite 24/24 on 21 Nov.
- [x] Keep the last valid sample and graph on decode, preparation or activation failure; surface a useful node-specific error. Evidence: `sampleFileJourney` in [history.ospml](../../../src/osprey/examples/daw/test/history.ospml), daw suite 24/24 on 21 Nov.
- [x] Carry source sample rate and root note into playback-rate calculation; a unity-rate sample must not be pitched according to its buffer length.
- [x] Define loop range, one-shot/reverse/end behaviour and source-position units; make the inspector match the actual supported behaviour. Region start/end (samples), boundary 0 loop/1 one-shot, signed rate for reverse; the inspector shows those descriptors with their units ("Source position (samples)", "Boundary (0 loop, 1 one-shot)"); see [SYNTH-ADVANCED-GRAINS-BOUNDARY].
- [ ] Show the loaded resource name, duration/rate/channel metadata, waveform and current source position using bounded observations.
- [ ] Include sample replacement in Undo/Redo without invalidating resources still owned by playing grains or history snapshots.
- [ ] Define an initial import-during-playback policy and show it clearly; do not block rendering while preparing a replacement.
- [ ] Persist resource identity/content hash and a portable resolution strategy; missing or changed files require an explicit recoverable result.
- [x] Document import limits, interpolation quality, root-note assumptions and any downmix policy alongside the Grain Cloud example. Done in "WAV import limits and tuning" in the [synth README](../../../src/osprey/packages/synth/README.md): 12 B–8 MiB, PCM 8/16/24/32 or float32, mono/stereo, 8–192 kHz, 2–1,048,576 frames, `(L + R) / 2` downmix, no resampling, root C4, linear non-bandlimited interpolation, stopped-only replacement.

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
- [x] Ship playable cloud, rhythmic-grain and frozen-texture patches with visible modulation routes. The three [Patches factories](../../../src/osprey/packages/synth/src/patches.ospml) wire an LFO to gain or cutoff. On 21 November the [loaded preset journey](../../../src/osprey/packages/synth/tests/clip_journeys.ospml) verified each cable, distinct nonzero PCM bounded by unity, repeatable state and equivalent audio across block splits under GC and ARC. This is offline graph evidence.
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
- [ ] Add pitch/time manipulation only with its own phase/transient handling and latency contract. Partial: `SpectralPitch` (transform 6) is now a peak-locked phase vocoder (Laroche and Dolson): each spectral peak's region moves by the whole bins nearest its true frequency's shift and keeps its lobe, the peak's phase advances at the shifted frequency and the bins around it keep their analysed offset (identity phase locking) ([spectral_peaks.ospml](../../../src/osprey/packages/synth/src/spectral_peaks.ospml), 4e212e09f, 1b45a3ba0). Latency is size − 1. The `pitchShifter` component is the FFT pitch-shift effect module (Prism palette "FFT pitch shifter", project component `pitchShifter`), preset to a plain shift (formant = ratio). Evidence, middle C through the song renderer at 1024 points: a sine keeps 60% of its energy a fifth up (ratio 1.5), 76% an octave up and 96% an octave down (the bin-by-bin mapping it replaced kept 12% an octave up); the shortfall is amplitude ripple, which [SYNTH-SPECTRAL-PITCH-EFFECT] requires removed. Shifts beyond an octave are out of scope; the daw test asserts G4 and C5 periods, half-period anti-correlation and more than half the level. Transient handling and time stretching remain open.
- [x] FFT pitch-shift effect in musical units ([SYNTH-SPECTRAL-PITCH-EFFECT]): `semitones` −12…+12 and `cents` −100…+100 replace the raw ratio on the `pitchShifter` effect (ratio formula in `SpectralMath.semitoneRatio`); formant preservation is an explicit option, off by default; range limited to one octave either way; acceptance within 1 cent and 1 dB, no ripple above 1 dB, at ±1/5/7/12 semitones and ±1/10/50 cents. Done: the spectral node takes `semitones`, `cents` and `keepFormants` (0 off), the ratio is `SpectralMath.semitoneRatio`, `formantRatio` keeps formants only when asked. Evidence: "the FFT pitch shifter moves a sine by 1, 5, 7 and 12 semitones and 1, 10 and 50 cents either way within 1 cent, 1 dB of level and 1 dB of ripple" in [wave_journeys.ospml](../../../src/osprey/packages/synth/tests/wave_journeys.ospml) measured on 21 Nov (500 Hz full-scale sine, 16 kHz, size 1024): worst pitch error 0.011 cent, worst level −0.42 dB (−7 semitones), worst ripple 0.06 dB; synth suite 56/56 under ARC. Measured on sines only, not on noisy or polyphonic input.
- [ ] Real-time autotune ([SYNTH-SPECTRAL-AUTOTUNE]): YIN-style fundamental detection with confidence, snapping to key/scale or the timeline's notes, retune speed, amount and humanise, formant-preserving FFT correction, unvoiced frames passed dry, bounded per-hop work without allocation, declared latency; native Osprey formulas; tests for a 30-cent-flat vibrato line, glide timing, unvoiced noise and block-split invariance.
- [x] Expose analysis/resynthesis separately so multiple transformations can share one analysis where compatible. `SpectralSeries` runs 1–8 stateless steps (band, shift, morph) on one analysis and one resynthesis; stateful steps (freeze, smear, pitch) are rejected as incompatible. Probe: band 500–2,000 Hz then +4 bins moves 1,031.25 Hz to exactly 1,406.25 Hz; a series containing freeze is rejected.
- [x] Formant pitch shifting [SYNTH-ADVANCED-SPECTRAL-FORMANT]: `SpectralPitch { ratio formant }` moves pitch by `ratio` and the spectral envelope by `formant` independently; `formant = 1` keeps the voice's formants where they were, `formant = ratio` is the plain (chipmunk) shift. The envelope is the per-frame peak magnitude within about 150 Hz (it follows partial peaks, not the gaps between them); each output bin is rescaled by envelope(j / formant) / envelope(j / ratio), bounded so silent bins are never amplified. Gain formulas live in `spectral_math.ospml`. A loaded test checks a vowel-like source (fixed resonances, moving pitch): pitch lands on target and the resonance peaks stay within one bin with `formant = 1`, and move with `formant = ratio`. Evidence (da0d559c9): 156.25 Hz vowel with a 1 kHz resonance, ratio 1.5 at 16 kHz — kept: 937.5 Hz/1406.25 Hz = 1.83; moved: 0.118; old-pitch and off-harmonic bins < 1e-6; the spectral node has a Formant control (0.25–4, default 1). Synth 40/40 GC and ARC.
- [x] Spectral morphing modes [SYNTH-SPECTRAL-MORPH]: Blend (log magnitude), Envelope (cross-synthesis, main input carries) and Partial (peaks paired in frequency order, gliding in log frequency and level as phase-locked lobes) on `SpectralMorph { amount mode }`, with the spectral node's morph-mode control (c6b1fd48a, 81552760a; [research](../../research/synthesis/spectral-morphing.md)). Evidence: blend 0.5↔0.05 midway 0.158 (5 amounts within 5%); envelope moves a 1 kHz resonance to 1.875 kHz; partial 220→330 Hz at 0.5 puts harmonics at k × 269.44 Hz, level 1/k within 20%, sources 30 dB down, over ten glides. Synth 42/42 GC and ARC. Open: automation-sweep smoothness and orphan-fade level tests; MQ-style cross-frame tracking for inharmonic sounds.
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
- [x] Treat a neural waveform generator as an optional later component requiring its own latency, memory and degradation contract. Stated in [Synthesis.md](../../specs/synth/Synthesis.md) [SYNTH-ADVANCED-NEURAL]: no instrument, patch or test depends on one, and none ships before its own latency, memory and degradation contract.
- [ ] Add application controls to preview/apply/reject fitted parameters without overwriting a working patch silently.
