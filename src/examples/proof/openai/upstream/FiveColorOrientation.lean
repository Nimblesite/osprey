import Mathlib.Data.Int.DivMod
import Mathlib.Tactic

namespace OAI

/-! Five colors separate the two orientations of a short interval of shifts. -/

namespace TwoPointCorrelations

def intervalColor (D x : ℤ) : ℤ := (x / D) % 5

def forwardColorPair (D x y : ℤ) : Prop :=
  (intervalColor D y - intervalColor D x) % 5 = 1 ∨
    (intervalColor D y - intervalColor D x) % 5 = 2

lemma short_shift_quotient_gap (D x y : ℤ) (hD : 0 < D)
    (hlow : D ≤ y - x) (hupp : y - x < 2 * D) :
    y / D - x / D = 1 ∨ y / D - x / D = 2 := by
  have h1 := Int.ediv_le_ediv hD (show x + 1 * D ≤ y by omega)
  have h2 := Int.ediv_le_ediv hD (show y ≤ x + 2 * D by omega)
  rw [Int.add_mul_ediv_right _ _ hD.ne'] at h1 h2
  omega

lemma short_shift_forward_colors (D x y : ℤ) (hD : 0 < D)
    (hlow : D ≤ y - x) (hupp : y - x < 2 * D) : forwardColorPair D x y := by
  have hg := short_shift_quotient_gap D x y hD hlow hupp
  unfold forwardColorPair intervalColor
  omega

lemma short_shift_not_reverse_colors (D x y : ℤ) (hD : 0 < D)
    (hlow : D ≤ y - x) (hupp : y - x < 2 * D) : ¬forwardColorPair D y x := by
  have hg := short_shift_quotient_gap D x y hD hlow hupp
  unfold forwardColorPair intervalColor
  omega

lemma intervalColor_bounds (D x : ℤ) : 0 ≤ intervalColor D x ∧ intervalColor D x < 5 := by
  exact ⟨Int.emod_nonneg _ (by norm_num), Int.emod_lt_of_pos _ (by norm_num)⟩

/-- Once a pair of endpoint colors has been fixed, the symmetric graph's
reverse edge cannot appear in its forward bilinear test. -/
lemma five_color_isolates_forward (D x y : ℤ) (hD : 0 < D)
    (hcolors : forwardColorPair D x y)
    (hedge : (D ≤ y - x ∧ y - x < 2 * D) ∨ (D ≤ x - y ∧ x - y < 2 * D)) :
    D ≤ y - x ∧ y - x < 2 * D := by
  rcases hedge with h | h
  · exact h
  · exact (short_shift_not_reverse_colors D y x hD h.1 h.2 hcolors).elim

end TwoPointCorrelations

end OAI
