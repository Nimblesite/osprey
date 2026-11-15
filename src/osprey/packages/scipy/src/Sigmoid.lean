/-! # The logistic sigmoid of `@osprey/scipy`

`scipy.special.expit` is mathematics, so its source is this formula. The Osprey module is
generated from this file ([MATH-CODE]); `logit` and `log_expit` stay in `logistic.py` until the
series they call are read from Lean too.
-/
open Float (exp)

namespace scipy.Sigmoid

/-- `scipy.special.expit(x)`: the logistic sigmoid `1 / (1 + exp (-x))`. It is 1 at `+∞`, 0 at
`-∞` and NaN at NaN, as scipy's is. -/
def expit (x : Float) : Float := 1 / (1 + exp (-x))

end scipy.Sigmoid
