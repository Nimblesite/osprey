# @osprey/math

Well-known formulas as functions. **The source is mathematics, written in Lean, not Osprey code**: each function is a `def` in a `.lean` file, the formula is the one [Mathlib](https://github.com/leanprover-community/mathlib4) states, and the Osprey module is generated from it on every build ([Math to code](../../../../docs/specs/tooling/MathToCode.md)).

| Call | Formula | Mathlib declaration |
| --- | --- | --- |
| `Real::pi` | 3.141592653589793 | `Real.pi` |
| `Real::sinh x` | `(exp x - exp (-x)) / 2` | `Real.sinh_eq` |
| `Real::cosh x` | `(exp x + exp (-x)) / 2` | `Real.cosh_eq` |
| `Real::tanh x` | `sinh x / cosh x` | `Real.tanh_eq_sinh_div_cosh` |
| `Real::arsinh x` | `log (x + √(1 + x ^ 2))` | `Real.arsinh` |
| `Real::logb (b, x)` | `log x / log b` | `Real.logb` |
| `Real::sigmoid x` | `(1 + exp (-x))⁻¹` | `Real.sigmoid` |
| `Real::negMulLog x` | `- x * log x` | `Real.negMulLog` |
| `Real::binEntropy p` | `p * log p⁻¹ + (1 - p) * log (1 - p)⁻¹` | `Real.binEntropy` |
| `Real::gaussianPDFReal (mu, v, x)` | `(√(2 * π * v))⁻¹ * exp (-(x - μ) ^ 2 / (2 * v))` | `ProbabilityTheory.gaussianPDFReal` |
| `Quadratic::discrim (a, b, c)` | `b ^ 2 - 4 * a * c` | `discrim` |
| `Quadratic::rootAdd (a, b, c)`, `rootSub` | `(-b ± s) / (2 * a)` with `s * s` the discriminant | `quadratic_eq_zero_iff` |
| `AffineMap::lineMap (a, b, c)` | `c * (b - a) + a` | `AffineMap.lineMap_apply_ring'` |
| `Real::sinc x` | `if x = 0 then 1 else sin x / x` | `Real.sinc` |
| `Real::rpow (x, y)`, `x > 0` | `exp (log x * y)` | `Real.rpow_def_of_pos` |
| `Chebyshev::firstKind (n, x)`, `x` in `[-1, 1]` | `cos (n * arccos x)`, from `Tₙ (cos θ) = cos (n θ)` | `Polynomial.Chebyshev.T_real_cos` |

```osprey-ml
import math::Real
import math::Quadratic

main () =
    print "${Real::logb (2.0, 8.0)} ${Real::sigmoid 0.0} ${Quadratic::rootAdd (1.0, -3.0, 2.0)}"
```

For audio synthesis: `sinc` is the kernel of band-limited interpolation and resampling, `rpow` gives equal-tempered pitch ratios (`rpow (2.0, cents / 1200.0)`) and decibel gains (`rpow (10.0, dB / 20.0)`), and `firstKind` is the transfer curve of harmonic waveshaping (a full-scale sine through `Tₙ` comes out as its `n`-th partial).

The sources are [src/Real.lean](src/Real.lean), [src/Quadratic.lean](src/Quadratic.lean), [src/AffineMap.lean](src/AffineMap.lean) and [src/Chebyshev.lean](src/Chebyshev.lean). To add a formula, add a `def` there; `osprey math src/osprey/packages/math/src/Real.lean` prints the Osprey it generates.

**What is and is not claimed.** The functions compute over `Float` (IEEE-754 binary64), where Mathlib's are over `ℝ`: a result approximates the real value, rounds, and can be infinite or NaN (`rootAdd` of a negative discriminant is NaN). The pinned Lean accepts every source and computes the same bits as the generated module on a grid of points (`make -C src/osprey/examples/proof math`). No theorem of Mathlib about these functions is proved here for `Float`.

Run `python3 scripts/packages/package_coverage.py --collect --compiler target/release/osprey --package math --reports target/coverage/packages --thresholds src/osprey/tests/coverage-thresholds.json` for the package tests and its coverage gate. Licensing: [NOTICE.md](NOTICE.md).
