/- [IR-LEAN-KERNEL-RFL] `kernel_rfl` closes `a = b` with `Eq.refl a` and leaves
   the check that `a` and `b` compute to the same thing to Lean's kernel.
   The elaborator's own check re-evaluates shared work and does not finish on
   a run of an emitted function; the kernel is the checker that is trusted
   either way, so nothing is skipped. -/
import Lean
open Lean Elab Tactic Meta

elab "kernel_rfl" : tactic => do
  let goal ← getMainGoal
  let type ← instantiateMVars (← goal.getType)
  let some (α, lhs, _) := type.eq? | throwError "kernel_rfl: the goal is not an equality"
  let level ← getLevel α
  goal.assign (mkApp2 (mkConst ``Eq.refl [level]) α lhs)
