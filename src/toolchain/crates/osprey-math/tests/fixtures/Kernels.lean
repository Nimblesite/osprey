/-! Band-limited oscillator kernels: the fragment's constructs in one source. -/
namespace synth.Kernels

/-- Fractional part. -/
def wrap (phase : Float) : Float := phase - Float.floor phase

/-- Polynomial band-limited step correction, one sample either side. -/
def blep (phase step : Float) : Float :=
  if step ≤ 0 then 0
  else if phase < step then
    let x := phase / step
    x + x - x * x - 1
  else if phase > 1 - step then
    let x := (phase - 1) / step
    x * x + x + x + 1
  else 0

private def tau : Float := 6.283185307179586

/-- One cycle of a sine per unit phase. -/
def sine (phase : Float) : Float := Float.sin (tau * phase)

def sq (x : Float) : Float := x ^ 2

def inside (x y : Float) (strict : Bool) : Bool :=
  x < y ∧ y < 1 || x == y && !strict

def shifted (x : Float) : Float := -x + 1e-3 + 2.5e2 - -x ^ 2

private def half (x : Float) : Float :=
  let y : Float := x / 2; y.sqrt + (x * 2).abs

def pick (x : Float) : Float :=
  let big := if x ≥ 1 then x.log else Float.atan2 x 2
  let small := half big
  if inside small big true then synth.Kernels.sq small else wrap (x.pow 0.5)

end synth.Kernels

#eval synth.Kernels.pick 3
#guard synth.Kernels.sq 3 == 9
