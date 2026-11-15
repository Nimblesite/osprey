/-! # The quadratic formula of `@osprey/math`

Mathlib states the roots of `a * (x * x) + b * x + c = 0` through the discriminant
(`Mathlib/Algebra/QuadraticDiscriminant.lean`). These are the same formulas over `Float`; the
Osprey module is generated from this file ([MATH-CODE]).
-/
open Float (sqrt)

namespace math.Quadratic

/-- The discriminant of `a * (x * x) + b * x + c`. Mathlib `discrim`: `b ^ 2 - 4 * a * c`. -/
def discrim (a b c : Float) : Float := b ^ 2 - 4 * a * c

/-- The root `(-b + s) / (2 * a)`, where `s * s` is the discriminant. Mathlib
`quadratic_eq_zero_iff`: `x = (-b + s) / (2 * a) ∨ x = (-b - s) / (2 * a)`. It is NaN when the
discriminant is negative. -/
def rootAdd (a b c : Float) : Float :=
  let s := sqrt (discrim a b c)
  (-b + s) / (2 * a)

/-- The root `(-b - s) / (2 * a)`, the other disjunct of Mathlib `quadratic_eq_zero_iff`. -/
def rootSub (a b c : Float) : Float :=
  let s := sqrt (discrim a b c)
  (-b - s) / (2 * a)

end math.Quadratic
