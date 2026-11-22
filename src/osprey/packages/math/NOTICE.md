# @osprey/math: licensing and upstream acknowledgement

The Osprey implementation is provided under the normal [MIT license](LICENSE), Copyright (c) 2026 NIMBLESITE PTY LTD, Australia. This does not claim ownership of any third-party material identified below. The package's license expression is `MIT AND Apache-2.0`.

## Specifically retained third-party material

`src/Real.lean`, `src/Quadratic.lean`, `src/AffineMap.lean` and `src/Chebyshev.lean` restate definitions and lemmas of [Mathlib](https://github.com/leanprover-community/mathlib4) at commit `1ccd71f89cbbd82ae7d097723ce1722ca7b01c33`, and quote each one's source line beside the restatement. Mathlib is released under the Apache License 2.0: see [`LICENSES/mathlib4-Apache-2.0.txt`](LICENSES/mathlib4-Apache-2.0.txt). The change in every case is the number type, `Float` (IEEE-754 binary64) in place of `ℝ`, with `1 / x`, `sqrt x` and `pi` written where Mathlib writes `x⁻¹`, `√x` and `π`.

| Function | Mathlib declaration | Mathlib file | Notice in that file |
| --- | --- | --- | --- |
| `Quadratic::discrim`, `rootAdd`, `rootSub` | `discrim`, `quadratic_eq_zero_iff` | `Mathlib/Algebra/QuadraticDiscriminant.lean` | Copyright (c) 2019 Zhouhang Zhou |
| `Real::logb` | `Real.logb` | `Mathlib/Analysis/SpecialFunctions/Log/Base.lean` | Copyright (c) 2022 Bolton Bailey |
| `Real::arsinh` | `Real.arsinh` | `Mathlib/Analysis/SpecialFunctions/Arsinh.lean` | Copyright (c) 2020 James Arthur |
| `Real::negMulLog` | `Real.negMulLog` | `Mathlib/Analysis/SpecialFunctions/Log/NegMulLog.lean` | Copyright (c) 2023 Rémy Degenne |
| `Real::binEntropy` | `Real.binEntropy` | `Mathlib/Analysis/SpecialFunctions/BinaryEntropy.lean` | Copyright (c) 2023 Adomas Baliuka |
| `Real::gaussianPDFReal` | `ProbabilityTheory.gaussianPDFReal` | `Mathlib/Probability/Distributions/Gaussian/Real.lean` | Copyright (c) 2023 Rémy Degenne |
| `Real::sigmoid` | `Real.sigmoid` | `Mathlib/Analysis/SpecialFunctions/Sigmoid.lean` | Copyright (c) 2025 Gaëtan Serré |
| `Real::sinh`, `cosh`, `tanh` | `Real.sinh_eq`, `Real.cosh_eq`, `Real.tanh_eq_sinh_div_cosh` | `Mathlib/Analysis/Complex/Trigonometric.lean` | Copyright (c) 2018 Chris Hughes |
| `AffineMap::lineMap` | `AffineMap.lineMap_apply_ring'` | `Mathlib/LinearAlgebra/AffineSpace/AffineMap.lean` | Copyright (c) 2020 Joseph Myers |
| `Real::sinc` | `Real.sinc` | `Mathlib/Analysis/SpecialFunctions/Trigonometric/Sinc.lean` | Copyright (c) 2025 Rémy Degenne |
| `Real::rpow` | `Real.rpow_def_of_pos` | `Mathlib/Analysis/SpecialFunctions/Pow/Real.lean` | Copyright (c) 2018 Chris Hughes |
| `Chebyshev::firstKind` | `Polynomial.Chebyshev.T_real_cos` | `Mathlib/Analysis/SpecialFunctions/Trigonometric/Chebyshev.lean` | Copyright (c) 2020 Johan Commelin |

Each Mathlib file is "Released under Apache 2.0 license as described in the file LICENSE" and lists its authors in its header.

## License scope

The Apache License 2.0 applies to the three `.lean` sources named above. The tests, manifests and documentation of this package are original and MIT licensed. No Mathlib proof, tactic script or other source is included.

## Distribution

The publisher carries `LICENSE`, this acknowledgement and the `LICENSES/` file in the digest-pinned release. Source or compiled redistributions containing the specifically retained material must carry its applicable notices. Upstream project names provide acknowledgement and imply no endorsement.
