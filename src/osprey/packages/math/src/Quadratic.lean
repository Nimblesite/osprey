/-! # The quadratic formula of `@osprey/math`

Mathlib states the roots of `a * (x * x) + b * x + c = 0` through the discriminant
(`Mathlib/Algebra/QuadraticDiscriminant.lean`). These are the same formulas over `Float`; the
Osprey module is generated from this file ([MATH-CODE]).
-/
open Float (sqrt)

namespace math.Quadratic

/-- The discriminant of `a * (x * x) + b * x + c`. Mathlib `discrim`: `b ^ 2 - 4 * a * c`.
# Examples
```osprey
print "${Quadratic::discrim (1.0, -3.0, 2.0)} ${Quadratic::discrim (1.0, 0.0, 1.0)}"
```
```output
1.0 -4.0
```
-/
def discrim (a b c : Float) : Float := b ^ 2 - 4 * a * c

/-- The root `(-b + s) / (2 * a)`, where `s * s` is the discriminant. Mathlib
`quadratic_eq_zero_iff`: `x = (-b + s) / (2 * a) ∨ x = (-b - s) / (2 * a)`. It is NaN when the
discriminant is negative.
# Examples
```osprey
print "${Quadratic::rootAdd (1.0, -3.0, 2.0)}"
```
```output
2.0
```
-/
def rootAdd (a b c : Float) : Float :=
  let s := sqrt (discrim a b c)
  (-b + s) / (2 * a)

/-- The root `(-b - s) / (2 * a)`, the other disjunct of Mathlib `quadratic_eq_zero_iff`.
# Examples
```osprey
print "${Quadratic::rootSub (1.0, -3.0, 2.0)}"
```
```output
1.0
```
-/
def rootSub (a b c : Float) : Float :=
  let s := sqrt (discrim a b c)
  (-b - s) / (2 * a)

end math.Quadratic
