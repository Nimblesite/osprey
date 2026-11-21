/- Smith, Spectral Audio Signal Processing, Weighted Overlap Add, eq. (9.44).
   Each finite-support frame contributes its analysis times synthesis gain.
   See [SYNTH-DSP-PROOFS] for the source and domain. -/
namespace Synthesis

def total {α : Type} (add : α → α → α) (zero : α) (weight : α → α) : List α → α
  | [] => zero
  | gain :: rest => add (weight gain) (total add zero weight rest)

theorem weightedOverlapAdd {α : Type} (add mul : α → α → α) (zero one sample : α)
    (mulZero : mul sample zero = zero) (mulOne : mul sample one = sample)
    (distributes : ∀ a b, mul sample (add a b) = add (mul sample a) (mul sample b))
    (gains : List α) (normalized : total add zero id gains = one) :
    total add zero (mul sample) gains = sample := by
  have factor (xs : List α) :
      total add zero (mul sample) xs = mul sample (total add zero id xs) := by
    induction xs with
    | nil => exact mulZero.symm
    | cons a xs ih => exact (congrArg (add (mul sample a)) ih).trans (distributes a _).symm
  exact (factor gains).trans ((congrArg (mul sample) normalized).trans mulOne)

end Synthesis
