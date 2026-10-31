import Mathlib.Data.Nat.Log

namespace OAI

/-! # Constant-state power-of-two initialization

The same executed doubling cursor supplies both prime candidate moduli
and trial-division limits.
-/

namespace LowSpaceSubsetSum.WordSeedPowerExecution

def loop : ℕ → ℕ → ℕ → ℕ × ℕ
  | 0, value, work => (value,work+1)
  | fuel+1, value, work => loop fuel (2*value) (work+3)

theorem loop_value (fuel value work : ℕ) :
    loop fuel value work = (value*2^fuel,work+3*fuel+1) := by
  induction fuel generalizing value work with
  | zero => simp only [loop,Nat.pow_zero,Nat.mul_one,Nat.mul_zero,Nat.add_zero]
  | succ fuel ih =>
    change loop fuel (2*value) (work+3) = _
    rw [ih,Nat.pow_succ]
    apply Prod.ext
    · simp only [Nat.mul_assoc,Nat.mul_comm]
    · omega

def run (width : ℕ) := loop width 1 0

theorem run_value (width : ℕ) : (run width).1 = 2^width := by
  simp only [run,loop_value,Nat.one_mul]

theorem run_work (width : ℕ) : (run width).2 = 3*width+1 := by
  simp only [run,loop_value,Nat.zero_add]

end LowSpaceSubsetSum.WordSeedPowerExecution

end OAI
