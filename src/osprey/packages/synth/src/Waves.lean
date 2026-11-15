/-! # Waves of `@osprey/synth` [SYNTH-DSP] [SYNTH-PERFORMANCE]

The mathematics of the oscillators and performance timbres. Phase is normalised to one cycle
per unit, `step` is the phase advanced per sample (frequency over sample rate), and every
output is nominally within `[-1, 1]`.

The polynomial corrections reduce aliasing at each discontinuity; they do not eliminate it
([SYNTH-RESEARCH-ANTIALIASING]). The Osprey module is generated from this file ([MATH-CODE]).
-/
namespace synth.Waves

/-- The fractional part of a phase: a phase wrapped into `[0, 1)`. -/
def wrap (phase : Float) : Float := phase - Float.floor phase

/-- The integral of a local linear discontinuity correction (polyBLEP), one sample either side
of a step. -/
def blep (phase step : Float) : Float :=
  if step ≤ 0 then 0
  else if phase < step then
    let x := phase / step
    x + x - x * x - 1
  else if phase > 1 - step then
    let x := (phase - 1) / step
    x * x + x + x + 1
  else 0

/-- The integral of the BLEP residual (polyBLAMP), for a corner in the waveform. -/
def blamp (phase step : Float) : Float :=
  if step ≤ 0 then 0
  else if phase < step then (1 - phase / step) ^ 3 / 3
  else if phase > 1 - step then (1 + (phase - 1) / step) ^ 3 / 3
  else 0

/-- One cycle of a sine per unit phase. -/
def sine (phase : Float) : Float := Float.sin (6.283185307179586 * phase)

/-- A sawtooth rising from -1 to 1, corrected at its wrap. -/
def saw (phase step : Float) : Float := 2 * phase - 1 - blep phase step

/-- A pulse that is high for `width` of the cycle, corrected at both edges. -/
def pulse (phase step width : Float) : Float :=
  let plain := if phase < width then 1 else -1
  plain + blep phase step - blep (wrap (phase - width)) step

/-- A triangle whose derivative jumps by +8 and -8, corrected at both corners. -/
def triangle (phase step : Float) : Float :=
  1 - 4 * Float.abs (phase - 0.5) + 4 * step * (blamp phase step - blamp (wrap (phase - 0.5)) step)

/-- The partial at `multiple` times the fundamental, silent once it would pass Nyquist. -/
def harmonic (phase step multiple : Float) : Float :=
  if step * multiple < 0.5 then sine (phase * multiple) else 0

/-- A glassy timbre: the fundamental with a third harmonic that grows with `tone`. -/
def glass (phase step tone : Float) : Float :=
  sine phase * (1 - tone * 0.3) + harmonic phase step 3 * tone * 0.3

/-- A bass timbre: the fundamental with a second harmonic that grows with `tone`. -/
def bass (phase step tone : Float) : Float :=
  sine phase * 0.65 + harmonic phase step 2 * (0.1 + tone * 0.25)

/-- A hollow timbre: the fundamental with odd harmonics that grow with `tone`. -/
def hollow (phase step tone : Float) : Float :=
  sine phase * 0.6 + harmonic phase step 3 * (0.1 + tone * 0.15) + harmonic phase step 5 * tone * 0.15

/-- Phase modulation by the octave above, deepening with `tone`. -/
def modulated (phase tone : Float) : Float :=
  sine (phase + (0.05 + tone * 0.3) * sine (phase * 2))

/-- Ring modulation: `warp` fades the dry signal into inharmonic sidebands at seven times the
fundamental. -/
def ring (dry phase warp : Float) : Float :=
  dry * (1 - warp + warp * sine (phase * 7))

end synth.Waves
