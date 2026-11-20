/-! # Chebyshev polynomials of `@osprey/math`

The Chebyshev polynomial of the first kind `Tₙ` turns a cosine into its `n`-th harmonic, which makes
it the transfer curve of harmonic waveshaping: a full-scale sine through `Tₙ` comes out as exactly
its `n`-th partial. On `[-1, 1]` it is evaluated through Mathlib's identity `Tₙ (cos θ) = cos (n θ)`
with `θ = arccos x`, over `Float` (IEEE-754 binary64) where Mathlib has `ℝ`; outside that interval
`arccos` is undefined and the result is NaN. It carries no proof.

Mathlib is Apache-2.0 licensed; `../NOTICE.md` names the files and their authors. The Osprey
module is generated from this file ([MATH-CODE]).
-/
open Float (cos acos)

namespace math.Chebyshev

/-- `Tₙ x` for `x` in `[-1, 1]` and a whole `n`. Mathlib `Polynomial.Chebyshev.T_real_cos`:
`(T ℝ n).eval (cos θ) = cos (n * θ)`, read at `θ = arccos x`. -/
def firstKind (n x : Float) : Float := cos (n * acos x)

end math.Chebyshev
