/-! Wavetable and additive synthesis mathematics [SYNTH-ADVANCED-TABLES].

Tables are built from harmonic spectra, so each mip level is exactly bandlimited to its
harmonic count; reading between samples is linear interpolation, whose residual error is
documented in the package README. Stiff-string inharmonicity follows Fletcher's
`n * sqrt(1 + B n^2)`.
-/
namespace synth.TableMath

/-- One harmonic of a periodic table at normalised `position` in `[0, 1)`. -/
def harmonic (amplitude number phase position : Float) : Float :=
  amplitude * Float.sin (6.283185307179586 * (number * position + phase))

/-- The highest whole harmonic below `0.45` of the sample rate at this fundamental. -/
def harmonicLimit (frequency sampleRate : Float) : Float :=
  if Float.abs frequency <= 0 then 1048576 else Float.floor (0.45 * sampleRate / Float.abs frequency)

/-- Partial gain near the upper limit: 1 below 0.4 of the rate, 0 from 0.45. -/
def nyquistFade (frequency sampleRate : Float) : Float :=
  let edge := Float.abs frequency / sampleRate
  if edge >= 0.45 then 0 else if edge <= 0.4 then 1 else (0.45 - edge) / 0.05

/-- A fractional partial count: partial `index` (from zero) fades in continuously. -/
def countGain (count index : Float) : Float :=
  if count - index >= 1 then 1 else if count - index <= 0 then 0 else count - index

/-- Stiff-string ratio of 1-based partial `number`, with an optional stretch exponent. -/
def stiffRatio (number inharmonicity stretch : Float) : Float :=
  Float.pow number stretch * Float.pow (1 + inharmonicity * number * number) 0.5

/-- Spectral tilt: amplitude `number ^ -slope`. -/
def tilt (number slope : Float) : Float := Float.pow number (-slope)

/-- Odd/even balance: +1 removes even partials, -1 removes odd partials, 0 keeps both. -/
def parity (number balance : Float) : Float :=
  let even := 0.5 + 0.5 * Float.cos (3.141592653589793 * number)
  if balance >= 0 then 1 - balance * even else 1 + balance * (1 - even)

/-- Partial `number` reaches -60 dB after `decay / (1 + damping (number - 1))` seconds. -/
def partialDecay (age decay damping number sampleRate : Float) : Float :=
  Float.pow 10 (-3 * age * (1 + damping * (number - 1)) / (decay * sampleRate))

/-- Peak-safe normalisation: divide by the summed gains only when they exceed one. -/
def normalised (sum gains amplitude : Float) : Float :=
  if gains <= 1 then amplitude * sum else amplitude * sum / gains

/-- Sine of a normalised phase. -/
def sinePhase (phase : Float) : Float := Float.sin (6.283185307179586 * phase)

/-- Classic spectra; even harmonics vanish exactly for square and triangle. -/
def sineAmplitude (number : Float) : Float := if number == 1 then 1 else 0
def sawAmplitude (number : Float) : Float := 0.6366197723675814 / number
def squareAmplitude (number : Float) : Float := 0.6366197723675814 * (1 - Float.cos (3.141592653589793 * number)) / number
def triangleAmplitude (number : Float) : Float :=
  0.8105694691387022 * Float.cos (1.5707963267948966 * (number - 1)) / (number * number)

/-- Absolute value, used for peak and gain sums. -/
def magnitude (value : Float) : Float := Float.abs value

def larger (one other : Float) : Float := if one > other then one else other

/-- Exponential pitch control: a normalised control maps to ±`octaves` around the base (0.5 = unchanged). -/
def octaveScale (control octaves : Float) : Float := Float.pow 2 (octaves * (2 * control - 1))

end synth.TableMath
