/- [IR-LEAN-FORCE] Bring the interpreter's state to normal form between steps.

   A proof kernel evaluates by substitution. Where a value is used twice, a
   kernel that does not share work evaluates it twice, and a state that is
   still "the previous state, changed" then costs twice as much at every step.
   Each function here rebuilds a value from its constructors, so what the next
   step receives is data, not a pending computation. Every one is the
   identity (`force*_eq`), so the meaning of a run is unchanged; only the
   kernel's work is. List payloads and integers are left alone: in a theorem
   they are variables, and a variable has no constructor to rebuild from. -/
import LlvmSubset.Memory
namespace LlvmSubset
open OwnershipLedger (Action)
variable {β : Sort u}

def forceNat (n : Nat) (k : Nat → β) : β :=
  match n with
  | 0 => k 0
  | m + 1 => k (m + 1)

def forceBool (b : Bool) (k : Bool → β) : β :=
  match b with
  | true => k true
  | false => k false

def forceVal (v : Val) (k : Val → β) : β :=
  match v with
  | .int x => k (.int x)
  | .ptr b off => forceNat b fun b => forceNat off fun off => k (.ptr b off)

def forceCells : List (Nat × Val) → (List (Nat × Val) → β) → β
  | [], k => k []
  | (key, v) :: rest, k =>
    forceNat key fun key => forceVal v fun v => forceCells rest fun rest => k ((key, v) :: rest)

def forceObj (obj : Obj) (k : Obj → β) : β :=
  match obj with
  | .raw => k .raw
  | .list elems managed handedOver =>
    forceBool managed fun managed => forceBool handedOver fun handedOver =>
      k (.list elems managed handedOver)

def forceBlk (blk : Blk) (k : Blk → β) : β :=
  match blk with
  | ⟨live, heap, size, layout, cells, obj⟩ =>
    forceBool live fun live => forceBool heap fun heap => forceNat size fun size =>
      forceNat layout fun layout => forceObj obj fun obj =>
        let finish := fun payload => k ⟨live, heap, size, layout, payload, obj⟩
        if heap then finish cells else forceCells cells finish

def forceMem : Mem → (Mem → β) → β
  | [], k => k []
  | blk :: rest, k => forceBlk blk fun blk => forceMem rest fun rest => k (blk :: rest)

def forceAction (action : Action) (k : Action → β) : β :=
  match action with
  | .alloc owner => forceNat owner fun owner => k (.alloc owner)
  | .borrow borrower lender =>
    forceNat borrower fun borrower => forceNat lender fun lender => k (.borrow borrower lender)
  | .read slot => forceNat slot fun slot => k (.read slot)
  | .endBorrow borrower => forceNat borrower fun borrower => k (.endBorrow borrower)
  | .move source destination =>
    forceNat source fun source => forceNat destination fun destination =>
      k (.move source destination)
  | .free owner => forceNat owner fun owner => k (.free owner)

def forceLog : Log → (Log → β) → β
  | [], k => k []
  | (address, action) :: rest, k =>
    forceNat address fun address => forceAction action fun action =>
      forceLog rest fun rest => k ((address, action) :: rest)

def forceSt (s : St) (k : St → β) : β :=
  match s with
  | ⟨mem, log⟩ => forceMem mem fun mem => forceLog log fun log => k ⟨mem, log⟩

theorem forceNat_eq (n : Nat) (k : Nat → β) : forceNat n k = k n := by
  cases n <;> rfl

theorem forceBool_eq (b : Bool) (k : Bool → β) : forceBool b k = k b := by
  cases b <;> rfl

theorem forceVal_eq (v : Val) (k : Val → β) : forceVal v k = k v := by
  cases v <;> simp [forceVal, forceNat_eq]

theorem forceCells_eq (cells : List (Nat × Val)) :
    ∀ k : List (Nat × Val) → β, forceCells cells k = k cells := by
  induction cells with
  | nil => intro k; rfl
  | cons head rest ih => intro k; cases head; simp [forceCells, forceNat_eq, forceVal_eq, ih]

theorem forceObj_eq (obj : Obj) (k : Obj → β) : forceObj obj k = k obj := by
  cases obj <;> simp [forceObj, forceBool_eq]

theorem forceBlk_eq (blk : Blk) (k : Blk → β) : forceBlk blk k = k blk := by
  obtain ⟨live, heap, size, layout, cells, obj⟩ := blk
  cases heap <;> simp [forceBlk, forceBool_eq, forceNat_eq, forceCells_eq, forceObj_eq]

theorem forceMem_eq (mem : Mem) : ∀ k : Mem → β, forceMem mem k = k mem := by
  induction mem with
  | nil => intro k; rfl
  | cons head rest ih => intro k; simp [forceMem, forceBlk_eq, ih]

theorem forceAction_eq (action : Action) (k : Action → β) : forceAction action k = k action := by
  cases action <;> simp [forceAction, forceNat_eq]

theorem forceLog_eq (log : Log) : ∀ k : Log → β, forceLog log k = k log := by
  induction log with
  | nil => intro k; rfl
  | cons head rest ih => intro k; cases head; simp [forceLog, forceNat_eq, forceAction_eq, ih]

theorem forceSt_eq (s : St) (k : St → β) : forceSt s k = k s := by
  cases s
  simp [forceSt, forceMem_eq, forceLog_eq]

/-- `m`, with the state it leaves brought to normal form. -/
def settled {α : Type} (m : M α) : M α := fun s =>
  match m s with
  | .ok (a, next) => forceSt next fun next => .ok (a, next)
  | .error e => .error e

theorem settled_eq {α : Type} (m : M α) : settled m = m := by
  funext s
  unfold settled
  cases m s with
  | error e => rfl
  | ok result => cases result; simp [forceSt_eq]

theorem Preserves.settled {α : Type} {m : M α} (kept : Preserves m) :
    Preserves (settled m) := by
  rw [settled_eq]
  exact kept

end LlvmSubset
