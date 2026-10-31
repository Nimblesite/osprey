-- The M0 identity theorem of plan 0041, as Lean states it. `lean --run` of this
-- file elaborates and kernel-checks the theorem; the proof benchmark times that
-- against Osprey's kernel checking the same term [PROOF-SPEED].
theorem id_imp : ∀ (p : Prop), p → p := fun _ h => h

def main : IO Unit := IO.println "checked: id_imp"
