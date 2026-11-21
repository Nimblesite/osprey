/- Smith, Spectral Audio Signal Processing, Weighted Overlap Add, eq. (9.44).
   Each finite-support frame contributes its analysis times synthesis gain.
   See [SYNTH-DSP-PROOFS] for the source and domain. -/
namespace Synthesis

def total {α β : Type} (add : α → α → α) (zero : α) (weight : β → α) : List β → α
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

/-- Smith (7.3), with the spatial interval chosen as one time step of travel.
    Walking m sites indexes the two rails by m earlier or later samples. -/
def walk {ι : Type} (step : ι → ι) : Nat → ι → ι
  | 0, time => time
  | m + 1, time => walk step m (step time)

theorem travelingWavePropagation {ι α : Type} (prev next : ι → ι)
    (prevNext : ∀ t, prev (next t) = t) (right left : ι → α) (t : ι) (m : Nat) :
    right (walk prev (m+1) (next t)) = right (walk prev m t) ∧
    left (walk next m (next t)) = left (walk next (m+1) t) :=
  ⟨congrArg (fun u => right (walk prev m u)) (prevNext t), rfl⟩

/-- Puckette, Periodic Signals. Sum the partials at the shifted time and use
    each partial's common period to recover the original finite Fourier sum. -/
theorem additivePeriod {ι α : Type} (add : α → α → α) (dc : α)
    (advance : ι → ι) (partials : List (ι → α)) (t : ι)
    (periodic : ∀ p, p ∈ partials → p (advance t) = p t) :
    total add dc (fun p => p (advance t)) partials = total add dc (fun p => p t) partials := by
  induction partials with
  | nil => rfl
  | cons p ps ih =>
    change add (p (advance t)) _ = add (p t) _
    rw [periodic p (.head ps), ih (fun q h => periodic q (.tail p h))]

end Synthesis
