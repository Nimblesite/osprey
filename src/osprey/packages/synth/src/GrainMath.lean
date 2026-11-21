/-! Sample-grain mathematics [SYNTH-ADVANCED-GRAINS]. -/
namespace synth.GrainMath
/-- Periodic source addressing also supports negative playback rates. -/
def wrapped (position size : Float) : Float :=
  let value := position - Float.floor (position / size) * size
  if value >= size then 0 else value
/-- Equal-power stereo placement for pan in [-1, 1]. -/
def leftGain (pan : Float) : Float := Float.cos ((pan + 1) * 0.7853981633974483)
def rightGain (pan : Float) : Float := Float.sin ((pan + 1) * 0.7853981633974483)
def cursor (position rate size : Float) : Float := wrapped (position + rate) size
/-- Linear ASR window. Positive fractions partition the lifetime; both endpoints are exactly zero. -/
def asr (age duration attack release : Float) : Float :=
  if age <= 0 || age >= duration - 1 then 0
  else if age < attack * (duration - 1) then age / (attack * (duration - 1))
  else if age > (1 - release) * (duration - 1) then (duration - 1 - age) / (release * (duration - 1))
  else 1
def weighted (sample amplitude envelope : Float) : Float := sample * amplitude * envelope
def placed (sample gain : Float) : Float := sample * gain
def fraction (position : Float) : Float := position - Float.floor position
def summed (a b : Float) : Float := a + b
/-- Park–Miller state update with multiplier 48271; all integer products fit binary64 exactly. -/
def nextSeed (seed : Float) : Float :=
  let product := seed * 48271
  product - Float.floor (product / 2147483647) * 2147483647
/-- Inclusive integer inter-onset jitter; zero jitter is strictly periodic. -/
def interval (period jitter seed : Float) : Float :=
  period + Float.floor ((seed - 1) / 2147483646 * (2 * jitter + 1)) - jitter
/-- Discrete censored exponential gap: inverse CDF, rounded up, then capped. Never a zero gap. -/
def exponentialInterval (mean maximum seed : Float) : Float :=
  let gap := Float.ceil (-mean * Float.log (1 - seed / 2147483647))
  if gap < 1 then 1 else if gap > maximum then maximum else gap
/-- Lifetime-average events per rendered frame; no elapsed frames means no measured density yet. -/
def density (events frames : Float) : Float := if frames <= 0 then 0 else events / frames
/-- Uniform bounded values; equal endpoints hold a parameter while its stream still advances. -/
def uniform (low high seed : Float) : Float := low + (high - low) * ((seed - 1) / 2147483646)
/-- Inclusive discrete uniform range, with finite generator quantisation bias. -/
def discrete (low high seed : Float) : Float := low + Float.floor ((high - low + 1) * ((seed - 1) / 2147483646))
/-- Offset stream initialization stays in the nonzero Park–Miller state domain. -/
def streamSeed (seed offset : Float) : Float :=
  let value := seed - 1 + offset
  1 + value - Float.floor (value / 2147483646) * 2147483646
/-- Clip symmetric parameter spreads to the declared valid control domain. -/
def spreadLow (center spread low : Float) : Float := if center - spread < low then low else center - spread
def spreadHigh (center spread high : Float) : Float := if center + spread > high then high else center + spread
/-- Advance an absolute fractional sample address, wrapping into [first, limit) when requested. -/
def regionCursor (position rate first limit : Float) (looping : Bool) : Float :=
  if looping then
    let next := first + cursor (position - first) rate (limit - first)
    if next >= limit then first else next
  else position + rate
end synth.GrainMath
