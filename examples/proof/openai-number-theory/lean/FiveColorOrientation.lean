/- Port of OpenAI math 7ef5c0cc5ca4b0e072b7a050fca09ab24bc6fa3a.
   Original: lean/OAI/NumberTheory/TwoPoint/Walks/FiveColorOrientation.lean
   Apache-2.0; definitions and statements retained exactly. Mathlib imports
   (Mathlib.Data.Int.DivMod, Mathlib.Tactic) narrowed to core Lean: every
   lemma and tactic used is core's; `norm_num` on `(5 : ℤ) ≠ 0` and
   `0 < (5 : ℤ)` is replaced by `decide`; `lemma` (Mathlib syntax) is
   spelled `theorem` and `ℤ` (Mathlib notation) `Int`, which elaborate to
   the same declarations. -/
set_option autoImplicit false

namespace OAI

/-! Five colors separate the two orientations of a short interval of shifts. -/

namespace TwoPointCorrelations

def intervalColor (D x : Int) : Int := (x / D) % 5

def forwardColorPair (D x y : Int) : Prop :=
  (intervalColor D y - intervalColor D x) % 5 = 1 ∨
    (intervalColor D y - intervalColor D x) % 5 = 2

theorem short_shift_quotient_gap (D x y : Int) (hD : 0 < D)
    (hlow : D ≤ y - x) (hupp : y - x < 2 * D) :
    y / D - x / D = 1 ∨ y / D - x / D = 2 := by
  have h1 := Int.ediv_le_ediv hD (show x + 1 * D ≤ y by omega)
  have h2 := Int.ediv_le_ediv hD (show y ≤ x + 2 * D by omega)
  rw [Int.add_mul_ediv_right _ _ (Int.ne_of_gt hD)] at h1 h2
  omega

theorem short_shift_forward_colors (D x y : Int) (hD : 0 < D)
    (hlow : D ≤ y - x) (hupp : y - x < 2 * D) : forwardColorPair D x y := by
  have hg := short_shift_quotient_gap D x y hD hlow hupp
  unfold forwardColorPair intervalColor
  omega

theorem short_shift_not_reverse_colors (D x y : Int) (hD : 0 < D)
    (hlow : D ≤ y - x) (hupp : y - x < 2 * D) : ¬forwardColorPair D y x := by
  have hg := short_shift_quotient_gap D x y hD hlow hupp
  unfold forwardColorPair intervalColor
  omega

theorem intervalColor_bounds (D x : Int) : 0 ≤ intervalColor D x ∧ intervalColor D x < 5 := by
  exact ⟨Int.emod_nonneg _ (by decide), Int.emod_lt_of_pos _ (by decide)⟩

/-- Once a pair of endpoint colors has been fixed, the symmetric graph's
reverse edge cannot appear in its forward bilinear test. -/
theorem five_color_isolates_forward (D x y : Int) (hD : 0 < D)
    (hcolors : forwardColorPair D x y)
    (hedge : (D ≤ y - x ∧ y - x < 2 * D) ∨ (D ≤ x - y ∧ x - y < 2 * D)) :
    D ≤ y - x ∧ y - x < 2 * D := by
  rcases hedge with h | h
  · exact h
  · exact (short_shift_not_reverse_colors D y x hD h.1 h.2 hcolors).elim

end TwoPointCorrelations

end OAI
