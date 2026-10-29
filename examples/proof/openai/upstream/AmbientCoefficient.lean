import OAI.Algebra.Determinantal.BilinearCounts

namespace OAI

open MvPolynomial
open PermanentBorder.KernelBilinear
namespace PermanentBorder.AmbientPolar

/-- H,U,V are respectively none, some false, some true. -/
def groupIndex : Option Bool → Fin 3
  | none => 0
  | some false => 1
  | some true => 2

lemma groupIndex_injective : Function.Injective groupIndex := by
  decide

lemma dimensions_explicit (K n : ℕ) :
    dimensions (Fin K) n =
      Finsupp.single none (K+1) + Finsupp.single (some false) n +
        Finsupp.single (some true) n := by
  classical
  ext j
  cases j with
  | none => simp
  | some b => cases b <;> simp

/-- The three-variable coefficient equals the mixed coefficient
for matrices of size `n + 1`. -/
lemma literal_coefficient_eq (K n : ℕ) :
    ((X (0 : Fin 3) + X 1)^(n+1) * (X 0 + X 2)^n *
      (X 1 + X 2)^K : MvPolynomial (Fin 3) ℕ).coeff
      (Finsupp.single 0 (K+1) + Finsupp.single 1 n + Finsupp.single 2 n) =
    (Multigraded.mixedPolynomial
      (firstGroup (κ := Fin K) (n := n)) secondGroup).coeff
      (dimensions (Fin K) n) := by
  classical
  have h := MvPolynomial.coeff_rename_mapDomain groupIndex groupIndex_injective
    (Multigraded.mixedPolynomial (firstGroup (κ := Fin K) (n := n)) secondGroup)
    (dimensions (Fin K) n)
  have hdegree : (dimensions (Fin K) n).mapDomain groupIndex =
      Finsupp.single 0 (K+1) + Finsupp.single 1 n + Finsupp.single 2 n := by
    rw [dimensions_explicit]
    simp only [Finsupp.mapDomain_add, Finsupp.mapDomain_single, groupIndex]
  rw [hdegree] at h
  simpa [mixedPolynomial_eq, groupIndex] using h

/-- A bound by the mixed coefficient is also a bound by the three-variable coefficient. -/
lemma count_le_literal_coefficient (K n D : ℕ)
    (h : D ≤ (Multigraded.mixedPolynomial
      (firstGroup (κ := Fin K) (n := n)) secondGroup).coeff
      (dimensions (Fin K) n)) :
    D ≤ ((X (0 : Fin 3) + X 1)^(n+1) * (X 0 + X 2)^n *
      (X 1 + X 2)^K : MvPolynomial (Fin 3) ℕ).coeff
      (Finsupp.single 0 (K+1) + Finsupp.single 1 n + Finsupp.single 2 n) := by
  rw [literal_coefficient_eq]
  exact h

end PermanentBorder.AmbientPolar

end OAI
