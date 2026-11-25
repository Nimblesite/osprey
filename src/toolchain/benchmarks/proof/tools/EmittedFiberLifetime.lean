/- [IR-LEAN-ATOMIC] [CONCURRENCY-FIBER-LIFETIME] The holder count, proved of the
   code that runs. `EmittedRuntime` is generated on every run by
   scripts/proof/fiber-ir-lean.sh from the fiber runtime as clang compiles it
   (`held_hold` and `held_release` of handle_lifetime.h), so each statement is
   about those instructions, for every count at once.

   A held object is one allocation: the holders word at byte 0, the dead-list
   link at 8, the address of its `free` at 16. `freeing` stands for that
   function: it frees the object it is handed, and is the only function there
   is, so a run that ends with the object freed called exactly it, with
   exactly the object.

   `hold` and the three `release` theorems say what the compiled code does to
   the word and the object. The `_is_step` theorems say that this is
   `FiberLifetime.step`: the compiled `held_hold` is the model's `acquire`,
   the compiled `held_release` its `release`. Each compiled function touches
   the word through one `atomicrmw` and nowhere else, which the model takes as
   one step, so `FiberLifetime`'s theorems about every interleaving of steps
   are theorems about every interleaving of these functions. -/
import LlvmSubset
import FiberLifetime
import EmittedRuntime
namespace EmittedFiberLifetime
open LlvmSubset
open FiberLifetime (Fiber step ofHolders)

/-- The number the object's `free` field holds: its function's address. -/
def freeAt : Nat := 7

/-- The object's `free`: frees what it is handed. No other function exists. -/
def freeing : CallFn := fun index args =>
  match args with
  | [v] => if index = freeAt then do
      freeVal 2 v
      pure (.int 0)
    else fault .noFunction
  | _ => fault .badArguments

/-- A held object whose holders word is `word`, at allocation 0. -/
def held (word : Int) : St :=
  St.empty.allocated true 24 0 [(0, .int word), (8, .int 0), (16, .int freeAt)] .raw

/-- Run the compiled function numbered `index` on `args`. -/
def run (index : Nat) (args : List Val) : M Val :=
  match EmittedRuntime.module.funcs[index]? with
  | some ⟨entry :: rest⟩ => runBlocks freeing 4 ⟨entry :: rest⟩ 8 entry (entry.params.zip args)
  | _ => fault .noFunction

/-- What is left: whether the object is still allocated, and its holders word. -/
structure Seen where
  alive : Bool
  word : Option Val
  deriving DecidableEq, Repr

def seen (ran : Except Fault (Val × St)) : Except Fault Seen :=
  ran.map fun (_, s) =>
    match s.mem[0]? with
    | some blk => ⟨blk.live, blk.cells.lookup 0⟩
    | none => ⟨false, none⟩

/-- `held_hold`: the word goes up by one and the object stays. -/
theorem hold (word : Int) :
    seen (run EmittedRuntime.«held_hold» [.ptr 0 0] (held word))
      = .ok ⟨true, some (.int (wrap 64 (word + 1)))⟩ := by kernel_rfl

/-- `held_release` by the last holder: the object is freed, through its own
    `free`, handed the object itself. -/
theorem release_last :
    seen (run EmittedRuntime.«held_release» [.ptr 0 0] (held 1))
      = .ok ⟨false, some (.int 0)⟩ := by kernel_rfl

/-- `held_release` by any other holder: the word goes down by one, the object
    stays and nothing is called. -/
theorem release_more (others : Nat) :
    seen (run EmittedRuntime.«held_release» [.ptr 0 0] (held ((others + 2 : Nat) : Int)))
      = .ok ⟨true, some (.int (wrap 64 (((others + 2 : Nat) : Int) - 1)))⟩ := by kernel_rfl

/-- `held_release` of no object touches nothing. -/
theorem release_null (word : Int) :
    seen (run EmittedRuntime.«held_release» [.int 0] (held word))
      = .ok ⟨true, some (.int word)⟩ := by kernel_rfl

/-- A count that fits the word is the word. -/
theorem wrap_count (count : Nat) (fits : count < 2 ^ 63) : wrap 64 (count : Int) = count := by
  unfold wrap Int.bmod
  simp
  omega

/-- The model state a result stands for. -/
def model : Seen → Option Fiber
  | ⟨false, _⟩ => some .freed
  | ⟨true, some (.int word)⟩ => some (ofHolders word.toNat)
  | ⟨true, _⟩ => none

/-- The compiled `held_hold` is the model's `acquire`. -/
theorem hold_is_step (others : Nat) (fits : others + 2 < 2 ^ 63) :
    (seen (run EmittedRuntime.«held_hold» [.ptr 0 0] (held ((others + 1 : Nat) : Int)))).map model
      = .ok (step (ofHolders (others + 1)) .acquire) := by
  rw [hold, show ((others + 1 : Nat) : Int) + 1 = ((others + 2 : Nat) : Int) from rfl,
    wrap_count (others + 2) fits]
  exact congrArg Except.ok (FiberLifetime.acquire_one others).symm

/-- The compiled `held_release` by the last holder is the model's last `release`. -/
theorem release_last_is_step :
    (seen (run EmittedRuntime.«held_release» [.ptr 0 0] (held 1))).map model
      = .ok (step (ofHolders 1) .release) := by
  rw [release_last]
  exact congrArg Except.ok (FiberLifetime.release_one 0).symm

/-- The compiled `held_release` by any other holder is the model's `release`. -/
theorem release_more_is_step (others : Nat) (fits : others + 2 < 2 ^ 63) :
    (seen (run EmittedRuntime.«held_release» [.ptr 0 0] (held ((others + 2 : Nat) : Int)))).map model
      = .ok (step (ofHolders (others + 2)) .release) := by
  rw [release_more, show ((others + 2 : Nat) : Int) - 1 = ((others + 1 : Nat) : Int) by omega,
    wrap_count (others + 1) (by omega)]
  exact congrArg Except.ok (FiberLifetime.release_one (others + 1)).symm

end EmittedFiberLifetime
