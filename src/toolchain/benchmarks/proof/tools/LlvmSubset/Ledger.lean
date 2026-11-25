/- [IR-LEAN-GENERAL] Every run follows the ownership ledger.

   The interpreter changes memory only through `allocBlk`, `liveBlk`, `touch`
   and `killBlk`, and each of those is a step the ledger accepts
   (LlvmSubset/Memory.lean). Everything else is those four composed, so the
   property passes up through every definition to `runFn`. -/
import LlvmSubset.Exec
namespace LlvmSubset

-- From here on `Preserves` is used only through its lemmas.
attribute [local irreducible] Preserves

/-- Close a `Preserves` goal for a definition built from the primitives, the
    monad operations and anything already proved and in scope. -/
macro "preserve" : tactic => `(tactic| repeat (first
  | split
  | with_reducible exact Preserves.pure _
  | with_reducible exact Preserves.fault _
  | with_reducible exact Preserves.lift _
  | with_reducible exact allocBlk_preserves _ _ _ _ _
  | with_reducible exact liveBlk_preserves _ _
  | with_reducible exact touch_preserves _ _ _
  | with_reducible exact killBlk_preserves _
  | with_reducible apply_assumption
  | with_reducible apply Preserves.settled
  | with_reducible apply Preserves.bind
  | intro _))

theorem loadAt_preserves (ty : Ty) (v : Val) : Preserves (loadAt ty v) := by
  cases v <;> (unfold loadAt; preserve)

theorem storeAt_preserves (ty : Ty) (v p : Val) : Preserves (storeAt ty v p) := by
  cases p <;> (unfold storeAt; preserve)

theorem freeEach_preserves (free : Val → M Unit) (each : ∀ v, Preserves (free v))
    (vs : List Val) : Preserves (freeEach free vs) := by
  induction vs with
  | nil => unfold freeEach; preserve
  | cons v rest ih => unfold freeEach; preserve

theorem freeVal_preserves (fuel : Nat) : ∀ v, Preserves (freeVal fuel v) := by
  induction fuel with
  | zero => intro v; unfold freeVal; preserve
  | succ n ih =>
    intro v
    have rest := freeEach_preserves (freeVal n) ih
    cases v with
    | int _ => unfold freeVal; exact Preserves.pure _
    | ptr b off =>
      unfold freeVal
      split
      · exact Preserves.fault _
      · apply Preserves.bind (liveBlk_preserves _ _)
        intro blk
        split
        · apply Preserves.bind (Preserves.lift _)
          intro owned
          apply Preserves.bind (killBlk_preserves _)
          intro _
          exact rest owned
        · exact Preserves.fault _

theorem allocated_preserves (size layout : Int) (cells : List (Nat × Val)) :
    Preserves (allocated size layout cells) := by
  unfold allocated
  split
  · exact Preserves.fault _
  · apply Preserves.bind (allocBlk_preserves _ _ _ _ _)
    intro b
    exact Preserves.pure _

theorem appendTo_preserves (source : Nat) (v : Val) (managed : Bool) :
    Preserves (appendTo source v managed) := by
  unfold appendTo; preserve

theorem lengthOf_preserves (source : Nat) : Preserves (lengthOf source) := by
  unfold lengthOf; preserve

theorem callExt_preserves (fuel : Nat) (name : Ext) (args : List Val) :
    Preserves (callExt fuel name args) := by
  have free := freeVal_preserves fuel
  have made := allocated_preserves
  have appended := appendTo_preserves
  have counted := lengthOf_preserves
  unfold callExt; preserve

<<<<<<< HEAD
=======
theorem rmwAt_preserves (op : RmwOp) (ty : Ty) (p operand : Val) :
    Preserves (rmwAt op ty p operand) := by
  have loaded := loadAt_preserves
  have stored := storeAt_preserves
  unfold rmwAt; preserve

theorem callAt_preserves (callFn : CallFn) (called : ∀ index args, Preserves (callFn index args))
    (values : List Val) (target : Val) : Preserves (callAt callFn values target) := by
  cases target <;> (unfold callAt; preserve)

>>>>>>> origin/fixes
theorem execInstr_preserves (callFn : CallFn) (called : ∀ index args, Preserves (callFn index args))
    (fuel : Nat) (env : Env) (instr : Instr) : Preserves (execInstr callFn fuel env instr) := by
  have loaded := loadAt_preserves
  have stored := storeAt_preserves
  have external := callExt_preserves
<<<<<<< HEAD
=======
  have atomic := rmwAt_preserves
  have indirect := callAt_preserves callFn called
>>>>>>> origin/fixes
  unfold execInstr; preserve

theorem execBody_preserves (callFn : CallFn) (called : ∀ index args, Preserves (callFn index args))
    (fuel : Nat) (body : List Instr) : ∀ env, Preserves (execBody callFn fuel body env) := by
  have one := execInstr_preserves callFn called fuel
  induction body with
  | nil => intro env; unfold execBody; preserve
  | cons instr rest ih => intro env; unfold execBody; rw [forceCells_eq]; preserve

theorem runBlocks_preserves (callFn : CallFn) (called : ∀ index args, Preserves (callFn index args))
    (fuel : Nat) (f : Func) (steps : Nat) :
    ∀ block env, Preserves (runBlocks callFn fuel f steps block env) := by
  have body := execBody_preserves callFn called fuel
  induction steps with
  | zero => intro block env; unfold runBlocks; preserve
  | succ n ih => intro block env; unfold runBlocks; preserve

theorem runFn_preserves (m : Module) (fuel : Nat) :
    ∀ index args, Preserves (runFn m fuel index args) := by
  induction fuel with
  | zero => intro index args; unfold runFn; preserve
  | succ n ih =>
    intro index args
    have blocks := runBlocks_preserves (runFn m n) ih n
    unfold runFn; preserve

/-- The general theorem. For every module the translator can write, every
    function in it, every argument list and every amount of fuel: a run that
    starts from a state the ledger accepts and answers at all took only steps
    the ledger accepts. So every allocation it reached is in a valid ledger
    state, live with exactly one owner or dead; nothing was freed twice and
    nothing was read or written after its free. No hypothesis names a
    particular function. -/
theorem run_follows_ledger (m : Module) (fuel index : Nat) (args : List Val) (s next : St)
    (v : Val) (good : Good s) (ran : runFn m fuel index args s = .ok (v, next)) :
    Good next ∧ OwnershipLedger.Safe (view next.mem) :=
  have keeps : ∀ s a next, runFn m fuel index args s = .ok (a, next) → Good s → Good next := by
    have held := runFn_preserves m fuel index args
    unfold Preserves at held
    exact held
  have reached := keeps s v next ran good
  ⟨reached, reached.safe⟩

end LlvmSubset
