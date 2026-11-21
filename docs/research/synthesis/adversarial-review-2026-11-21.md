# Adversarial review of the synth and the DAW, 2026-11-21 [SYNTH-REVIEW-2026-11-21]

Four `zai/glm-5.3` reviewers attacked the spectral effects, the dynamics, the DAW editor and the DAW render path.
Every finding was checked against the code before it was fixed or refuted. Each fix has a test that failed
without it, and the guards named below keep it fixed. Model results are recorded in PiNotes.

## How these got in

Every one of these defects sat on a path with no test at its boundary. Each control was checked on its own, and
nothing checked the total the controls produce. Tails and ranges were written for one FFT size and then assumed
for every size. Accumulation used `listConcat`, which copies its left side. A fractional value was truncated
instead of refused. Don't add a control, a size or a limit without a test at its edges, at every size it takes.

## Synth: fixed

| Finding | Fix | Guards |
| --- | --- | --- |
| The spectral tail was `size − 1 + hop`, so hosts cut up to 512 samples of a shifted note at 1024 points | `b9a9823db`: `2 · (size − 1)` | `tailHeld` (audible past latency plus a hop, exact silence after the tail); `spectralTimed` at 64 to 4096 points for both spectral nodes |
| A fractional bin shift was silently truncated | `b9a9823db`: refused with "Spectral shift must be a whole number of bins." | `shiftEdges`: nine fractions refused, eight whole shifts rendered |
| The shift dial spanned ±2047 at every size, so a legal dial value refused every sample | `3daba3b3a`: ±(size / 2 − 1) | `spectralTimed` checks the range at seven sizes; `shiftEdges` plays ±31 at 64 points and refuses ±32 and ±2047 |
| Spectral packing and note accumulation were quadratic (0.58 s against 0.01 s for 300 blocks) | `9d08c8a0b`: `Sequences::joined` appends | `blocksJoined`: order and length for six shapes, including 2000 × 256, which times out if joining copies |

### Refuted

The dynamics reviewer said the true-peak limiter read its oversampler four frames late and under-held
transients. The prototype filter is centred at 15.5, so the phases are right. Applying the suggested change
breaks `limitBurst`, which now runs a burst at starts 0, 1, 7, 500, 1000 and 2391 at four limiter settings (`0cffa8ee7`).

## DAW: fixed (in the daw history journeys)

- A key a module cannot reach refuses the strike, naming the note and frequency (`pitchRangeJourney`).
- A project file with an unplayable tempo or drive, a routing whose attack or release lies outside 0.001 to 10 s,
  an out-of-range sustain, or two tracks with one name is refused when it opens (`projectRefusalJourney`, `sustainRefusalJourney`).
- Sustain lengths that every control allowed but `frozen` refused now render, and longer totals are refused at decode.

## Open

- The far Space room scales the size by 0.91, so a Space size below 0.2748 refuses the whole render.
- `Arrangements::frames` ignores sustain pads, so pads and their reverb are cut at the piece end.
- At 8 kHz a 0.5 s intro tail gives 4000 samples, under the 4096 a sustain needs.
- Editing a built-in instrument in Prism replaces its sound and drops its settings; [built-in instruments become patches](../../plans/daw.md) is the fix.
