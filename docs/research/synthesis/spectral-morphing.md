# Spectral morphing [SYNTH-SPECTRAL-MORPH]

Collected 21 November 2026 to justify the three morphing modes planned for `SpectralMorph { amount mode }` ([plan](../../plans/advanced-synthesis.md), [spec](../../specs/runtime/Synthesis.md#morphing-synth-spectral-morph)): *Blend* (log-magnitude interpolation), *Envelope* (cross-synthesis) and *Partial* (matched-peak interpolation). Summaries describe the cited work; the recommended design is ours, not a claim by those authors. Bibliographic details marked *unverified* could not be re-checked against a publisher's record and must be confirmed before formal citation; dates come from the work or its bibliographic record.

## What morphs today

`SpectralFrames.morph` blends magnitudes linearly (`Interpolation.linear`) and keeps the phase of the dominant input (`morphBin`: below `amount` 0.5 the main input), inside the streaming STFT adapter whose latency is `size − 1` samples. Formant pitch shifting already estimates a spectral envelope (`envelope`: peak magnitude within ≈150 Hz) and rescales bins by an envelope ratio (`formantGain`); the phase-vocoder machinery — measured instantaneous frequency (`trueBin`), accumulated synthesis phase, identity phase locking after Laroche and Dolson (1999) — exists in `pitch` and `frozen`. All three modes reuse this analysis; none changes the adapter's latency or tail contracts.

## The literature

**Magnitude domain.** Linear magnitude interpolation is dominated by the louder input, so a midway setting sounds like a crossfade rather than a timbre between. Interpolating magnitudes in a logarithmic (dB) domain — the geometric mean at the midpoint — equalises the two contributions; this is standard practice in timbre morphing (Tellman, Haken and Holloway 1995; Slaney, Covell and Buckhet 1996) and follows from loudness being roughly logarithmic in amplitude. The cost is a floor: blending any partial with silence leaves a ghost at half the floor depth (−60 dB for a −120 dB floor).

**Source–filter morphing.** Decompose each magnitude spectrum into a smooth spectral envelope (filter, vocal tract) and a fine-structure excitation, then interpolate the parts separately. Cepstral smoothing originates with Oppenheim (1969; with Schafer 1968), linear-predictive envelopes with Markel and Gray (1976), iterative *true envelope* estimation with Röbel (2005, DAFx; *unverified*). Cross-synthesis — one sound's envelope on another's excitation — is as old as the channel vocoder (Dudley 1939) and entered computer-music practice with Moorer's phase-vocoder survey (1978). Osprey's peak-envelope estimator is the cheapest member of this family: a bounded max-filter, no transform, but it follows resolved partial peaks rather than a smooth formant curve.

**Sinusoidal partial morphing.** McAulay and Quatieri (1986) analyse and resynthesize speech as tracked sinusoids; Serra's SMS (Serra 1989 thesis; Serra and Smith 1990) adds a stochastic residual for everything the partials miss. Tellman, Haken and Holloway (1995, the Lemur work) morph timbres by matching spectral peaks between two analyses, interpolating each matched pair's frequency linearly in Hz and its amplitude in dB, and crossfading unmatched peaks — their contribution is handling unequal peak counts. Slaney, Covell and Buckhet (1996) automate morphing by aligning features and interpolating in a perceptual domain. Bonada, Loscos, Cano, Serra and de Boer (ICMC 2001; *unverified*) apply SMS-style morphing to singing voice. The Loris package of Fitz and Haken continues this line (software; no single paper verified).

**Phase.** The phase vocoder (Flanagan and Golden 1966; Dolson's 1986 tutorial) accumulates synthesis phase from measured instantaneous frequency; naive magnitude morphs carrying either input's phase sound phasy and beat. Phase locking — every bin under a peak shares that peak's rotation (Puckette 1995; Laroche and Dolson 1999 identity locking) — keeps a partial's bins coherent; Osprey's `locked` already implements this. Transients are the known casualty of phase-vocoder manipulation; Verma and Meng (ICMC 2000; *unverified*) model them explicitly, and SMS keeps them in the residual.

**Automation and perception.** An automated morph amount changes the modified STFT frame by frame, and overlap-add then reconstructs a signal whose frames are mutually inconsistent (Griffin and Lim 1984), so fast sweeps ripple. Perceptually, listeners weigh spectral envelope and pitch heavily — the motivation for Slaney et al.'s alignment — and a morph is convincing when prominent features *move* continuously rather than crossfade.

## Methods compared

| Method | Interpolates | Phase | Strengths | Weaknesses |
| --- | --- | --- | --- | --- |
| Linear magnitude blend (current) | amplitude, linearly | dominant input | trivial, stateless | midway ≈ the louder input; crossfade character |
| dB magnitude blend | amplitude, geometrically | dominant input, locked | midway is perceptually midway; stateless | floor ghosting; partials stay put |
| Source–filter morph (cepstrum/LPC/true envelope) | envelope and excitation separately | dominant input, locked | independent formant and excitation trajectories | estimator cost/bias; envelope–pitch confusion |
| Peak-envelope cross-synthesis | envelope of A onto excitation of B, by amount | side input, locked | classic vocoder timbre; reuses `envelope` | degenerates when f0 exceeds the envelope radius |
| MQ/SMS-style partial morph | matched-peak frequency (Hz) and amplitude (dB) | dominant pair member, locked | partials genuinely travel between pitches | needs peak matching; noise and transients unmodelled |
| Automatic morphing (Slaney et al.) | aligned features, perceptual scale | analysis-driven | hands-free | complex; offline in practice |

## Built design

Three modes on `SpectralMorph { amount mode }` (81552760a), all inside the existing STFT node, with formulas in `SpectralMath.lean`. Every mode keeps one contract: amount 0 is the main input and 1 is the side.

**Blend** (`logBlend`). Per bin, magnitude is `exp ((1 − t) ln (a + ε) + t ln (b + ε)) − ε` with ε = 10⁻⁹, the geometric path. Phase comes from the dominant input. Partials stay at their own frequencies, so this is a morph in timbre, not pitch. A sine at 0.5 blended with one at 0.05 measures 0.158 halfway (a crossfade would give 0.275), and 0.281 and 0.089 at the quarters, all within 5%.

**Envelope** (`envelopeMorph`). Cross-synthesis keeps the main input's harmonics and phase, and rescales each bin by `(E_side / E_main) ^ t`. Both envelopes use the formant shifter's peak envelope (about 150 Hz), the gain is capped at 16, and a silent own-envelope gives a gain of 1. Unlike the design above, which made the side input the carrier, the main input stays the carrier, so 0 is the main input as in every other mode. A vowel with a 1 kHz resonance takes on the side input's 1.875 kHz resonance at amount 1, with no energy between its harmonics.

**Partial** (`SpectralPartials`). Peaks within 26 dB (5%) of the frame's loudest bin are kept. The threshold sits above the Hann window's −31 dB sidelobes, so a sidelobe is never matched as a partial. Each peak is centred by parabolic interpolation of its log levels (`peakOffset`, `logLevel`). Main and side peaks pair in frequency order, so harmonic k pairs with harmonic k. A pair glides in *log* frequency and log level (`logBlend`). The geometric midpoint between two pitches is the musical one (220 → 330 Hz at 0.5 is 269.4 Hz, the equal-tempered midpoint; linear Hz would give 275). An unpaired peak fades where it stands. Each glided peak's five-bin lobe is redrawn with the centre's accumulated synthesis phase, and each neighbour sits π further on (`lobePhase`), as a Hann-windowed sinusoid's bins do. Without that phase lock the output level fell to 0.32. With it, a unit fundamental comes out at 0.9997, and the 1/2 and 1/3 harmonics at 0.4996 and 0.333. Partial morphing is stateful because it carries synthesis phase, so a spectral series refuses it.

Tests (`wave_journeys.ospml`): Blend at five amounts. Envelope at three amounts, with both resonances checked. Partial over ten glides: rising, falling, fifths, an octave and amounts from 0 to 1. Each harmonic k = 1–3 must land at k × the glide with level 1/k within 20%, and each source pitch must sit 30 dB below it.

## Validation (measurable acceptance)

Rendered at 48 kHz, size 4,096, hop 1,024, tones at −12 dB, measured on the rendered output after latency alignment; spectral peaks measured with parabolic interpolation on a 65,536-point FFT (bin 0.73 Hz).

| # | Test | Acceptance |
| --- | --- | --- |
| 1 | Blend midpoint: 220 Hz and 330 Hz harmonic tones, amount 0.5 | each partial's magnitude = geometric mean of the two inputs' partials within 0.5 dB; partials remain at 220·k and 330·k Hz (positions unchanged) |
| 2 | Blend endpoints: amount 0 and 1 | rms difference vs direct pass of main/side ≤ 1e-5 |
| 3 | Envelope carrier: amount 0 | rms difference vs direct pass of the side ≤ 1e-5 |
| 4 | Envelope cross: main = 220 Hz saw through a 1 kHz resonance, side = 330 Hz saw, amount 1 | output peaks only at 330·k Hz within one bin; M(990)/M(330) within 1 dB of the main's envelope ratio E(990)/E(330) |
| 5 | Envelope midway: same inputs, amount 0.5 | peaks within one bin of 330·k Hz; the dB tilt across them halfway between tests 3 and 4 within 1.5 dB |
| 6 | Partial midpoint: 220 Hz and 330 Hz harmonic tones, amount 0.5 | fundamental at 275 ± 5 Hz; partial k at 275·k ± 5 Hz for k ≤ 8; ≥ 95% of harmonic energy within ±1 bin of 275·k |
| 7 | Partial orphans: main 220 Hz + side 277 Hz, amount 0.25 | unmatched main partials at 0.75·magnitude within 1 dB; unmatched side partials at 0.25·magnitude within 1 dB |
| 8 | Steadiness: held amount 0.5 on steady tones | per-hop output rms constant within 0.5 dB after the latency window |
| 9 | Automation: amount ramps 0 → 1 over 0.5 s | no frame-to-frame magnitude jump > 2 dB beyond the monotone trend |

## Failure modes

- **Floor ghosting** (Blend, Partial amplitudes): a partial blended with silence leaves a −floor/2 dB image (−60 dB at 0.5). Keep the floor at −120 dB and never raise true silence above it; the bound is measured, not hidden.
- **Envelope radius bias** (Envelope): fundamentals above ≈150 Hz, or partials spaced wider than the radius, collapse the peak envelope onto the partial peaks, and Envelope mode degenerates toward Blend. Cepstral or true-envelope smoothing (Oppenheim 1969; Röbel 2005) is the upgrade at FFT cost.
- **Match churn** (Partial): inharmonic or noisy spectra give different order-based pairings each frame → warble. No cross-frame tracking in v1; MQ-style tracking is the literature fix. Peak-count and magnitude thresholds bound the damage.
- **Transients and noise** (Partial): matched peaks cannot represent attacks or breathiness (the SMS residual; Verma and Meng), so percussive material smears over the window; Blend is the fallback mode.
- **Mid-amount phase incoherence** (Blend): bins inherited from the non-dominant input carry the dominant's phase, so partial cancellation can occur; identity locking mitigates it within one peak, not across inputs.
- **The 0.5 phase-donor switch** (Blend): the dominant-input rule flips at amount 0.5, a phase seam when sweeping through. Envelope avoids it (side phase throughout). Crossfading donors near 0.5 is the open refinement.
- **Overlap-add inconsistency under fast automation** (all modes): frame-to-frame changes make the modified STFT inconsistent (Griffin and Lim 1984); bound amount sweeps to ≲ one per window.

## References

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
