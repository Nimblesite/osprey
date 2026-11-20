/-! Compressor and limiter gain laws [SYNTH-DYNAMICS].

The compressor is the feed-forward, log-domain design of Giannoulis, Massberg and Reiss ("Digital Dynamic
Range Compressor Design", JAES 2012): a soft-knee static curve, then a smooth decoupled detector on the
gain reduction, with a program-dependent release. The limiter measures true peak by four-times
oversampling (ITU-R BS.1770-4 Annex 2), holds the lowest required gain across its lookahead and averages
it over the same window, so the gain has fully arrived when the peak leaves the delay. -/
namespace synth.DynamicsMath

private def lower (one other : Float) : Float := if one < other then one else other
private def higher (one other : Float) : Float := if one > other then one else other

/-- Level in decibels of a linear amplitude: `20 log10 |x|`, floored at −180 dB. -/
def decibels (amplitude : Float) : Float :=
  8.685889638065035 * Float.log (higher (Float.abs amplitude) 0.000000001)
/-- Linear gain of a decibel value: `10 ^ (dB / 20)`. -/
def linear (level : Float) : Float := Float.exp (level * 0.1151292546497023)

/-- One-pole coefficient that covers 63% of a step in `seconds` at `rate`; zero time is instant. -/
def coefficient (seconds rate : Float) : Float :=
  if seconds <= 0 then 0 else Float.exp (-1 / (seconds * rate))

/-- Mean square of the signal for an RMS detector. -/
def meanSquare (previous sample alpha : Float) : Float :=
  alpha * previous + (1 - alpha) * sample * sample
/-- RMS level in decibels from a mean square: `10 log10`. -/
def rmsLevel (square : Float) : Float :=
  4.342944819032518 * Float.log (higher square 0.000000000000000001)
/-- Linked detection: every channel is reduced by the loudest channel's amount, so the image holds. -/
def linked (one other : Float) : Float := higher one other
/-- Detector level between peak (`blend` 0) and RMS (`blend` 1), in decibels. -/
def detected (peak rms blend : Float) : Float := peak + (rms - peak) * blend

/-- Static curve: output level for input `level`, with `threshold` (dB), `ratio` (≥ 1) and `knee` width (dB).
Below the knee the level is unchanged, above it the slope is `1 / ratio`, and inside it a quadratic joins
the two with a continuous slope. -/
def curve (level threshold ratio knee : Float) : Float :=
  let over := level - threshold
  if 2 * over < -knee then level
  else if 2 * over <= knee && knee > 0 then
    level + (1 / ratio - 1) * (over + knee / 2) * (over + knee / 2) / (2 * knee)
  else threshold + over / ratio
/-- Gain reduction the curve asks for, in decibels (never positive). -/
def reduction (level threshold ratio knee : Float) : Float :=
  curve level threshold ratio knee - level
/-- Downward expansion below `threshold` for a gate: slope `ratio` under the threshold, hard knee. -/
def expansion (level threshold ratio : Float) : Float :=
  if level >= threshold then 0 else (level - threshold) * (ratio - 1)

/-- Smooth decoupled detector on the reduction (dB): more reduction moves at the attack coefficient,
less at the release coefficient. -/
def smoothed (previous target attack release : Float) : Float :=
  if target < previous then attack * previous + (1 - attack) * target
  else release * previous + (1 - release) * target
/-- Program-dependent release: a fast and a slow detector run together and the deeper one wins, so a
single transient recovers fast while dense material recovers slowly and does not pump. -/
def programmed (fast slow : Float) : Float := lower fast slow

/-- Make-up gain (dB) that restores the level a 0 dB signal lost, scaled by `amount` in [0, 1]. -/
def makeup (threshold ratio knee amount : Float) : Float :=
  -(reduction 0 threshold ratio knee) * amount
/-- Parallel compression: `mix` 0 is the dry signal, 1 the compressed one. -/
def blended (dry wet mix : Float) : Float := dry + (wet - dry) * mix

/-- Prototype tap `position` (from 0) of the `phases`-times oversampling filter with `taps` taps per
phase: a Hann-windowed sinc with its cutoff at the original Nyquist frequency. -/
def oversampleTap (position taps phases : Float) : Float :=
  let last := taps * phases - 1
  let x := (position - last / 2) / phases
  let window := 0.5 - 0.5 * Float.cos (6.283185307179586 * position / last)
  if x == 0 then window else window * Float.sin (3.141592653589793 * x) / (3.141592653589793 * x)
/-- The position in the prototype of tap `index` of `phase`. -/
def tapPosition (index phase phases : Float) : Float := index * phases + phase
/-- True peak so far: the largest magnitude among the oversampled values. -/
def truePeak (peak value : Float) : Float := higher peak (Float.abs value)

/-- Lookahead in whole samples for `milliseconds` at `rate`, at least one. -/
def lookahead (milliseconds rate : Float) : Float :=
  higher 1 (Float.ceil (milliseconds * rate / 1000))
/-- Gain (≤ 1) that brings `peak` to `ceiling` (linear); a peak under the ceiling needs none. -/
def required (peak ceiling : Float) : Float := if peak <= ceiling then 1 else ceiling / peak
/-- Hold: the lowest required gain across the lookahead window. -/
def held (lowest gain : Float) : Float := lower lowest gain
/-- Moving average over `window` samples, updated from the value entering and the one leaving. Averaging
the held gain over the lookahead reaches the held value exactly when the peak arrives: no overshoot. -/
def averaged (previous entering leaving window : Float) : Float :=
  previous + (entering - leaving) / window
/-- Release of the held gain: a drop is taken at once (the lookahead already shaped it), a rise follows
the release coefficient. -/
def released (previous target release : Float) : Float :=
  if target <= previous then target else release * previous + (1 - release) * target
/-- The last guard: nothing leaves above the ceiling, whatever the arithmetic before it did. -/
def ceilinged (sample ceiling : Float) : Float :=
  if sample > ceiling then ceiling else if sample < -ceiling then -ceiling else sample
/-- Multiply-accumulate: one filter tap applied to one sample, added to the running sum. -/
def accumulated (sum tap sample : Float) : Float := sum + tap * sample

/-- One limited output sample: the input at its gain, clamped to the ceiling. -/
def limitedSample (sample gain ceiling : Float) : Float := ceilinged (sample * gain) ceiling

end synth.DynamicsMath
