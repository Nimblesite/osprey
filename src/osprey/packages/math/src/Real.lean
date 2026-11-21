/-! # Real-valued functions of `@osprey/math`

Each definition is the formula of the Mathlib declaration it names, written over `Float`
(IEEE-754 binary64) where Mathlib has `ℝ`, and the Mathlib source is quoted beside it. Core Lean
does not define `x⁻¹`, `√x` or `π` for `Float`, so those three are spelled `1 / x`, `sqrt x` and
`pi`; every other symbol is Mathlib's. A `Float` result is an approximation of the real value and
carries no proof.

Mathlib is Apache-2.0 licensed; `../NOTICE.md` names the files and their authors. The Osprey
module is generated from this file ([MATH-CODE]).
-/
open Float (exp log sqrt sin)

namespace math.Real

/-- The circle constant to binary64. Mathlib `Real.pi`, written `π`. -/
def pi : Float := 3.141592653589793

/-- Hyperbolic sine. Mathlib `Real.sinh_eq`: `sinh x = (exp x - exp (-x)) / 2`. -/
def sinh (x : Float) : Float := (exp x - exp (-x)) / 2

/-- Hyperbolic cosine. Mathlib `Real.cosh_eq`: `cosh x = (exp x + exp (-x)) / 2`. -/
def cosh (x : Float) : Float := (exp x + exp (-x)) / 2

/-- Hyperbolic tangent. Mathlib `Real.tanh_eq_sinh_div_cosh`: `tanh x = sinh x / cosh x`. -/
def tanh (x : Float) : Float := sinh x / cosh x

/-- Inverse hyperbolic sine. Mathlib `Real.arsinh`: `log (x + √(1 + x ^ 2))`. -/
def arsinh (x : Float) : Float := log (x + sqrt (1 + x ^ 2))

/-- The logarithm of `x` to base `b`. Mathlib `Real.logb`: `log x / log b`. -/
def logb (b x : Float) : Float := log x / log b

/-- The logistic sigmoid. Mathlib `Real.sigmoid`: `(1 + exp (-x))⁻¹`. -/
def sigmoid (x : Float) : Float := 1 / (1 + exp (-x))

/-- The summand of Shannon entropy, in nats. Mathlib `Real.negMulLog`: `- x * log x`. -/
def negMulLog (x : Float) : Float := -x * log x

/-- The entropy, in nats, of a coin that lands heads with probability `p`. Mathlib
`Real.binEntropy`: `p * log p⁻¹ + (1 - p) * log (1 - p)⁻¹`. -/
def binEntropy (p : Float) : Float := p * log (1 / p) + (1 - p) * log (1 / (1 - p))

/-- The density at `x` of the normal distribution with mean `mu` and variance `v`. Mathlib
`ProbabilityTheory.gaussianPDFReal`: `(√(2 * π * v))⁻¹ * rexp (-(x - μ) ^ 2 / (2 * v))`. -/
def gaussianPDFReal (mu v x : Float) : Float :=
  1 / sqrt (2 * pi * v) * exp (-(x - mu) ^ 2 / (2 * v))

/-- The unnormalised sinc function, the kernel of band-limited interpolation and resampling.
Mathlib `Real.sinc`: `sinc x = if x = 0 then 1 else sin x / x`. -/
def sinc (x : Float) : Float := if x == 0 then 1 else sin x / x

/-- `x` to the real power `y`, for `x > 0`: a pitch ratio `2 ^ (cents / 1200)` or a gain
`10 ^ (dB / 20)`. Mathlib `Real.rpow_def_of_pos`: `x ^ y = exp (log x * y)`. -/
def rpow (x y : Float) : Float := exp (log x * y)

end math.Real
