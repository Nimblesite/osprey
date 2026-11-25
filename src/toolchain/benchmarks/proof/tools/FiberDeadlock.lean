/- [CONCURRENCY-DEADLOCK] Why a fiber the collector reports as blocked for ever
   can never be woken (src/toolchain/compiler/runtime/memory_gc_waits.h).

   The rule is Saioc, Lee, Møller and Chabbi's ("Dynamic Partial Deadlock
   Detection and Recovery via Garbage Collection", ASPLOS 2025). A fiber
   waiting on a channel is woken only by an operation on that channel, and an
   operation needs a reference to it. `Live` is the collector's fixpoint: a
   fiber is live if it runs, or if it waits on a channel a live fiber can
   name. A fiber that is not live is `Stuck`, which is what a collection
   counts.

   A `World` is what a collection sees: the references each fiber's stack and
   record hold, the references between objects, and the channel each waiting
   fiber waits on. The initial thread and the program's globals are a fiber
   that never waits. `Step` is everything a running fiber can do to that
   picture: store a reference it can name into an object it can name, hand a
   reference it can name to any fiber (a load, a spawn's captures, a received
   value), start waiting on a channel it can name, and wake a fiber waiting on
   a channel it can name. Dropping references only shrinks what can be named.

   The theorems: no step makes a fiber live that was not (`step_keeps_stuck`),
   so across any run a stuck fiber stays stuck (`stuck_for_ever`) and still
   waits (`stuck_never_runs`): reporting it is never a false alarm. The model
   covers the rule, not the C loop that computes the fixpoint, which the
   runtime's suite checks. -/
namespace FiberDeadlock

structure World (Fiber Obj : Type) where
  holds : Fiber → Obj → Prop
  edge : Obj → Obj → Prop
  waits : Fiber → Obj → Prop

variable {Fiber Obj : Type}

/-- Fiber `f` can name object `o`. -/
inductive Reach (w : World Fiber Obj) (f : Fiber) : Obj → Prop
  | held {o : Obj} : w.holds f o → Reach w f o
  | via {a b : Obj} : Reach w f a → w.edge a b → Reach w f b

def Runs (w : World Fiber Obj) (f : Fiber) : Prop := ∀ c, ¬ w.waits f c

/-- The collector's fixpoint. -/
inductive Live (w : World Fiber Obj) : Fiber → Prop
  | runs {f : Fiber} : Runs w f → Live w f
  | woken {f g : Fiber} {c : Obj} : w.waits f c → Live w g → Reach w g c → Live w f

/-- What a collection reports. -/
def Stuck (w : World Fiber Obj) (f : Fiber) : Prop := ¬ Live w f

inductive Step (w : World Fiber Obj) : World Fiber Obj → Prop
  | store {g : Fiber} {a b : Obj} : Runs w g → Reach w g a → Reach w g b →
      Step w ⟨w.holds, fun x y => w.edge x y ∨ (x = a ∧ y = b), w.waits⟩
  | give {g h : Fiber} {o : Obj} : Runs w g → Reach w g o →
      Step w ⟨fun f x => w.holds f x ∨ (f = h ∧ x = o), w.edge, w.waits⟩
  | block {g : Fiber} {c : Obj} : Runs w g → Reach w g c →
      Step w ⟨w.holds, w.edge, fun f x => w.waits f x ∨ (f = g ∧ x = c)⟩
  | wake {g f : Fiber} {c : Obj} : Runs w g → Reach w g c → w.waits f c →
      Step w ⟨w.holds, w.edge, fun x y => w.waits x y ∧ x ≠ f⟩

/-- An object some live fiber can name. -/
def Known (w : World Fiber Obj) (o : Obj) : Prop := ∃ g, Live w g ∧ Reach w g o

/-- Whatever a live fiber can name after a step, a live fiber could name before it. -/
theorem step_keeps_known {w w' : World Fiber Obj} (step : Step w w') {g : Fiber} {o : Obj}
    (live : Live w g) (reach : Reach w' g o) : Known w o := by
  cases step with
  | store runs reachA reachB =>
    induction reach with
    | held h => exact ⟨g, live, .held h⟩
    | via _ e ih =>
      cases e with
      | inl old => exact match ih with | ⟨k, liveK, reachK⟩ => ⟨k, liveK, .via reachK old⟩
      | inr new => exact new.2 ▸ ⟨_, .runs runs, reachB⟩
  | give runs reachO =>
    induction reach with
    | held h =>
      cases h with
      | inl old => exact ⟨g, live, .held old⟩
      | inr new => exact new.2 ▸ ⟨_, .runs runs, reachO⟩
    | via _ e ih => exact match ih with | ⟨k, liveK, reachK⟩ => ⟨k, liveK, .via reachK e⟩
  | block _ _ =>
    induction reach with
    | held h => exact ⟨g, live, .held h⟩
    | via _ e ih => exact match ih with | ⟨k, liveK, reachK⟩ => ⟨k, liveK, .via reachK e⟩
  | wake _ _ _ =>
    induction reach with
    | held h => exact ⟨g, live, .held h⟩
    | via _ e ih => exact match ih with | ⟨k, liveK, reachK⟩ => ⟨k, liveK, .via reachK e⟩

/-- No step makes a fiber live that was not. -/
theorem step_keeps_live [DecidableEq Fiber] {w w' : World Fiber Obj} (step : Step w w')
    {f : Fiber} (live : Live w' f) : Live w f := by
  induction live with
  | @runs f runs' =>
    cases step with
    | store _ _ _ => exact .runs runs'
    | give _ _ => exact .runs runs'
    | block _ _ => exact .runs fun c waits => runs' c (.inl waits)
    | @wake g woke c runsG reachC waitsC =>
      exact if same : f = woke then same ▸ .woken waitsC (.runs runsG) reachC
        else .runs fun c waits => runs' c ⟨waits, same⟩
  | @woken f g c waits' _ reach' liveG =>
    have known : Known w c := step_keeps_known step liveG reach'
    cases step with
    | store _ _ _ => exact match known with | ⟨k, liveK, reachK⟩ => .woken waits' liveK reachK
    | give _ _ => exact match known with | ⟨k, liveK, reachK⟩ => .woken waits' liveK reachK
    | block runsG _ =>
      cases waits' with
      | inl old => exact match known with | ⟨k, liveK, reachK⟩ => .woken old liveK reachK
      | inr new => exact new.1 ▸ .runs runsG
    | wake _ _ _ => exact match known with | ⟨k, liveK, reachK⟩ => .woken waits'.1 liveK reachK

/-- A fiber the collector reports stays reportable after any step. -/
theorem step_keeps_stuck [DecidableEq Fiber] {w w' : World Fiber Obj} (step : Step w w')
    {f : Fiber} (stuck : Stuck w f) : Stuck w' f :=
  fun live => stuck (step_keeps_live step live)

/-- Any number of steps. -/
inductive Steps : World Fiber Obj → World Fiber Obj → Prop
  | none (w : World Fiber Obj) : Steps w w
  | more {w mid w' : World Fiber Obj} : Step w mid → Steps mid w' → Steps w w'

/-- A reported fiber is stuck for ever, whatever the rest of the program does. -/
theorem stuck_for_ever [DecidableEq Fiber] {w w' : World Fiber Obj} (steps : Steps w w')
    {f : Fiber} (stuck : Stuck w f) : Stuck w' f := by
  induction steps with
  | none _ => exact stuck
  | more step _ ih => exact ih (step_keeps_stuck step stuck)

/-- So it never runs again: reporting it is never a false alarm. -/
theorem stuck_never_runs [DecidableEq Fiber] {w w' : World Fiber Obj} (steps : Steps w w')
    {f : Fiber} (stuck : Stuck w f) : ¬ Runs w' f :=
  fun runs => stuck_for_ever steps stuck (.runs runs)

/-- A fiber waiting on a channel a running fiber can name is never reported. -/
theorem reachable_wait_is_live {w : World Fiber Obj} {f g : Fiber} {c : Obj}
    (waits : w.waits f c) (runs : Runs w g) (reach : Reach w g c) : Live w f :=
  .woken waits (.runs runs) reach

end FiberDeadlock
