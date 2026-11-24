/- [MEM-OWNERSHIP-STATIC-VERIFICATION-COMPILER] The compiled update consumes a Wide record.
   Four distinct managed leaves represent item, names, old label and new label. -/
import Emitted
namespace OwnershipRecord
open LlvmSubset

def seed (n : Int) : Tracked (fun _ => OwnershipLedger.dead) := Id.run do
  let leaf (t : Tracked (fun _ => OwnershipLedger.dead)) := (t.alloc true 8 0 [] .raw).2
  let t := leaf (leaf (leaf (leaf Tracked.empty)))
  return (t.alloc true 40 3585 [(0, .int 0), (8, .ptr 0 0), (16, .ptr 1 0),
    (24, .ptr 2 0), (32, .int n)] .raw).2

def observe (n : Int) : Option (List Bool × List (Option Val) × List Bool) :=
  match runFn Emitted.module 12 Emitted.«relabel(ptr,ptr)» [.ptr 4 0, .ptr 3 0] (seed n) with
  | .ok (result@(.ptr b 0), t) => do
    let record ← t.mem[b]?
    let freed ← (freeVal 12 t result).toOption
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
theorem compiledRelabelLedger (n : Int) (r : Tracked (fun _ => OwnershipLedger.dead)) (v : Val)
    (done : runFn Emitted.module 12 Emitted.«relabel(ptr,ptr)» [.ptr 4 0, .ptr 3 0] (seed n) = .ok (v, r)) :
    Run (fun _ => OwnershipLedger.dead) r.log (view r.mem) ∧ OwnershipLedger.Safe (view r.mem) :=
  run_follows_ledger Emitted.module 12 Emitted.«relabel(ptr,ptr)» _ (seed n) r v done
end OwnershipRecord
