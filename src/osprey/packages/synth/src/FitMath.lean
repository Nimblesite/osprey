/-! Offline parameter-fitting mathematics [SYNTH-ADVANCED-LEARNED].

The objective is the mean squared difference of log power spectra, which is multimodal in
pitch. The first quarter of the budget therefore samples uniformly (global exploration); the
rest is a (1+1) evolution strategy with Rechenberg's one-fifth success rule (expand by 1.5 on success,
contract by 0.82 on failure), on parameters normalised to `[0, 1]`.
-/
namespace synth.FitMath

/-- Log power with a −90 dB floor, so silent bins stay finite. -/
def logPower (power : Float) : Float := Float.log (power + 0.000000001)

/-- Squared log-spectral difference of one bin. -/
def binLoss (candidate target : Float) : Float :=
  let d := logPower candidate - logPower target
  d * d

/-- A normalised value moved by a symmetric uniform step and clipped to `[0, 1]`. -/
def perturbed (value step seed : Float) : Float :=
  let moved := value + step * (2 * ((seed - 1) / 2147483646) - 1)
  if moved < 0 then 0 else if moved > 1 then 1 else moved

/-- A uniform draw in `[0, 1]` from a Park–Miller state, for the global exploration phase. -/
def explored (seed : Float) : Float := (seed - 1) / 2147483646

/-- Map between a parameter's range and `[0, 1]`. -/
def normalised (value low high : Float) : Float := (value - low) / (high - low)
def denormalised (value low high : Float) : Float := low + value * (high - low)

/-- Clip a value into `[low, high]`. -/
def clip (value low high : Float) : Float := if value < low then low else if value > high then high else value

/-- One-fifth-rule step adaptation. -/
def adapted (step improved : Float) : Float := if improved > 0 then step * 1.5 else step * 0.82

/-- A macro amount in [0, 1] shaped by `exponent` and mapped onto `[low, high]`. -/
def macroValue (amount low high exponent : Float) : Float := low + (high - low) * Float.pow amount exponent

/-- Logarithmic control mapping between positive bounds: equal control steps are equal ratios. -/
def logarithmic (control low high : Float) : Float := low * Float.pow (high / low) control
def logarithmicControl (value low high : Float) : Float := Float.log (value / low) / Float.log (high / low)

end synth.FitMath
