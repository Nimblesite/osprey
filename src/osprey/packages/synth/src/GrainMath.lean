/-! Sample-grain mathematics [SYNTH-ADVANCED-GRAINS]. -/
namespace synth.GrainMath
/-- Periodic source addressing also supports negative playback rates. -/
def wrapped (position size : Float) : Float :=
  let value := position - Float.floor (position / size) * size
  if value >= size then 0 else value
/-- A symmetric Hann window with zero endpoints; duration is validated above two samples. -/
def window (age duration : Float) : Float :=
  if age <= 0 || age >= duration - 1 then 0
  else 0.5 - 0.5 * Float.cos (6.283185307179586 * age / (duration - 1))
/-- Linear source interpolation; this is not a bandlimited resampler. -/
def interpolated (a b fraction : Float) : Float := a + (b - a) * fraction
/-- Equal-power stereo placement for pan in [-1, 1]. -/
def leftGain (pan : Float) : Float := Float.cos ((pan + 1) * 0.7853981633974483)
def rightGain (pan : Float) : Float := Float.sin ((pan + 1) * 0.7853981633974483)
def cursor (position rate size : Float) : Float := wrapped (position + rate) size
def weighted (sample amplitude age duration : Float) : Float := sample * amplitude * window age duration
def placed (sample gain : Float) : Float := sample * gain
def fraction (position : Float) : Float := position - Float.floor position
def summed (a b : Float) : Float := a + b
end synth.GrainMath
