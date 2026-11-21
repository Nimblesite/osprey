/-! Spectrum probes the synth tests read rendered audio with [SYNTH-TEST-SPECTRUM].

A Hann-windowed discrete Fourier sum evaluated at one frequency. A sine of amplitude `a` at that
frequency sums to `a · size / 4`, since the window's mean is one half and a real sine puts half its
energy in each sign of frequency, so `4 |X| / size` reads `a` back.
-/
namespace synthtests.ProbeMath

/-- The phase, in radians, of `frequency` at sample `index`. -/
def cycle (frequency index sampleRate : Float) : Float := 6.283185307179586 * frequency * index / sampleRate

/-- The real term of one sample's Fourier sum at `frequency`, weighted by its window value. -/
def windowedRe (sample window frequency index sampleRate : Float) : Float :=
  sample * window * Float.cos (cycle frequency index sampleRate)

/-- The imaginary term of one sample's Fourier sum at `frequency`, weighted by its window value. -/
def windowedIm (sample window frequency index sampleRate : Float) : Float :=
  sample * window * Float.sin (cycle frequency index sampleRate)

/-- The amplitude of the sine the windowed sums read. -/
def sineAmplitude (re im size : Float) : Float := 4 * Float.sqrt (re * re + im * im) / size

/-- An amplitude ratio in decibels; silence reads as -400 dB rather than minus infinity. -/
def decibels (ratio : Float) : Float := 20 * Float.log (ratio + 1e-20) / Float.log 10

/-- `frequency` moved by `cents`. -/
def centsUp (frequency cents : Float) : Float := frequency * Float.pow 2 (cents / 1200)

/-- Where a peak lies between three equally spaced probes, from -0.5 to 0.5 of a step. -/
def vertex (a b c : Float) : Float := if a - 2 * b + c == 0 then 0 else 0.5 * (a - c) / (a - 2 * b + c)

/-- A sawtooth's `k`th harmonic amplitude, 2 / (π k). -/
def sawHarmonic (k : Float) : Float := 2 / (3.141592653589793 * k)

/-- A pulse of duty `width`'s `k`th harmonic amplitude, 4 |sin(π k width)| / (π k). -/
def pulseHarmonic (k width : Float) : Float := 4 * Float.abs (Float.sin (3.141592653589793 * k * width)) / (3.141592653589793 * k)

/-- A triangle's `k`th harmonic amplitude: 8 / (π k)² for odd `k`, none for even. -/
def triangleHarmonic (k : Float) : Float :=
  if k - 2 * Float.floor (k / 2) == 1 then 8 / (9.869604401089358 * k * k) else 0

/-- The prewarped analog frequency ratio a bilinear filter sees at `frequency` against `cutoff`. -/
def warpedRatio (frequency cutoff sampleRate : Float) : Float :=
  Float.tan (3.141592653589793 * frequency / sampleRate) / Float.tan (3.141592653589793 * cutoff / sampleRate)

/-- The magnitude of the second-order state-variable low-pass at ratio `w` and quality `q`. -/
def lowGain (w q : Float) : Float := 1 / Float.sqrt ((1 - w * w) * (1 - w * w) + w * w / (q * q))

/-- The second-order high-pass magnitude: the low-pass times `w²`. -/
def highGain (w q : Float) : Float := w * w * lowGain w q

/-- The unnormalised second-order band-pass magnitude, peaking at `q`: the low-pass times `w`. -/
def bandGain (w q : Float) : Float := w * lowGain w q

/-- A pure sine's `k`th harmonic amplitude: the fundamental alone. -/
def sineHarmonic (k : Float) : Float := if k == 1 then 1 else 0

/-- The magnitude at angular frequency `omega` of the one-pole `y += c (x - y)`. -/
def onePoleGain (c omega : Float) : Float :=
  c / Float.sqrt (1 - 2 * (1 - c) * Float.cos omega + (1 - c) * (1 - c))

/-- The magnitude of the two-tap `gain ((1 - s) x[n] + s x[n-1])`; linear interpolation at fraction `s`. -/
def twoTapGain (gain s omega : Float) : Float :=
  gain * Float.sqrt ((1 - s) * (1 - s) + s * s + 2 * s * (1 - s) * Float.cos omega)

/-- How far above `k f` a stiff string's `k`th partial lies, in cents: `f_k = k f √(1 + B k²)`. -/
def stretchCents (b k : Float) : Float := 600 * Float.log (1 + b * k * k) / Float.log 2

/-- The `k`th partial's level under spectral tilt `tilt`: `1 / k^tilt`. -/
def tiltLevel (k tilt : Float) : Float := 1 / Float.pow k tilt

/-- How far below its Fourier amplitude polyBLEP's correction may hold a partial at `frequency`
(48 kHz), in decibels: the residual's low-pass roll-off. -/
def blepAllowance (frequency : Float) : Float :=
  if frequency <= 2000 then 0.1
  else if frequency <= 5000 then 0.6
  else if frequency <= 9000 then 1.6
  else if frequency <= 15000 then 4.5
  else 10

/-- How far under the fundamental polyBLEP holds a partial the series lacks when a pulse's two edges fall
at different sub-sample positions, in decibels: the residuals no longer cancel near Nyquist. -/
def blepNull (frequency : Float) : Float :=
  if frequency <= 5000 then 60 else if frequency <= 10000 then 35 else 0
