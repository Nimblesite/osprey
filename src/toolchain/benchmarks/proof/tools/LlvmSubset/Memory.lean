/- [IR-LEAN-MEMORY] The memory model. Memory is a list of allocations that
   only grows: a `Val.ptr block off` names allocation `block` for ever, so a
   freed allocation stays recognisable and any later use of it is a fault.

   Every change to memory is one of three primitives (`alloc`, `access`,
   `kill`), and each one is built from a step the ownership ledger accepts.
   A `Tracked` state carries the proof, so "this run followed the ledger" is
   true by construction for everything the interpreter can do. -/
import OwnershipLedger
import LlvmSubset.Syntax
namespace LlvmSubset
open OwnershipLedger (Action owned dead step)

/-- What the runtime keeps behind an allocation it manages itself. -/
inductive Obj where
  /-- Plain words, read and written by the emitted code. -/
  | raw
  /-- A persistent list: its elements, whether they are managed pointers, and
      whether a derived version has taken the elements over. -/
  | list (elems : List Val) (managed : Bool) (handedOver : Bool)
  deriving DecidableEq, Repr

structure Blk where
  live : Bool
  heap : Bool
  size : Nat
  layout : Nat
  cells : List (Nat × Val)
  obj : Obj
  deriving DecidableEq, Repr

abbrev Mem := List Blk
abbrev Log := List (Nat × Action)

/-- The ledger's view: allocation `b`, while live, is owned by slot `b`. -/
def view (m : Mem) : OwnershipLedger.Heap := fun b =>
  match m[b]? with
  | some blk => if blk.live then owned b else dead
  | none => dead

/-- A ledger path that records the action taken at each step, newest first. -/
inductive Run : OwnershipLedger.Heap → Log → OwnershipLedger.Heap → Prop where
  | here (h) : Run h [] h
  | edge {start before log address action next} (preceding : Run start log before)
      (accepted : step (before address) action = some next) :
      Run start ((address, action) :: log)
        (fun atAddr => if atAddr = address then next else before atAddr)

theorem Run.path {start log finish} (run : Run start log finish) :
    OwnershipLedger.Path start finish := by
  induction run with
  | here => exact .here _
  | edge _ accepted ih => exact .edge ih accepted

theorem view_alloc (m : Mem) (blk : Blk) (alive : blk.live = true) :
    view (m ++ [blk]) =
      fun atAddr => if atAddr = m.length then owned m.length else view m atAddr := by
  funext a
  unfold view
  by_cases same : a = m.length
  · subst same; simp [alive]
  · by_cases inside : a < m.length
    · simp [same, List.getElem?_append_left inside]
    · have past : m.length ≤ a := Nat.le_of_not_lt inside
      have beyond : m.length < a := Nat.lt_of_le_of_ne past (Ne.symm same)
      have none₁ : (m ++ [blk])[a]? = none := by
        apply List.getElem?_eq_none
        simp only [List.length_append, List.length_singleton]
        exact Nat.succ_le_of_lt beyond
      have none₂ : m[a]? = none := List.getElem?_eq_none past
      simp [same, none₁, none₂]

theorem view_set (m : Mem) (b : Nat) (old next : Blk) (found : m[b]? = some old) :
    view (m.set b next) =
      fun atAddr => if atAddr = b then (if next.live then owned b else dead)
        else view m atAddr := by
  funext a
  unfold view
  have inside : b < m.length := by
    rcases Nat.lt_or_ge b m.length with h | h
    · exact h
    · rw [List.getElem?_eq_none h] at found; cases found
  by_cases same : a = b
  · subst same; simp [List.getElem?_set_self inside]
  · simp [same, List.getElem?_set_ne (Ne.symm same)]

/-- The interpreter's state: memory, and every ledger action taken on it. -/
structure St where
  mem : Mem
  log : Log

/-- A state reached only through steps the ownership ledger accepts. -/
def Good (s : St) : Prop := Run (fun _ => dead) s.log (view s.mem)

/-- A computation over the state that may fault. Memory changes only through
    the four primitives below, so what holds of them holds of every run. -/
def M (α : Type) := St → Except Fault (α × St)

instance : Monad M where
  pure a := fun s => .ok (a, s)
  bind m f := fun s =>
    match m s with
    | .ok (a, next) => f a next
    | .error e => .error e

def fault {α : Type} (e : Fault) : M α := fun _ => .error e

instance : MonadLift (Except Fault) M where
  monadLift
    | .ok a => pure a
    | .error e => fault e

def St.allocated (s : St) (heap : Bool) (size layout : Nat) (cells : List (Nat × Val)) (obj : Obj) : St :=
  ⟨s.mem ++ [⟨true, heap, size, layout, cells, obj⟩], (s.mem.length, .alloc s.mem.length) :: s.log⟩

/-- A fresh allocation: the ledger's `alloc` on a cell that never existed. -/
def allocBlk (heap : Bool) (size layout : Nat) (cells : List (Nat × Val)) (obj : Obj) : M Nat :=
  fun s => .ok (s.mem.length, s.allocated heap size layout cells obj)

/-- The live allocation `b`; `dead` is the fault using a freed one is. -/
def liveBlk (dead : Fault) (b : Nat) : M Blk := fun s =>
  match s.mem[b]? with
  | none => .error .badPointer
  | some blk => if blk.live then .ok (blk, s) else .error dead

/-- Read or write through a live allocation: the ledger's `read` by its owner. -/
def touch (b : Nat) (cells : List (Nat × Val)) (obj : Obj) : M Unit := fun s =>
  match s.mem[b]? with
  | none => .error .badPointer
  | some blk =>
    if blk.live then
      .ok ((), ⟨s.mem.set b { blk with cells := cells, obj := obj }, (b, .read b) :: s.log⟩)
    else .error .useAfterFree

/-- Free a live allocation: the ledger's `free` by its owner. -/
def killBlk (b : Nat) : M Unit := fun s =>
  match s.mem[b]? with
  | none => .error .badPointer
  | some blk =>
    if blk.live then .ok ((), ⟨s.mem.set b { blk with live := false }, (b, .free b) :: s.log⟩)
    else .error .doubleFree

/-- Whatever `m` answers, it answers from a good state with a good state. -/
def Preserves {α : Type} (m : M α) : Prop :=
  ∀ s a next, m s = .ok (a, next) → Good s → Good next

theorem Preserves.pure {α : Type} (a : α) : Preserves (Pure.pure a : M α) := by
  intro s b next h good
  cases h
  exact good

theorem Preserves.fault {α : Type} (e : Fault) : Preserves (fault e : M α) := by
  intro s b next h
  cases h

theorem Preserves.bind {α β : Type} {m : M α} {f : α → M β}
    (first : Preserves m) (rest : ∀ a, Preserves (f a)) : Preserves (m >>= f) := by
  intro s b next h good
  show Good next
  have h' : (match m s with
      | .ok (a, mid) => f a mid
      | .error e => .error e) = .ok (b, next) := h
  split at h'
  · rename_i a mid found
    exact rest a mid b next h' (first s a mid found good)
  · cases h'

theorem Preserves.lift {α : Type} (e : Except Fault α) : Preserves (liftM e : M α) := by
  cases e with
  | ok a => exact Preserves.pure a
  | error e => exact Preserves.fault e

theorem allocBlk_preserves (heap : Bool) (size layout : Nat) (cells : List (Nat × Val))
    (obj : Obj) : Preserves (allocBlk heap size layout cells obj) := by
  intro s a next h good
  cases h
  show Run _ _ (view (s.mem ++ [_]))
  rw [view_alloc s.mem _ rfl]
  refine .edge good ?_
  have fresh : view s.mem s.mem.length = dead := by simp [view]
  simp [fresh, step]

theorem liveBlk_preserves (dead : Fault) (b : Nat) : Preserves (liveBlk dead b) := by
  intro s a next h good
  unfold liveBlk at h
  split at h
  · cases h
  · split at h
    · cases h; exact good
    · cases h

theorem touch_preserves (b : Nat) (cells : List (Nat × Val)) (obj : Obj) :
    Preserves (touch b cells obj) := by
  intro s a next h good
  unfold touch at h
  split at h
  · cases h
  · rename_i blk found
    split at h
    · rename_i alive
      cases h
      show Run _ _ (view (s.mem.set b _))
      rw [view_set s.mem b blk _ found]
      have before : view s.mem b = owned b := by simp [view, found, alive]
      have accepted : step (view s.mem b) (.read b) = some (owned b) := by
        simp [before, step, owned]
      simpa [alive] using Run.edge good accepted
    · cases h

theorem killBlk_preserves (b : Nat) : Preserves (killBlk b) := by
  intro s a next h good
  unfold killBlk at h
  split at h
  · cases h
  · rename_i blk found
    split at h
    · rename_i alive
      cases h
      show Run _ _ (view (s.mem.set b _))
      rw [view_set s.mem b blk _ found]
      have before : view s.mem b = owned b := by simp [view, found, alive]
      have accepted : step (view s.mem b) (.free b) = some dead := by
        simp [before, step, owned, dead]
      simpa using Run.edge good accepted
    · cases h

/-- Memory before anything is allocated. -/
def St.empty : St := ⟨[], []⟩

theorem St.empty_good : Good St.empty := .here _

/-- A good state is a ledger path from the empty heap, so every allocation in
    it is in a valid ledger state: live with exactly one owner, or dead. -/
theorem Good.safe {s : St} (good : Good s) : OwnershipLedger.Safe (view s.mem) :=
  OwnershipLedger.from_empty good.path

end LlvmSubset
