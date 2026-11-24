/- [MEM-OWNERSHIP-STATIC-VERIFICATION-COMPILER] The compiled update consumes a Wide record.
   Four distinct managed leaves represent item, names, old label and new label. -/
import Emitted
namespace OwnershipRecord
open LlvmSubset

def seed (n : Int) : St := Id.run do
  let leaf (s : St) := s.allocated true 8 0 [] .raw
  let t := leaf (leaf (leaf (leaf St.empty)))
  return t.allocated true 40 3585 [(0, .int 0), (8, .ptr 0 0), (16, .ptr 1 0),
    (24, .ptr 2 0), (32, .int n)] .raw

theorem seed_good (n : Int) : Good (seed n) := by
  have push (s : St) (h : Good s) (size layout : Nat) (cells : List (Nat × Val)) :
      Good (s.allocated true size layout cells .raw) :=
    allocBlk_preserves true size layout cells .raw s s.mem.length _ rfl h
  exact push _ (push _ (push _ (push _ (push _ St.empty_good _ _ _) _ _ _) _ _ _) _ _ _) _ _ _

def observe (n : Int) : Option (List Bool × List (Option Val) × List Bool) :=
  match runFn Emitted.module 12 Emitted.«relabel(ptr,ptr)» [.ptr 4 0, .ptr 3 0] (seed n) with
  | .ok (result@(.ptr b 0), t) => do
    let record ← t.mem[b]?
    let (_, freed) ← (freeVal 12 result t).toOption
    return ((t.mem.filter (·.heap)).map (·.live),
      [0, 8, 16, 24, 32].map (fun offset => record.cells.lookup offset),
      (freed.mem.filter (·.heap)).map (·.live))
  | _ => none

set_option maxRecDepth 32000 in
set_option maxHeartbeats 4000000 in
theorem compiledRelabel (n : Int) : observe n =
    some ([true, true, false, true, false, true],
      [some (.int 0), some (.ptr 0 0), some (.ptr 1 0), some (.ptr 3 0), some (.int n)],
      [false, false, false, false, false, false]) := by
  kernel_rfl

/-- Conditional ledger soundness, independently replayed in the Osprey kernel. -/
theorem compiledRelabelLedger (n : Int) (r : St) (v : Val)
    (done : runFn Emitted.module 12 Emitted.«relabel(ptr,ptr)» [.ptr 4 0, .ptr 3 0] (seed n) = .ok (v, r)) :
    Good r ∧ OwnershipLedger.Safe (view r.mem) :=
  run_follows_ledger Emitted.module 12 Emitted.«relabel(ptr,ptr)» _ (seed n) r v (seed_good n) done
/-- The specialized generic identity returns its input allocation alive, with
    every caller-supplied cell unchanged and no additional heap blocks. -/
def identityResult (cells : List (Nat × Val)) : Except Fault (Val × List (Bool × List (Nat × Val))) := do
  let t := St.empty.allocated true 8 0 cells .raw
  let (result, after) ← runFn Emitted.module 4 Emitted.«keep(ptr)» [.ptr 0 0] t
  return (result, (after.mem.filter (·.heap)).map fun b => (b.live, b.cells))

theorem compiledIdentity (cells : List (Nat × Val)) : identityResult cells =
    .ok (.ptr 0 0, [(true, cells)]) := by kernel_rfl


/-- The record owns a name and a nested box. The caller may place arbitrary
    bytes, represented as cells, in the returned name allocation. -/
def fieldResult (cells : List (Nat × Val)) (health : Int) : Except Fault (Val × List Bool × Option (List (Nat × Val))) := Id.run do
  let t := St.empty.allocated true 8 0 cells .raw
  let t := t.allocated true 8 0 [] .raw
  let t := t.allocated true 16 513 [(8, .ptr 1 0)] .raw
  let t := t.allocated true 32 1537 [(8, .ptr 0 0), (16, .ptr 2 0), (24, .int health)] .raw
  return (runFn Emitted.module 8 Emitted.«foeName(ptr)» [.ptr 3 0] t).map fun (v, (r : St)) =>
    (v, (r.mem.filter (·.heap)).map (·.live), (r.mem[0]?).map (·.cells))

theorem compiledFieldTake (cells : List (Nat × Val)) (health : Int) :
    fieldResult cells health = .ok (.ptr 0 0, [true, false, false, false], some cells) := by
  kernel_rfl

end OwnershipRecord
