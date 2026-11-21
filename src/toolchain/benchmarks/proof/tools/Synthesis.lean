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

/-- Smith (5.2). On the unit circle the numerator is a unit rotation of the
    conjugate denominator. Their magnitudes agree at every frequency. -/
theorem allpassMagnitude {α β : Type} (add mul : α → α → α) (conj : α → α) (norm : α → β)
    (one coeff delay : α) (realCoeff : conj coeff = coeff) (conjOne : conj one = one)
    (conjAdd : ∀ x y, conj (add x y) = add (conj x) (conj y))
    (conjMul : ∀ x y, conj (mul x y) = mul (conj x) (conj y))
    (normConj : ∀ x, norm (conj x) = norm x)
    (unitRotation : ∀ x, norm (mul x delay) = norm x) (circle : mul (conj delay) delay = one)
    (unitL : ∀ x, mul one x = x) (unitR : ∀ x, mul x one = x)
    (assoc : ∀ x y z, mul (mul x y) z = mul x (mul y z))
    (dist : ∀ x y z, mul (add x y) z = add (mul x z) (mul y z))
    (swap : ∀ x y, add x y = add y x) :
    norm (add coeff delay) = norm (add one (mul coeff delay)) := by
  have numerator : mul (conj (add one (mul coeff delay))) delay = add coeff delay := by
    rw [conjAdd, conjOne, conjMul, realCoeff, dist, unitL, assoc, circle, unitR, swap]
  exact (congrArg norm numerator).symm.trans ((unitRotation _).trans (normConj _))

/-- Smith's difference equation immediately preceding (5.2), factored by η. -/
theorem allpassRecurrence {α : Type} (add sub mul : α → α → α) (a x prevX prevY : α)
    (dist : ∀ u v w, mul u (sub v w) = sub (mul u v) (mul u w))
    (moveSub : ∀ u v w, sub (add u v) w = add (sub u w) v) :
    sub (add (mul a x) prevX) (mul a prevY) = add (mul a (sub x prevY)) prevX := by
  rw [moveSub, dist]

/-- Parker et al., equations (8)–(9). Apply the fundamental theorem of calculus
    to the oriented integral, then reverse both differences in the quotient. -/
theorem antiderivativeAverage {α : Type} (sub div integral : α → α → α) (primitive : α → α)
    (curr prev zero : α) (distinct : sub curr prev ≠ zero)
    (fundamental : ∀ a b, integral a b = sub (primitive b) (primitive a))
    (reverse : ∀ a b x y, sub x y ≠ zero → div (sub b a) (sub y x) = div (sub a b) (sub x y)) :
    div (integral curr prev) (sub prev curr) =
      div (sub (primitive curr) (primitive prev)) (sub curr prev) := by
  rw [fundamental]
  exact reverse _ _ _ _ distinct

/-- An arbitrary additive constant in the primitive cancels from (9). -/
theorem antiderivativeOffset {α : Type} (add sub div : α → α → α) (primitive : α → α)
    (curr prev c : α) (cancel : ∀ a b k, sub (add a k) (add b k) = sub a b) :
    div (sub (add (primitive curr) c) (add (primitive prev) c)) (sub curr prev) =
    div (sub (primitive curr) (primitive prev)) (sub curr prev) := by rw [cancel]

/-- In a constant segment f(x)=g the primitive is g*x+c, so (9) returns g. -/
theorem antiderivativeConstantCurve {α : Type} (add sub mul div : α → α → α)
    (curr prev gain c zero : α) (distinct : sub curr prev ≠ zero)
    (shift : ∀ a b k, sub (add a k) (add b k) = sub a b)
    (dist : ∀ a b k, sub (mul k a) (mul k b) = mul k (sub a b))
    (cancel : ∀ k x, x ≠ zero → div (mul k x) x = k) :
    div (sub (add (mul gain curr) c) (add (mul gain prev) c)) (sub curr prev) = gain := by
  rw [shift, dist]
  exact cancel _ _ distinct

def quarterSquares {α : Type} (add sub : α → α → α) (sq : α → α) (r c s : α) :=
  add (add (sq (sub r c)) (sq (add r s))) (add (sq (add r c)) (sq (sub r s)))

def sixSquares {α : Type} (add : α → α → α) (sq : α → α) (r : α) :=
  add (add (sq r) (sq r)) (add (add (sq r) (sq r)) (add (sq r) (sq r)))

/-- Four quarter-turn shifts of a Hann window. Multiply each amplitude by two:
    (1-c), (1+s), (1+c), (1-s). The sum of their squares is six, independently
    of the phase. Keep the scalar unit r symbolic for a homogeneous identity. -/
theorem hannQuarterEnergy {α : Type} (add sub : α → α → α) (sq : α → α)
    (assoc : ∀ a b c, add (add a b) c = add a (add b c))
    (comm : ∀ a b, add a b = add b a)
    (squares : ∀ a b, add (sq (sub a b)) (sq (add a b)) =
      add (add (sq a) (sq a)) (add (sq b) (sq b)))
    (r c s : α) (circle : add (sq c) (sq s) = sq r) :
    quarterSquares add sub sq r c s = sixSquares add sq r := by
  unfold quarterSquares sixSquares
  have regroup (a b c d : α) : add (add a b) (add c d) = add (add a c) (add b d) := by
    rw [assoc a b, ← assoc b c, comm b c, assoc c b, ← assoc a c]
  rw [regroup, comm (sq (add r s)) (sq (sub r s)), squares, squares,
      regroup, regroup (sq c) (sq c) (sq s) (sq s), circle, assoc]

/-- For hop N/8, even-indexed shifts and odd-indexed shifts each form a
    quarter-turn quartet. They contribute equal energy at every phase. -/
theorem hannEighthEnergy {α : Type} (add sub : α → α → α) (sq : α → α)
    (assoc : ∀ a b c, add (add a b) c = add a (add b c))
    (comm : ∀ a b, add a b = add b a)
    (squares : ∀ a b, add (sq (sub a b)) (sq (add a b)) =
      add (add (sq a) (sq a)) (add (sq b) (sq b)))
    (r c s c' s' : α) (evenCircle : add (sq c) (sq s) = sq r)
    (oddCircle : add (sq c') (sq s') = sq r) :
    add (quarterSquares add sub sq r c s) (quarterSquares add sub sq r c' s') =
    add (sixSquares add sq r) (sixSquares add sq r) := by
  rw [hannQuarterEnergy add sub sq assoc comm squares r c s evenCircle,
      hannQuarterEnergy add sub sq assoc comm squares r c' s' oddCircle]

/-- DFT polynomial at one bin, in increasing sample order. -/
def bin {α β : Type} (add mul : α → α → α) (zero root : α) (sample : β → α) : List β → α
  | [] => zero
  | x :: xs => add (sample x) (mul root (bin add mul zero root sample xs))

/-- The same polynomial on an even-length frame, consuming consecutive pairs. -/
def pairedBin {α : Type} (add mul : α → α → α) (zero root : α) : List (α × α) → α
  | [] => zero
  | (e, o) :: xs => add e (mul root (add o (mul root (pairedBin add mul zero root xs))))

/-- Smith (A.1). Splitting even and odd powers leaves two half-size transforms
    at the squared root, with one extra root multiplying the odd transform. -/
theorem radixTwoSplit {α : Type} (add mul : α → α → α) (zero z : α)
    (plusZero : ∀ a, add a zero = a) (timesZero : ∀ a, mul a zero = zero)
    (addAssoc : ∀ a b c, add (add a b) c = add a (add b c))
    (comm : ∀ a b, add a b = add b a)
    (mulAssoc : ∀ a b c, mul (mul a b) c = mul a (mul b c))
    (dist : ∀ a b c, mul a (add b c) = add (mul a b) (mul a c))
    (frame : List (α × α)) :
    pairedBin add mul zero z frame =
      add (bin add mul zero (mul z z) Prod.fst frame)
        (mul z (bin add mul zero (mul z z) Prod.snd frame)) := by
  induction frame with
  | nil => exact ((congrArg (add zero) (timesZero z)).trans (plusZero zero)).symm
  | cons pair rest ih =>
    obtain ⟨e, o⟩ := pair
    simp only [pairedBin, bin, ih, dist, mulAssoc, addAssoc]
    rw [← addAssoc (mul z o), comm (mul z o), addAssoc]

/-- Conjugating a bin of a real frame evaluates its polynomial at the
    conjugate root. On the unit circle this is the negative-frequency bin. -/
theorem realBinConjugate {α : Type} (add mul : α → α → α) (zero z : α) (conj : α → α)
    (conjZero : conj zero = zero)
    (conjAdd : ∀ a b, conj (add a b) = add (conj a) (conj b))
    (conjMul : ∀ a b, conj (mul a b) = mul (conj a) (conj b))
    (frame : List α) (realInput : ∀ x, x ∈ frame → conj x = x) :
    conj (bin add mul zero z id frame) = bin add mul zero (conj z) id frame := by
  induction frame with
  | nil => exact conjZero
  | cons x xs ih =>
    change conj (add x (mul z _)) = add x (mul (conj z) _)
    rw [conjAdd, conjMul, realInput x (.head xs), ih (fun y h => realInput y (.tail x h))]

/-- Smith's upper butterfly. Advancing k by half a frame negates z and
    preserves its square, so both bins share the same two subtransforms. -/
theorem radixTwoUpper {α : Type} (add mul : α → α → α) (neg : α → α) (zero z : α)
    (plusZero : ∀ a, add a zero = a) (timesZero : ∀ a, mul a zero = zero)
    (addAssoc : ∀ a b c, add (add a b) c = add a (add b c))
    (comm : ∀ a b, add a b = add b a)
    (mulAssoc : ∀ a b c, mul (mul a b) c = mul a (mul b c))
    (dist : ∀ a b c, mul a (add b c) = add (mul a b) (mul a c))
    (negSquare : ∀ a, mul (neg a) (neg a) = mul a a)
    (negMul : ∀ a b, mul (neg a) b = neg (mul a b)) (frame : List (α × α)) :
    pairedBin add mul zero (neg z) frame =
      add (bin add mul zero (mul z z) Prod.fst frame)
        (neg (mul z (bin add mul zero (mul z z) Prod.snd frame))) := by
  rw [radixTwoSplit add mul zero (neg z) plusZero timesZero addAssoc comm mulAssoc dist, negSquare, negMul]

/-- DFT superposition, addition part. Distribute each twiddle over the two
    contributions, then collect terms from the same input signal. -/
theorem binAdditive {α ι : Type} (add mul : α → α → α) (zero z : α)
    (plusZero : ∀ a, add a zero = a) (assoc : ∀ a b c, add (add a b) c = add a (add b c))
    (comm : ∀ a b, add a b = add b a)
    (dist : ∀ a b c, mul a (add b c) = add (mul a b) (mul a c))
    (x y : ι → α) (indices : List ι) :
    bin add mul zero z (fun i => add (x i) (y i)) indices =
      add (bin add mul zero z x indices) (bin add mul zero z y indices) := by
  induction indices with
  | nil => exact (plusZero zero).symm
  | cons i rest ih =>
    simp only [bin, ih, dist, assoc]
    rw [← assoc (y i), comm (y i), assoc]

/-- DFT superposition, scaling part. The constant commutes with the twiddle
    and factors out of the complete finite sum. -/
theorem binHomogeneous {α ι : Type} (add mul : α → α → α) (zero z gain : α)
    (timesZero : ∀ a, mul a zero = zero) (comm : ∀ a b, mul a b = mul b a)
    (assoc : ∀ a b c, mul (mul a b) c = mul a (mul b c))
    (dist : ∀ a b c, mul a (add b c) = add (mul a b) (mul a c))
    (x : ι → α) (indices : List ι) :
    bin add mul zero z (fun i => mul gain (x i)) indices =
      mul gain (bin add mul zero z x indices) := by
  induction indices with
  | nil => exact (timesZero gain).symm
  | cons i rest ih =>
    simp only [bin, ih, dist]
    rw [← assoc z gain, comm z gain, assoc]

end Synthesis
