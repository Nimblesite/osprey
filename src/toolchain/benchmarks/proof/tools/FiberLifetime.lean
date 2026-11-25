/- [CONCURRENCY-FIBER-LIFETIME] The holder count that decides when a fiber is
   freed (src/toolchain/compiler/runtime/fiber_lifetime.h).

   A fiber is a future (Halstead, Multilisp, TOPLAS 1985) whose storage is
   reclaimed once nothing can reach it (Baker and Hewitt, The Incremental
   Garbage Collection of Processes, MIT AI Memo 454, 1977). The runtime keeps
   that reachability as a count of holders: the program's handle, the runner
   until the body has finished, and each await or probe while it reads. A
   fiber is `live n` while `n + 1` holders remain, and `freed` once the last
   one has let go.

   `fiber_hold` is `acquire`, `fiber_release` is `release`, and a spawn starts
   at `spawned`: the handle and the runner. Taking or ending a hold on a freed
   fiber is the use after free the runtime must never make, so `step` refuses
   it with `none`.

   The theorems: every trace in which only holders act runs without a use
   after free and ends with exactly as many holders as it says (`run_valid`);
   the fiber is freed exactly when the last holder lets go (`freed_iff`); and a
   spawned fiber whose handle and runner both let go, with every await
   balanced, is freed (`spawn_then_both_release`), the leak #359 was. -/
namespace FiberLifetime

inductive Fiber where
  | live (others : Nat)
  | freed

inductive Event where
  | acquire
  | release

/-- A spawned fiber has two holders: its handle and its runner. -/
def spawned : Fiber := .live 1

/-- One hold taken or ended; `none` is a use after free. -/
def step : Fiber → Event → Option Fiber
  | .live n, .acquire => some (.live (n + 1))
  | .live 0, .release => some .freed
  | .live (n + 1), .release => some (.live n)
  | .freed, _ => none

def run : Fiber → List Event → Option Fiber
  | fiber, [] => some fiber
  | fiber, event :: rest =>
    match step fiber event with
    | some next => run next rest
    | none => none

/-- The fiber that `holders` holders leave. -/
def ofHolders : Nat → Fiber
  | 0 => .freed
  | n + 1 => .live n

/-- `Valid k events j`: starting with `k` holders, only a holder acquires (an
    awaiter is reached through a handle it holds) and only a holder releases,
    leaving `j`. -/
inductive Valid : Nat → List Event → Nat → Prop
  | done (k : Nat) : Valid k [] k
  | acquire {k j : Nat} {rest : List Event} :
      Valid (k + 2) rest j → Valid (k + 1) (.acquire :: rest) j
  | release {k j : Nat} {rest : List Event} :
      Valid k rest j → Valid (k + 1) (.release :: rest) j

theorem acquire_one (k : Nat) :
    step (ofHolders (k + 1)) .acquire = some (ofHolders (k + 2)) :=
  match k with
  | 0 => rfl
  | _ + 1 => rfl

theorem release_one (k : Nat) : step (ofHolders (k + 1)) .release = some (ofHolders k) :=
  match k with
  | 0 => rfl
  | _ + 1 => rfl

/-- A step that succeeds continues the run from where it left the fiber. -/
theorem run_step {fiber next : Fiber} {event : Event} {rest : List Event}
    (stepped : step fiber event = some next) : run fiber (event :: rest) = run next rest := by
  show (match step fiber event with
    | some after => run after rest
    | none => none) = run next rest
  rw [stepped]

/-- Every valid trace runs without a use after free, leaving its holders. -/
theorem run_valid {k j : Nat} {events : List Event} (valid : Valid k events j) :
    run (ofHolders k) events = some (ofHolders j) := by
  induction valid with
  | done k => rfl
  | @acquire k _ _ _ ih => exact (run_step (acquire_one k)).trans ih
  | @release k _ _ _ ih => exact (run_step (release_one k)).trans ih

/-- The fiber is freed exactly when no holder is left. -/
theorem freed_iff (j : Nat) : ofHolders j = .freed ↔ j = 0 :=
  match j with
  | 0 => ⟨fun _ => rfl, fun _ => rfl⟩
  | _ + 1 => ⟨fun h => Fiber.noConfusion h, fun h => Nat.noConfusion h⟩

/-- Nothing at all may be done to a freed fiber. -/
theorem freed_is_final (event : Event) : step .freed event = none := rfl

/-- Any number of balanced awaits, each holding the fiber while it reads. -/
def awaits : Nat → List Event
  | 0 => []
  | n + 1 => .acquire :: .release :: awaits n

theorem awaits_valid (k : Nat) (n : Nat) (rest : List Event) (j : Nat)
    (valid : Valid (k + 1) rest j) : Valid (k + 1) (awaits n ++ rest) j :=
  match n with
  | 0 => valid
  | n + 1 => .acquire (.release (awaits_valid k n rest j valid))

/-- #359: a spawned fiber, awaited any number of times, is freed once its
    runner finishes and its handle dies. -/
theorem spawn_then_both_release (n : Nat) :
    run spawned (awaits n ++ [.release, .release]) = some .freed :=
  run_valid (awaits_valid 1 n _ 0 (.release (.release (.done 0))))

end FiberLifetime
