prelude
import Init.Data.Nat.Basic

/-!
Compatible ports of three exact `Init.Data.Nat.Lemmas` statements proved from
`Init.Data.Nat.Basic` alone. The `prelude` header stops Lean auto-importing the
full `Init` prelude, so `Init.Data.Nat.Lemmas` (and its `omega` bodies) is
genuinely absent. Statements, binder order and hypotheses are unchanged;
only the proof bodies differ.
-/

namespace Nat

theorem add_eq_left {a b : Nat} : a + b = a ↔ b = 0 :=
  Iff.intro
    (fun h => Nat.add_left_cancel (h.trans (Nat.add_zero a).symm))
    (fun h => by rw [h, Nat.add_zero])

theorem add_eq_right {a b : Nat} : a + b = b ↔ a = 0 := by
  rw [Nat.add_comm a b]
  exact add_eq_left

theorem sub_sub_sub_cancel_right {a b c : Nat} (h : c ≤ b) :
    a - c - (b - c) = a - b := by
  induction c generalizing a b with
  | zero => rfl
  | succ c ih =>
    cases a with
    | zero => rw [Nat.zero_sub, Nat.zero_sub, Nat.zero_sub]
    | succ a =>
      cases b with
      | zero => exact absurd h (Nat.not_succ_le_zero c)
      | succ b =>
        rw [Nat.succ_sub_succ, Nat.succ_sub_succ, Nat.succ_sub_succ]
        exact ih (Nat.le_of_succ_le_succ h)

end Nat
