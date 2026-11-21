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

/-- Spectral-envelope radius in bins: about 150 Hz. -/
def envelopeRadius (size sampleRate : Float) : Float := Float.ceil (150 * size / sampleRate)

/-- Formant gain: wanted over current envelope, 1 where the envelope is silent, at most 16 (+24 dB). -/
def formantGain (wanted current : Float) : Float :=
  if current <= 0.000000001 then 1 else if wanted / current > 16 then 16 else wanted / current

/-- Log-magnitude interpolation: the geometric path between two levels, so halfway sounds halfway. -/
def logBlend (a b amount : Float) : Float :=
  Float.exp ((1 - amount) * Float.log (a + 0.000000001) + amount * Float.log (b + 0.000000001)) - 0.000000001

/-- Cross-synthesis gain `(wanted / own) ^ amount`, 1 where the own envelope is silent, at most 16. -/
def envelopeMorph (wanted own amount : Float) : Float := Float.pow (formantGain wanted own) amount

/-- Natural-log level of a magnitude, floored at 1e-9. -/
def logLevel (m : Float) : Float := Float.log (m + 0.000000001)

/-- Parabolic peak offset in bins from three neighbouring log levels (Smith and Serra, 1987). -/
def peakOffset (a b c : Float) : Float := if a - 2 * b + c == 0 then 0 else 0.5 * (a - c) / (a - 2 * b + c)

/-- A Hann lobe bin's phase: the centre's phase plus π per bin of offset. -/
def lobePhase (centre offset : Float) : Float := principal (centre + 3.141592653589793 * offset)

/-- Whole bins a peak region moves so its lobe lands nearest the peak's true `frequency` (in bins) times `ratio`. -/
def peakShift (frequency ratio : Float) : Float := nearest (frequency * ratio - frequency)

/-- Pitch ratio of a shift by `semitones` plus `cents` hundredths of a semitone: 2 ^ ((semitones + cents / 100) / 12). -/
def semitoneRatio (semitones cents : Float) : Float := Float.pow 2 ((semitones + cents / 100) / 12)

/-- Formant ratio of a pitch shift by `ratio`: 1 with `keep` on holds the formants still, otherwise they move with it. -/
def formantRatio (keep ratio : Float) : Float := if keep >= 0.5 then 1 else ratio

/-- Identity phase locking: a bin near a moved peak keeps its analysed phase offset from that peak. -/
def lockedPhase (peakPhase phase peakAnalysis : Float) : Float := principal (peakPhase + phase - peakAnalysis)

/-- One analysis sample: the input under a Hann window of `size`. -/
def windowedSample (sample index size : Float) : Float := sample * hann index size

/-- Whether `frequency` falls in the octave band around `centre`: from centre / √2 up to centre · √2. -/
def inOctave (frequency centre : Float) : Bool :=
  frequency >= centre * 0.7071067811865476 && frequency < centre * 1.4142135623730951

/-- Band level in dB of one-sided Hann power `energy` summed over `frames` frames of `size` points, where a
full-scale sine reads 0 dB (its one-sided power per frame is 3 · size² / 32); floored at −120 dB. -/
def bandLevel (energy frames size : Float) : Float :=
  let ratio := energy * 32 / (3 * size * size * frames)
  if ratio < 0.000000000001 then -120 else 4.342944819032518 * Float.log ratio

end synth.SpectralMath
