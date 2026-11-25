/- [IR-LEAN-EMITTED] Theorems about the functions the compiler emitted for
   single_owner.test.osp. `Emitted` is generated on every run by
   scripts/proof/ir-lean.sh from the compiler's own IR, so each statement is
   about the code that would run, for every input at once. -/
import LlvmSubset
import Emitted
namespace EmittedOwnership
open LlvmSubset

/-- What a caller can see when a call returns: the object the result points
    at, every heap object still alive, and how many were freed. Allocation
    numbers are left out, so the statements survive a change in how many
    stack slots the compiler reserves. -/
structure Seen where
  result : Option Obj
  live : List Obj
  freed : Nat
  deriving DecidableEq, Repr

def objectAt (mem : Mem) : Val → Option Obj
  | .ptr b 0 => (mem[b]?).map (·.obj)
  | _ => none

def seen (run : Except Fault (Val × St)) : Except Fault Seen :=
  run.map fun (v, s) =>
    let heap := s.mem.filter (fun (b : Blk) => b.heap)
    ⟨objectAt s.mem v, (heap.filter (·.live)).map (·.obj), (heap.filter (!·.live)).length⟩

/-- A state holding the heap objects `objs`, each owned by the caller, at
    allocations 0, 1, ... in order. -/
def holding : List (Nat × Obj) → St → St
  | [], s => s
  | (size, obj) :: rest, s =>
    holding rest (s.allocated true size 0 [] obj)

/-- Such a state is one the ledger accepts, so `run_follows_ledger` applies
    to every run the theorems below start from it. -/
theorem holding_good (objs : List (Nat × Obj)) : ∀ s, Good s → Good (holding objs s) := by
  induction objs with
  | nil => intro s good; exact good
  | cons head rest ih =>
    intro s good
    exact ih _ (allocBlk_preserves true head.1 0 [] head.2 s s.mem.length _ rfl good)

/-- Memory holding one list the caller owns, at allocation 0. -/
def oneList (xs : List Val) (managed : Bool) : St :=
  holding [(0, .list xs managed false)] St.empty

/-- `fn grow(xs, x) = listAppend(xs, x)` over scalars: for every list and
    every element, the emitted code returns the longer list, frees the old
    one exactly once, and leaves nothing else alive. -/
theorem growScalar (xs : List Val) (x : Int) :
    seen (runFn Emitted.module 4 Emitted.«grow(ptr,i64)» [.ptr 0 0, .int x] (oneList xs false))
      = .ok ⟨some (.list (xs ++ [.int x]) false false),
             [.list (xs ++ [.int x]) false false], 1⟩ := by kernel_rfl

/-- The same function over managed elements: the element moves into the new
    list and is not freed with the old one. -/
theorem growManaged (xs : List Val) :
    seen (runFn Emitted.module 4 Emitted.«grow(ptr,ptr)» [.ptr 0 0, .ptr 1 0]
        (holding [(0, .list xs true false), (8, .raw)] St.empty))
      = .ok ⟨some (.list (xs ++ [.ptr 1 0]) true false),
             [.raw, .list (xs ++ [.ptr 1 0]) true false], 1⟩ := by kernel_rfl

/-- `growWhen(xs, x, enabled)`: on the growing path the old list is freed
    once and the new one returned; on the other path the list comes back
    untouched and nothing is freed. -/
theorem growWhen (xs : List Val) (x : Int) (enabled : Bool) :
    seen (runFn Emitted.module 16 Emitted.«growWhen(ptr,i64,i1)»
        [.ptr 0 0, .int x, .int (if enabled then 1 else 0)] (oneList xs false))
      = .ok (if enabled then
          ⟨some (.list (xs ++ [.int x]) false false), [.list (xs ++ [.int x]) false false], 1⟩
        else ⟨some (.list xs false false), [.list xs false false], 0⟩) := by
  cases enabled <;> kernel_rfl

end EmittedOwnership
