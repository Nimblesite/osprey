/- [CONCURRENCY-FIBER-LIFETIME] [CONCURRENCY-CHANNEL-LIFETIME] The holder count
   that decides when a fiber or a channel is freed
   (src/toolchain/compiler/runtime/handle_lifetime.h).

   A fiber is a future (Halstead, Multilisp, TOPLAS 1985) whose storage is
   reclaimed once nothing can reach it (Baker and Hewitt, The Incremental
   Garbage Collection of Processes, MIT AI Memo 454, 1977). The runtime keeps
   that reachability as a count of holders: the program's handle, the runner
   until the body has finished, and each await or probe while it reads. A
   fiber is `live n` while `n + 1` holders remain, and `freed` once the last
   one has let go.

   `held_hold` is `acquire`, `held_release` is `release`, and a spawn starts
   at `spawned`: the handle and the runner. A channel counts the same way from
   `made`, its handle alone, with each send, receive and select holding it
   while it reads. Taking or ending a hold on a freed
   fiber is the use after free the runtime must never make, so `step` refuses
   it with `none`.

   The theorems: every trace in which only holders act runs without a use
   after free and ends with exactly as many holders as it says (`run_valid`);
   the fiber is freed exactly when the last holder lets go (`freed_iff`); and a
   spawned fiber whose handle and runner both let go, with every await
   balanced, is freed (`spawn_then_both_release`), the leak #359 was; a channel
   is freed once its handle dies, however often it was used
   (`channel_then_handle_release`). Under ownership a handle is the one block a
   program shares ([MEM-OWNERSHIP-STATIC-HANDLES]); its header counts the
   sharers, and that count is this holder count (`share_is_acquire`,
   `free_is_release`), so only the last sharer's free ends the handle
   (`free_last_iff`). For the
   list handle finalizers hand their holds over on: whatever the interleaving
   of pushes and drains, no fiber is lost or duplicated
   (`dead_list_conserves`), and a final drain releases each pushed fiber
   exactly as often as it was pushed (`drained_releases_each_once`). -/
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

/-- A channel has one holder when it is made: its handle. -/
def made : Fiber := .live 0

/-- A channel, used any number of times, is freed once its handle dies. -/
theorem channel_then_handle_release (n : Nat) :
    run made (awaits n ++ [.release]) = some .freed :=
  run_valid (awaits_valid 0 n _ 0 (.release (.done 0)))

/- The header word of a handle under ownership (`memory_ownership.c`):
   `OSP_OWN_OWNED` is `owned`, one sharer, and `OSP_OWN_HELD + n + 1` is
   `held n`, the first sharer and `n + 1` more. `osp_ownership_share` and
   `osp_retain` are `share`; `OSP_HEAP_DROP_LAST` is `free`, answering `none`
   when the free was the last, which finalizes and frees the block. -/

inductive Header where
  | owned
  | held (more : Nat)

def share : Header → Header
  | .owned => .held 0
  | .held more => .held (more + 1)

def free : Header → Option Header
  | .owned => none
  | .held 0 => some .owned
  | .held (more + 1) => some (.held more)

def sharers : Header → Nat
  | .owned => 1
  | .held more => more + 2

/-- The holder count a header stands for. -/
def counted (header : Header) : Fiber := ofHolders (sharers header)

def afterFree : Option Header → Fiber
  | none => .freed
  | some header => counted header

/-- Sharing a handle takes exactly one hold. -/
theorem share_is_acquire (header : Header) :
    step (counted header) .acquire = some (counted (share header)) :=
  match header with
  | .owned => rfl
  | .held _ => rfl

/-- A sharer's free ends exactly one hold. -/
theorem free_is_release (header : Header) :
    step (counted header) .release = some (afterFree (free header)) :=
  match header with
  | .owned => rfl
  | .held 0 => rfl
  | .held (_ + 1) => rfl

/-- The handle is finalized and freed exactly by its last sharer. -/
theorem free_last_iff (header : Header) : free header = none ↔ sharers header = 1 :=
  match header with
  | .owned => ⟨fun _ => rfl, fun _ => rfl⟩
  | .held 0 => ⟨fun h => Option.noConfusion h, fun h => Nat.noConfusion (Nat.succ.inj h)⟩
  | .held (_ + 1) => ⟨fun h => Option.noConfusion h, fun h => Nat.noConfusion (Nat.succ.inj h)⟩

/- The dead-handle list (`held_handle_finalize` and `held_reclaim`). A
   finalizer pushes one fiber with a compare-and-swap; the runtime drains by
   exchanging the whole list for the empty one, then releases each fiber it
   took. Both are single atomic steps, so any interleaving is a sequence of
   `push` and `drain`. `pending` is the list; `released` is every fiber a drain
   has handed to `held_release`. -/

inductive DeadOp where
  | push (fiber : Nat)
  | drain

structure DeadList where
  pending : List Nat
  released : List Nat

def deadStep : DeadList → DeadOp → DeadList
  | ⟨pending, released⟩, .push fiber => ⟨fiber :: pending, released⟩
  | ⟨pending, released⟩, .drain => ⟨[], pending ++ released⟩

def deadRun : DeadList → List DeadOp → DeadList
  | list, [] => list
  | list, op :: rest => deadRun (deadStep list op) rest

/-- The fibers the finalizers pushed. -/
def pushed : List DeadOp → List Nat
  | [] => []
  | .push fiber :: rest => fiber :: pushed rest
  | .drain :: rest => pushed rest

/-- How many times `fiber` occurs in `fibers`. -/
def occurrences (fiber : Nat) : List Nat → Nat
  | [] => 0
  | other :: rest => (if fiber == other then 1 else 0) + occurrences fiber rest

theorem occurrences_append (fiber : Nat) (left right : List Nat) :
    occurrences fiber (left ++ right) = occurrences fiber left + occurrences fiber right := by
  induction left with
  | nil => exact (Nat.zero_add _).symm
  | cons other rest ih =>
    show (if fiber == other then 1 else 0) + occurrences fiber (rest ++ right) = _
    rw [ih]
    exact (Nat.add_assoc _ _ _).symm

/-- Every fiber in the list or released, counted. -/
def held (fiber : Nat) (list : DeadList) : Nat :=
  occurrences fiber list.pending + occurrences fiber list.released

theorem held_step (fiber : Nat) (list : DeadList) (op : DeadOp) :
    held fiber (deadStep list op) = held fiber list + occurrences fiber (pushed [op]) := by
  cases list with
  | mk pending released =>
    cases op with
    | push other =>
      show (if fiber == other then 1 else 0) + occurrences fiber pending + occurrences fiber released =
        occurrences fiber pending + occurrences fiber released + ((if fiber == other then 1 else 0) + 0)
      rw [Nat.add_zero, Nat.add_assoc, Nat.add_comm]
    | drain =>
      show 0 + occurrences fiber (pending ++ released) =
        occurrences fiber pending + occurrences fiber released + 0
      rw [Nat.zero_add, Nat.add_zero, occurrences_append]

theorem pushed_cons (op : DeadOp) (rest : List DeadOp) :
    pushed (op :: rest) = pushed [op] ++ pushed rest :=
  match op with
  | .push _ => rfl
  | .drain => rfl

/-- No fiber is lost or duplicated: whatever the interleaving, every pushed
    fiber is still in the list or released, exactly as often as it was pushed. -/
theorem dead_list_conserves (fiber : Nat) (ops : List DeadOp) (list : DeadList) :
    held fiber (deadRun list ops) = held fiber list + occurrences fiber (pushed ops) := by
  induction ops generalizing list with
  | nil => exact (Nat.add_zero _).symm
  | cons op rest ih =>
    show held fiber (deadRun (deadStep list op) rest) = _
    have split : occurrences fiber (pushed (op :: rest)) =
        occurrences fiber (pushed [op]) + occurrences fiber (pushed rest) := by
      rw [pushed_cons op rest, occurrences_append]
    rw [ih (deadStep list op), held_step, split, Nat.add_assoc]

theorem drained_pending (list : DeadList) (ops : List DeadOp) :
    (deadRun list (ops ++ [.drain])).pending = [] := by
  induction ops generalizing list with
  | nil => cases list; rfl
  | cons op rest ih => exact ih (deadStep list op)

theorem pushed_drain (ops : List DeadOp) : pushed (ops ++ [.drain]) = pushed ops := by
  induction ops with
  | nil => rfl
  | cons op rest ih =>
    cases op with
    | push fiber => exact congrArg (fiber :: ·) ih
    | drain => exact ih

/-- After a final drain, each fiber a finalizer pushed has been released
    exactly as many times as it was pushed: its handle's hold ends once. -/
theorem drained_releases_each_once (fiber : Nat) (ops : List DeadOp) :
    occurrences fiber (deadRun ⟨[], []⟩ (ops ++ [.drain])).released =
      occurrences fiber (pushed ops) := by
  have conserved := dead_list_conserves fiber (ops ++ [.drain]) ⟨[], []⟩
  unfold held at conserved
  rw [drained_pending, pushed_drain] at conserved
  exact (Nat.zero_add _).symm.trans (conserved.trans (Nat.zero_add _))

end FiberLifetime
