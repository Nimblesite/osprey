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

/-- Smith (9.44), necessity. Reconstruct the constant unit signal to recover
    the sum of the overlapping window products. -/
theorem overlapNormalizationNecessary {α : Type} (add mul : α → α → α) (zero one : α)
    (unit : ∀ x, mul one x = x) (gains : List α)
    (reconstruct : ∀ x, total add zero (mul x) gains = x) :
    total add zero id gains = one := by
  have unchanged (xs : List α) : total add zero (mul one) xs = total add zero id xs := by
    induction xs with
    | nil => rfl
    | cons g gs ih =>
      change add (mul one g) _ = add g _
      rw [unit, ih]
  exact (unchanged gains).symm.trans (reconstruct one)

/-- Smith (5.1) factored into the multiply used by GrainMath.interpolated.
    Move the common product to the right by distributivity and commutativity. -/
theorem interpolatorFactor {α : Type} (add sub mul : α → α → α) (one a b t : α)
    (rightSub : ∀ x y z, mul (sub x y) z = sub (mul x z) (mul y z))
    (unit : ∀ x, mul one x = x) (comm : ∀ x y, mul x y = mul y x)
    (moveSub : ∀ x y z, add (sub x y) z = add x (sub z y)) :
    add (mul (sub one t) a) (mul t b) = add a (mul (sub b a) t) := by
  rw [rightSub, unit, moveSub, comm t b, comm t a, rightSub]

/-- The one-multiply sample reader returns each neighbour at its own endpoint. -/
theorem interpolatorEndpoints {α : Type} (add sub mul : α → α → α) (zero one a b : α)
    (timesZero : ∀ x, mul x zero = zero) (timesOne : ∀ x, mul x one = x)
    (plusZero : ∀ x, add x zero = x) (cancel : ∀ x y, add x (sub y x) = y) :
    add a (mul (sub b a) zero) = a ∧ add a (mul (sub b a) one) = b := by
  rw [timesZero, plusZero, timesOne, cancel]
  exact ⟨rfl, rfl⟩

/-- Constant input is the zero-slope case of Smith's exactness on a line. -/
theorem interpolatorConstant {α : Type} (add sub mul : α → α → α) (zero a t : α)
    (selfSub : ∀ x, sub x x = zero) (zeroTimes : ∀ x, mul zero x = zero)
    (plusZero : ∀ x, add x zero = x) :
    add a (mul (sub a a) t) = a := by rw [selfSub, zeroTimes, plusZero]

/-- Smart (8)–(9). In either branch the modulation pulse consists of a
    window times the index times the sinusoid evaluated at that branch's phase. -/
theorem zeroPulseIndices {α : Type} (add mul : α → α → α) (zero : α)
    (rightZero : ∀ x, mul x zero = zero) (leftZero : ∀ x, mul zero x = zero)
    (addZero : ∀ x, add x zero = x) (first : Bool)
    (base₁ base₂ window₁ window₂ sine₁ sine₂ : α) :
    (if first then add base₁ (mul (mul window₁ zero) sine₁)
     else add base₂ (mul (mul window₂ zero) sine₂)) =
    (if first then base₁ else base₂) := by
  rw [rightZero, rightZero, leftZero, leftZero, addZero, addZero]

/-- The endpoint phases of the symmetric grain window are zero and one full
    turn. Substitute cos(0) = cos(2π) = 1 in the uncentred raised cosine. -/
theorem hannEndpoints {α : Type} (sub mul : α → α → α) (cos : α → α)
    (zero one half turn : α) (cosZero : cos zero = one) (cosTurn : cos turn = one)
    (timesOne : ∀ x, mul x one = x) (selfSub : ∀ x, sub x x = zero) :
    sub half (mul half (cos zero)) = zero ∧ sub half (mul half (cos turn)) = zero := by
  rw [cosZero, cosTurn, timesOne, selfSub]
  exact ⟨rfl, rfl⟩

end Synthesis
