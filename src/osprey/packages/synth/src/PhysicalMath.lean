/-! Physical and modal synthesis mathematics [SYNTH-ADVANCED-PHYSICAL].

References: J. O. Smith, *Physical Audio Signal Processing* (digital waveguides, the
Karplus–Strong loss filter, pick-position combs, first-order allpass dispersion) and
Bilbao/Rossing for modal frequency ratios. Ratios for bars, membranes and bells are
idealised textbook values, not measurements of any particular instrument.
-/
namespace synth.PhysicalMath

/-- Per-step gain giving a 60 dB decay after `decay` seconds, applied every `step` samples. -/
def decayGain (decay step sampleRate : Float) : Float :=
  Float.pow 10 (-3 * step / (decay * sampleRate))

/-- Radian frequency per sample. -/
def omega (frequency sampleRate : Float) : Float := 6.283185307179586 * frequency / sampleRate

/-- Coupled-form (Gold–Rader) resonator: the state rotates by `angle` and shrinks by `radius` each
sample, so retuning or re-damping can never increase stored energy. Output is the imaginary part. -/
def coupledCosine (radius angle : Float) : Float := radius * Float.cos angle
def coupledSine (radius angle : Float) : Float := radius * Float.sin angle
def coupledReal (real imag cosine sine drive : Float) : Float := cosine * real - sine * imag + drive
def coupledImag (real imag cosine sine : Float) : Float := sine * real + cosine * imag

/-- Input scale for about `gain` at the resonant peak of the coupled form (a band-pass). -/
def peakInput (radius angle gain : Float) : Float :=
  gain * (1 - radius) * Float.pow (1 - 2 * radius * Float.cos (2 * angle) + radius * radius) 0.5 / (radius * Float.sin angle)

/-- Brightness 1 is a transparent loss filter; brightness 0 is the two-point average. -/
def smoothing (brightness : Float) : Float := 0.5 * (1 - brightness)

/-- Frequency-dependent loop loss: `gain * ((1 - s) x[n] + s x[n-1])`, unity-or-lower gain. -/
def loss (current previous gain s : Float) : Float := gain * ((1 - s) * current + s * previous)

/-- Low-frequency phase delay of the first-order allpass `(a + z⁻¹) / (1 + a z⁻¹)`. -/
def allpassDelay (a : Float) : Float := (1 - a) / (1 + a)

/-- Delay-line read length that tunes the loop, after subtracting the loss and allpass delay. -/
def loopDelay (frequency sampleRate s a : Float) : Float :=
  sampleRate / frequency - s - allpassDelay a

/-- Modal frequency ratios for the first eight modes of idealised bodies. -/
def harmonicRatio (index : Float) : Float := index + 1

def barRatio (index : Float) : Float :=
  if index < 1 then 1 else if index < 2 then 2.756 else if index < 3 then 5.404
  else if index < 4 then 8.933 else if index < 5 then 13.345 else if index < 6 then 18.638
  else if index < 7 then 24.814 else 31.871

def membraneRatio (index : Float) : Float :=
  if index < 1 then 1 else if index < 2 then 1.594 else if index < 3 then 2.136
  else if index < 4 then 2.296 else if index < 5 then 2.653 else if index < 6 then 2.918
  else if index < 7 then 3.156 else 3.501

def bellRatio (index : Float) : Float :=
  if index < 1 then 0.5 else if index < 2 then 1 else if index < 3 then 1.183
  else if index < 4 then 1.506 else if index < 5 then 2 else if index < 6 then 2.514
  else if index < 7 then 2.662 else 3.011

/-- Ideal-string strike/pickup weighting of mode `index`; an approximation for other bodies. -/
def positionWeight (index position : Float) : Float :=
  Float.sin (3.141592653589793 * (index + 1) * position)

/-- Higher modes decay faster as brightness falls: `decay / ratio ^ (1 - brightness)`. -/
def modeDecay (decay ratio brightness : Float) : Float := decay / Float.pow ratio (1 - brightness)

/-- Bipolar white noise from a Park–Miller state. -/
def bipolar (seed : Float) : Float := 2 * seed / 2147483647 - 1

/-- One-pole colouring: colour 0 is white, colour 1 is the darkest supported setting. -/
def coloured (smoothed white colour : Float) : Float := smoothed + (1 - 0.99 * colour) * (white - smoothed)

/-- Linear burst envelope over `duration` samples. -/
def burst (strength age duration : Float) : Float :=
  if age >= duration then 0 else strength * (1 - age / duration)

/-- One-pole smoothing coefficient: 63% of a step after `time` seconds. -/
def smoothingCoefficient (time sampleRate : Float) : Float := 1 - Float.exp (-1 / (time * sampleRate))

/-- Linear slew: move toward `target` by at most `1 / (time · rate)` per sample (full scale in `time`). -/
def slewed (value target time sampleRate : Float) : Float :=
  let limit := 1 / (time * sampleRate)
  let step := target - value
  if step > limit then value + limit else if step < -limit then value - limit else target

/-- STK bow friction table `(|Δv · slope| + 0.75)^-4`, clipped to `[0.01, 0.98]`, so the bow
injection never exceeds the velocity difference. -/
def bowTable (difference slope : Float) : Float :=
  let raw := Float.pow (Float.abs (difference * slope) + 0.75) (-4)
  if raw > 0.98 then 0.98 else if raw < 0.01 then 0.01 else raw

/-- STK's pressure-to-slope mapping: pressure 0 (light, slope 5) to 1 (heavy, slope 1). -/
def bowSlope (pressure : Float) : Float := 5 - 4 * pressure

/-- Frequency-independent loss σ for a 60 dB decay in `decay` seconds (per-sample units). -/
def meshLoss (decay sampleRate : Float) : Float := 6.907755278982137 / (decay * sampleRate)

/-- Damped 2-D wave-equation update `(1+σ) u⁺ = 2u − (1−σ) u⁻ + λ² ∇²u`; stable for λ ≤ 1/√2, σ ≥ 0. -/
def meshStep (current previous laplacian courant damping : Float) : Float :=
  (2 * current - (1 - damping) * previous + courant * courant * laplacian) / (1 + damping)

/-- Lowest mode of a fixed-edge rectangular mesh of `width × height` interior points. -/
def meshFundamental (width height courant sampleRate : Float) : Float :=
  courant * sampleRate / 2 * Float.pow (1 / ((width + 1) * (width + 1)) + 1 / ((height + 1) * (height + 1))) 0.5

/-- Denormal policy: feedback state below 1e-30 (about −600 dBFS) is flushed to exactly zero, so decaying
loops reach true silence instead of lingering in slow subnormal arithmetic. -/
def flushed (value : Float) : Float := if Float.abs value < 1e-30 then 0 else value

end synth.PhysicalMath
