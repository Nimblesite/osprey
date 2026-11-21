/-! Nonlinear, feedback and frequency-shifting mathematics [SYNTH-ADVANCED-NONLINEAR].

First-order antiderivative antialiasing follows Parker, Zavalishin and Le Bivic (DAFx 2016):
`y = (F(x[n]) - F(x[n-1])) / (x[n] - x[n-1])`, falling back to the curve at the midpoint when
the inputs are nearly equal. Every curve is bounded to `[-1, 1]`. The Hilbert pair uses Olli
Niemitalo's published polyphase IIR coefficients; the feedback network uses a 4×4 Householder
reflection, which is orthogonal (lossless).
-/
namespace synth.NonlinearMath

/-- Hyperbolic tangent, saturated beyond |x| = 20 where it equals ±1 in binary64. -/
def tanh (x : Float) : Float :=
  let c := if x > 20 then 20 else if x < -20 then -20 else x
  let e := Float.exp (2 * c)
  (e - 1) / (e + 1)

/-- Antiderivative of tanh, `log cosh x`, in an overflow-free form. -/
def tanhIntegral (x : Float) : Float :=
  let a := Float.abs x
  a + Float.log (1 + Float.exp (-2 * a)) - 0.6931471805599453

def hard (x : Float) : Float := if x > 1 then 1 else if x < -1 then -1 else x
def hardIntegral (x : Float) : Float :=
  let a := Float.abs x
  if a <= 1 then x * x / 2 else a - 0.5

/-- Cubic soft clip `1.5 (x - x³/3)`, reaching ±1 at |x| = 1. -/
def cubic (x : Float) : Float :=
  if x >= 1 then 1 else if x <= -1 then -1 else 1.5 * (x - x * x * x / 3)
def cubicIntegral (x : Float) : Float :=
  let a := Float.abs x
  if a <= 1 then 1.5 * (x * x / 2 - x * x * x * x / 12) else a - 0.375

/-- Sine wavefolder. -/
def fold (x : Float) : Float := Float.sin (1.5707963267948966 * x)
def foldIntegral (x : Float) : Float := -0.6366197723675814 * Float.cos (1.5707963267948966 * x)

/-- One-pole DC blocker. -/
def dcBlock (input previousInput previousOutput coefficient : Float) : Float :=
  input - previousInput + coefficient * previousOutput
def dcCoefficient (cutoff sampleRate : Float) : Float := Float.exp (-6.283185307179586 * cutoff / sampleRate)

/-- Blackman-windowed half-band low-pass tap `index` of `taps` (odd), for 2× resampling. -/
def halfband (index taps : Float) : Float :=
  let centre := (taps - 1) / 2
  let t := index - centre
  let window := 0.42 - 0.5 * Float.cos (6.283185307179586 * index / (taps - 1)) + 0.08 * Float.cos (12.566370614359172 * index / (taps - 1))
  if t == 0 then 0.5 * window else window * Float.sin (1.5707963267948966 * t) / (3.141592653589793 * t)

/-- Schroeder allpass: internal `v = x + g v[n-D]`, output `y = v[n-D] - g v`. -/
def schroederIn (input delayed gain : Float) : Float := input + gain * delayed
def schroederOut (internal delayed gain : Float) : Float := delayed - gain * internal

/-- One-pole damping inside the feedback loop; gain never exceeds one. -/
def damp (input previous damping : Float) : Float := (1 - damping) * input + damping * previous

/-- Householder reflection row `x_i − (2/N) Σ x`: the matrix is orthogonal, so it preserves energy. -/
def householder (value sum count : Float) : Float := value - 2 * sum / count

/-- Second-order Hilbert allpass section `a²(x[n] + y[n-2]) - x[n-2]`. -/
def hilbert (coefficient input olderInput olderOutput : Float) : Float :=
  coefficient * coefficient * (input + olderOutput) - olderInput

/-- Single-sideband shift from the Hilbert pair (the second path lags the delayed first by 90°),
selecting the upper sideband for positive carrier frequency. -/
def shifted (inPhase quadrature phase : Float) : Float :=
  inPhase * Float.cos (6.283185307179586 * phase) + quadrature * Float.sin (6.283185307179586 * phase)

end synth.NonlinearMath
