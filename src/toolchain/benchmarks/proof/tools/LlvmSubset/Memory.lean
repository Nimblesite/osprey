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
    · have past : m.length < a := by omega
      have none₁ : (m ++ [blk])[a]? = none := by
        apply List.getElem?_eq_none; simp; omega
      have none₂ : m[a]? = none := List.getElem?_eq_none (by omega)
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

/-- A state reached from `start` only through ledger-accepted steps. -/
structure Tracked (start : OwnershipLedger.Heap) where
  mem : Mem
  log : Log
  run : Run start log (view mem)

namespace Tracked
variable {start : OwnershipLedger.Heap}

/-- A fresh allocation: the ledger's `alloc` on a cell that never existed. -/
def alloc (t : Tracked start) (heap : Bool) (size layout : Nat)
    (cells : List (Nat × Val)) (obj : Obj) : Nat × Tracked start :=
  let blk : Blk := ⟨true, heap, size, layout, cells, obj⟩
  (t.mem.length, ⟨t.mem ++ [blk], (t.mem.length, .alloc t.mem.length) :: t.log, by
    rw [view_alloc t.mem blk rfl]
    refine .edge t.run ?_
    have fresh : view t.mem t.mem.length = dead := by
      simp [view]
    simp [fresh, step]⟩)

/-- The live allocation `b`, or the fault its use would be. -/
def liveOf (m : Mem) (b : Nat) (entry : Option Blk) (found : m[b]? = entry) :
    Except Fault {blk : Blk // m[b]? = some blk ∧ blk.live = true} :=
  match entry, found with
  | none, _ => .error .badPointer
  | some blk, found =>
    if alive : blk.live = true then .ok ⟨blk, found, alive⟩ else .error .useAfterFree

/-- The live allocation `b`, or the fault its use would be. -/
def live (t : Tracked start) (b : Nat) :
    Except Fault {blk : Blk // t.mem[b]? = some blk ∧ blk.live = true} :=
  liveOf t.mem b t.mem[b]? rfl

/-- Read or write through a live allocation: the ledger's `read` by its owner. -/
def access (t : Tracked start) (b : Nat) (blk : Blk)
    (found : t.mem[b]? = some blk ∧ blk.live = true)
    (cells : List (Nat × Val)) (obj : Obj) : Tracked start :=
  ⟨t.mem.set b { blk with cells := cells, obj := obj }, (b, .read b) :: t.log, by
    rw [view_set t.mem b blk _ found.1]
    have before : view t.mem b = owned b := by simp [view, found.1, found.2]
    have accepted : step (view t.mem b) (.read b) = some (owned b) := by
      simp [before, step, owned]
    simpa [found.2] using Run.edge t.run accepted⟩

/-- Free a live allocation: the ledger's `free` by its owner. -/
def kill (t : Tracked start) (b : Nat) (blk : Blk)
    (found : t.mem[b]? = some blk ∧ blk.live = true) : Tracked start :=
  ⟨t.mem.set b { blk with live := false }, (b, .free b) :: t.log, by
    rw [view_set t.mem b blk _ found.1]
    have before : view t.mem b = owned b := by simp [view, found.1, found.2]
    have accepted : step (view t.mem b) (.free b) = some dead := by
      simp [before, step, owned, dead]
    simpa using Run.edge t.run accepted⟩

end Tracked

/-- The general theorem's ledger half: whatever a tracked state has done, it
    is a path the ownership ledger accepts, so every allocation it reached is
    in a valid ledger state: live with exactly one owner, or dead. -/
theorem Tracked.safe (t : Tracked (fun _ => dead)) : OwnershipLedger.Safe (view t.mem) :=
  OwnershipLedger.from_empty t.run.path

end LlvmSubset
