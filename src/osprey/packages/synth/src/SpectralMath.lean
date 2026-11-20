/-! Short-time Fourier analysis and resynthesis mathematics [SYNTH-ADVANCED-SPECTRAL].

Periodic Hann analysis and synthesis windows with hop `size / 4` or `size / 8` sum to the
constant `3 size / (8 hop)`, so overlap-add reconstruction scales by its reciprocal
(Allen and Rabiner; Smith, *Spectral Audio Signal Processing*).
-/
namespace synth.SpectralMath

/-- Periodic Hann window value at `index` of `size`. -/
def hann (index size : Float) : Float := 0.5 - 0.5 * Float.cos (6.283185307179586 * index / size)

/-- Overlap-add gain restoring unity after analysis and synthesis Hann windows. -/
def overlapScale (size hop : Float) : Float := 8 * hop / (3 * size)

/-- FFT twiddle angle for bin `index` of a transform of `size`; `sign` is -1 forward, +1 inverse. -/
def twiddle (sign index size : Float) : Float := sign * 6.283185307179586 * index / size

/-- Complex product components. -/
def productRe (ar ai br bi : Float) : Float := ar * br - ai * bi
def productIm (ar ai br bi : Float) : Float := ar * bi + ai * br

/-- Squared modulus of a complex value. -/
def power (re im : Float) : Float := re * re + im * im

/-- Magnitude of a complex value. -/
def magnitude (re im : Float) : Float := Float.pow (power re im) 0.5

/-- Unit-vector components; silence maps to phase zero. -/
def unitRe (re im : Float) : Float :=
  let m := magnitude re im
  if m <= 0 then 1 else re / m
def unitIm (re im : Float) : Float :=
  let m := magnitude re im
  if m <= 0 then 0 else im / m

/-- Centre frequency of bin `index`. -/
def binFrequency (index size sampleRate : Float) : Float := index * sampleRate / size

/-- Expected phase advance of a bin-centred sinusoid over one hop. -/
def hopAdvance (index hop size : Float) : Float := 6.283185307179586 * index * hop / size

/-- Band-pass gain with linear one-bin-wide edges around `low` and `high` hertz. -/
def bandGain (frequency low high width : Float) : Float :=
  if frequency < low - width || frequency > high + width then 0
  else if frequency < low then (frequency - (low - width)) / width
  else if frequency > high then ((high + width) - frequency) / width
  else 1

/-- Linear interpolation used for smear and morph. -/
def blend (from to amount : Float) : Float := from + (to - from) * amount

/-- Phase angle of a complex value. -/
def angle (re im : Float) : Float := Float.atan2 im re

/-- Wrap an angle into [−π, π). -/
def principal (value : Float) : Float :=
  value - 6.283185307179586 * Float.floor ((value + 3.141592653589793) / 6.283185307179586)

/-- Phase-vocoder instantaneous frequency of bin `index`, in bins, from its phase advance over one hop. -/
def trueBin (index phase previous hop size : Float) : Float :=
  index + principal (phase - previous - 6.283185307179586 * index * hop / size) * size / (6.283185307179586 * hop)

/-- Synthesis phase advance for a component at `bin` (fractional) over one hop. -/
def binAdvance (bin hop size : Float) : Float := 6.283185307179586 * bin * hop / size

/-- Rectangular components of `level · e ^ (i·phase)`: resynthesis from magnitude and phase. -/
def polarRe (level phase : Float) : Float := level * Float.cos phase
def polarIm (level phase : Float) : Float := level * Float.sin phase

/-- Nearest integer with ties rounded up: the output bin of the pitch map `round (k · ratio)`. -/
def nearest (value : Float) : Float := Float.floor (value + 0.5)

/-- Inclusive source-bin bounds that can land on output bin `target` under `ratio`. -/
def sourceLow (target ratio : Float) : Float := Float.floor ((target - 0.5) / ratio) - 1
def sourceHigh (target ratio : Float) : Float := Float.ceil ((target + 0.5) / ratio) + 1

end synth.SpectralMath
