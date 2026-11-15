/-! # Scalar functions of `@osprey/micrograd`

micrograd's engine names its own operations `exp` and `tanh`, after `Value.exp` and
`Value.tanh`, so the scalar mathematics those operations compute lives here under other names.
The Osprey module is generated from this file ([MATH-CODE]).
-/
namespace micrograd.Libm

/-- `e` raised to `x`. -/
def exp_of (x : Float) : Float := Float.exp x

/-- micrograd's tanh, the lecture's `(exp(2x) - 1) / (exp(2x) + 1)`, not the C library's. -/
def tanh_of (x : Float) : Float :=
  let e := Float.exp (2 * x)
  (e - 1) / (e + 1)

end micrograd.Libm
