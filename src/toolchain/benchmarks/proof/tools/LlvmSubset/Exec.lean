/- [IR-LEAN-EXEC] An executable semantics for the subset: one function runs
   block by block against tracked memory. It is total (fuel bounds calls and
   block transitions), so Lean can evaluate it inside a proof and compile it
   for the differential check against the real binary. -/
import LlvmSubset.Force
namespace LlvmSubset


abbrev Env := List (Nat × Val)

/-- An integer brought back into `bits` bits, two's complement. -/
def wrap (bits : Nat) (v : Int) : Int := if bits = 1 then v % 2 else Int.bmod v (2 ^ bits)

/-- The same bits read as an unsigned number. -/
def unsigned (bits : Nat) (v : Int) : Int := if bits = 1 then v else v % (2 ^ bits : Nat)

def Ty.bits : Ty → Nat
  | .int bits => bits
  | _ => 64

/-- The value a register holds. -/
def evalOp (env : Env) (id : Nat) : Except Fault Val :=
  match env.lookup id with
  | some v => .ok v
  | none => .error .undefinedRegister

def evalAll (env : Env) : List Nat → Except Fault (List Val)
  | [] => .ok []
  | id :: rest => do
    let v ← evalOp env id
    let more ← evalAll env rest
    .ok (v :: more)

def evalIndices (env : Env) : List Index → Except Fault (List Val)
  | [] => .ok []
  | .const v :: rest => do
    let more ← evalIndices env rest
    .ok (.int v :: more)
  | .reg id :: rest => do
    let v ← evalOp env id
    let more ← evalIndices env rest
    .ok (v :: more)

def loadAt (ty : Ty) : Val → M Val
  | .int _ => fault .badPointer
  | .ptr b off => do
    let blk ← liveBlk .useAfterFree b
    if off + ty.size ≤ blk.size then
      match blk.cells.lookup off with
      | some v => do
        touch b blk.cells blk.obj
        pure v
      | none => fault .uninitialised
    else fault .outOfBounds

def storeAt (ty : Ty) (v : Val) : Val → M Unit
  | .int _ => fault .badPointer
  | .ptr b off => do
    let blk ← liveBlk .useAfterFree b
    if off + ty.size ≤ blk.size then
      touch b ((off, v) :: blk.cells.filter (fun cell => cell.1 != off)) blk.obj
    else fault .outOfBounds

/-- Offset the indices after the first add, walking into `ty`. -/
def gepOffset : Ty → List Val → Option Nat
  | _, [] => some 0
  | .struct fields, .int index :: rest =>
    if index < 0 then none else
    match fieldOffset fields index.toNat 0 with
    | some (offset, field) => (offset + ·) <$> gepOffset field rest
    | none => none
  | .array _ elem, .int index :: rest =>
    if index < 0 then none else (index.toNat * elem.size + ·) <$> gepOffset elem rest
  | _, _ => none

def gep (ty : Ty) (base : Val) : List Val → Except Fault Val
  | .int first :: rest =>
    if first < 0 then .error .outOfBounds else
    match gepOffset ty rest, base with
    | some offset, .ptr b off => .ok (.ptr b (off + first.toNat * ty.size + offset))
    | some offset, .int n => .ok (.int (n + (first.toNat * ty.size + offset : Nat)))
    | none, _ => .error .unsupported
  | _ => .error .unsupported

def cast (op : CastOp) (source target : Ty) : Val → Val
  | .int v => match op with
    | .zext => .int (unsigned source.bits v)
    | .trunc => .int (wrap target.bits v)
    | _ => .int v
  | pointer => pointer

def bitwise (op : Nat → Nat → Nat) (a b : Int) : Except Fault Int :=
  if a < 0 ∨ b < 0 then .error .unsupported else .ok (op a.toNat b.toNat : Nat)

def binary (op : BinOp) (bits : Nat) : Val → Val → Except Fault Val
  | .int a, .int b => do
    let exact ← match op with
      | .add => .ok (a + b) | .sub => .ok (a - b) | .mul => .ok (a * b)
      | .and => bitwise Nat.land a b | .or => bitwise Nat.lor a b
      | .xor => bitwise Nat.xor a b
      | .shl => if b < 0 then .error .unsupported else .ok (a * (2 ^ b.toNat : Nat))
    .ok (.int (wrap bits exact))
  | _, _ => .error .unsupported

def compare (pred : Pred) (bits : Nat) (left right : Val) : Except Fault Val :=
  let bit (holds : Bool) : Except Fault Val := .ok (.int (if holds then 1 else 0))
  match pred, left, right with
  | .eq, a, b => bit (a == b)
  | .ne, a, b => bit (a != b)
  | .slt, .int a, .int b => bit (a < b)
  | .sle, .int a, .int b => bit (a ≤ b)
  | .sgt, .int a, .int b => bit (b < a)
  | .sge, .int a, .int b => bit (b ≤ a)
  | .ult, .int a, .int b => bit (unsigned bits a < unsigned bits b)
  | .ule, .int a, .int b => bit (unsigned bits a ≤ unsigned bits b)
  | .ugt, .int a, .int b => bit (unsigned bits b < unsigned bits a)
  | .uge, .int a, .int b => bit (unsigned bits b ≤ unsigned bits a)
  | _, _, _ => .error .unsupported

<<<<<<< HEAD
=======
/-- The word an `atomicrmw` leaves, from the word it found and its operand. -/
def rmw (op : RmwOp) (bits : Nat) (found operand : Val) : Except Fault Val :=
  match op with
  | .xchg => .ok operand
  | .add => binary .add bits found operand
  | .sub => binary .sub bits found operand

>>>>>>> origin/fixes
/-- The values a dying allocation owns: the words its layout mask marks
    ([GC-ARC-PERCEUS]: bit `8 + i` of the layout word is the word at byte
    `8 * i`), or a list's managed elements unless a newer version took them. -/
def children (blk : Blk) : Except Fault (List Val) :=
  match blk.obj with
  | .list elems managed handedOver => .ok (if managed && !handedOver then elems else [])
  | .raw =>
    if blk.layout % 256 = 1 ∨ blk.layout % 256 = 5 then
      ((List.range 56).filter (fun word => (blk.layout >>> (8 + word)) % 2 = 1)).mapM
        (fun word => match blk.cells.lookup (8 * word) with
          | some v => .ok v
          | none => .error .uninitialised)
    else .ok []

def freeEach (free : Val → M Unit) : List Val → M Unit
  | [] => pure ()
  | v :: rest => do
    settled (free v)
    freeEach free rest

/-- `osp_ownership_free`: free the allocation, then everything it owns. A
    null or a foreign word has no owner to release. -/
def freeVal : Nat → Val → M Unit
  | 0, _ => fault .fuel
  | _ + 1, .int _ => pure ()
  | fuel + 1, .ptr b off =>
    if off ≠ 0 then fault .freeInside else do
      let blk ← liveBlk .doubleFree b
      if blk.heap then do
        let owned ← liftM (children blk)
        killBlk b
        freeEach (freeVal fuel) owned
      else fault .freeOfStack

/-- Zeroed words for `size` bytes. -/
def zeroed (size : Nat) : List (Nat × Val) :=
  (List.range (size / 8)).map (fun word => (8 * word, .int 0))

def allocated (size layout : Int) (cells : List (Nat × Val)) : M Val :=
  if size < 0 ∨ layout < 0 then fault .badArguments else do
    let b ← allocBlk true size.toNat layout.toNat cells .raw
    pure (.ptr b 0)

def appendTo (source : Nat) (v : Val) (managed : Bool) : M Val := do
  let blk ← liveBlk .useAfterFree source
  match blk.obj with
  | .list elems _ false => do
    touch source blk.cells (.list elems managed true)
    let b ← allocBlk true 0 0 [] (.list (elems ++ [v]) managed false)
    pure (.ptr b 0)
  | .list _ _ true => fault .movedFrom
  | .raw => fault .badPointer

def lengthOf (source : Nat) : M Val := do
  let blk ← liveBlk .useAfterFree source
  match blk.obj with
  | .list elems _ _ => do
    touch source blk.cells blk.obj
    pure (.int elems.length)
  | .raw => fault .badPointer

/-- The runtime entry points, each at the level the emitted code can observe. -/
def callExt (fuel : Nat) : Ext → List Val → M Val
  | .osp_alloc, [.int size] => allocated size 0 []
  | .osp_alloc_tagged, [.int size, .int layout] => allocated size layout (zeroed size.toNat)
  | .osp_alloc_tagged_noinit, [.int size, .int layout] => allocated size layout []
  | .osp_ownership_free, [v] => do
    freeVal fuel v
    pure (.int 0)
  | .osprey_list_append_of, [.int 0, v, .int managed] => do
    let b ← allocBlk true 0 0 [] (.list [v] (managed != 0) false)
    pure (.ptr b 0)
  | .osprey_list_append_of, [.ptr source 0, v, .int managed] => appendTo source v (managed != 0)
  | .osprey_list_length, [.ptr source 0] => lengthOf source
  | _, _ => fault .badArguments

abbrev CallFn := Nat → List Val → M Val

<<<<<<< HEAD
=======
/-- [IR-LEAN-ATOMIC] `atomicrmw`: the word at `p` becomes `rmw` of itself, and
    the word it was is the answer. The interpreter runs one thread, so nothing
    comes between the read and the write: the one indivisible step the
    instruction is among threads. A model of interleavings takes each such
    step whole. -/
def rmwAt (op : RmwOp) (ty : Ty) (p operand : Val) : M Val := do
  let found ← loadAt ty p
  let left ← liftM (rmw op ty.bits found operand)
  storeAt ty left p
  pure found

/-- A call through a function pointer: the function whose number it holds. -/
def callAt (callFn : CallFn) (values : List Val) : Val → M Val
  | .int index => if index < 0 then fault .badPointer else callFn index.toNat values
  | .ptr _ _ => fault .badPointer

>>>>>>> origin/fixes
def execInstr (callFn : CallFn) (fuel : Nat) (env : Env) : Instr → M Env
  | .const dst v => pure ((dst, .int v) :: env)
  | .alloca dst ty => do
    let b ← allocBlk false ty.size 0 [] .raw
    pure ((dst, .ptr b 0) :: env)
  | .store ty value pointer => do
    let v ← liftM (evalOp env value)
    let p ← liftM (evalOp env pointer)
    storeAt ty v p
    pure env
  | .load dst ty pointer => do
    let p ← liftM (evalOp env pointer)
    let v ← loadAt ty p
    pure ((dst, v) :: env)
  | .gep dst ty base indices => do
    let v ← liftM (do
      let p ← evalOp env base
      let at' ← evalIndices env indices
      gep ty p at')
    pure ((dst, v) :: env)
  | .cast dst op source value target => do
    let v ← liftM (evalOp env value)
    pure ((dst, cast op source target v) :: env)
  | .bin dst op ty left right => do
    let v ← liftM (do
      let a ← evalOp env left
      let b ← evalOp env right
      binary op ty.bits a b)
    pure ((dst, v) :: env)
  | .icmp dst pred ty left right => do
    let v ← liftM (do
      let a ← evalOp env left
      let b ← evalOp env right
      compare pred ty.bits a b)
    pure ((dst, v) :: env)
  | .select dst condition whenTrue whenFalse => do
    let v ← liftM (do
      let c ← evalOp env condition
      evalOp env (if c = Val.int 0 then whenFalse else whenTrue))
    pure ((dst, v) :: env)
<<<<<<< HEAD
=======
  | .atomicrmw dst op ty pointer operand => do
    let p ← liftM (evalOp env pointer)
    let by' ← liftM (evalOp env operand)
    let found ← rmwAt op ty p by'
    pure ((dst, found) :: env)
  | .call dst (.reg id) args => do
    let target ← liftM (evalOp env id)
    let values ← liftM (evalAll env args)
    let v ← callAt callFn values target
    pure (match dst with
      | some dst => (dst, v) :: env
      | none => env)
>>>>>>> origin/fixes
  | .call dst (.fn index) args => do
    let values ← liftM (evalAll env args)
    let v ← callFn index values
    pure (match dst with
      | some dst => (dst, v) :: env
      | none => env)
  | .call dst (.ext name) args => do
    let values ← liftM (evalAll env args)
    let v ← callExt fuel name values
    pure (match dst with
      | some dst => (dst, v) :: env
      | none => env)

def execBody (callFn : CallFn) (fuel : Nat) : List Instr → Env → M Env
  | [], env => pure env
  | instr :: rest, env => forceCells env fun env => do
    let env ← settled (execInstr callFn fuel env instr)
    execBody callFn fuel rest env

/-- The block a branch enters and the registers it enters with: the target's
    parameters bound to the arguments, every argument read before any binds. -/
def enter (f : Func) (env : Env) (target : Nat) (args : List Nat) : Except Fault (Block × Env) :=
  match f.blocks[target]? with
  | none => .error .noBlock
  | some block =>
    if block.params.length = args.length then do
      let values ← evalAll env args
      .ok (block, block.params.zip values ++ env)
    else .error .badPhi

/-- Where a block's terminator leads: a returned value, or the next block. -/
def leave (f : Func) (env : Env) : Term → Except Fault (Val ⊕ (Block × Env))
  | .ret none => .ok (.inl (Val.int 0))
  | .ret (some id) => .inl <$> evalOp env id
  | .br target args => .inr <$> enter f env target args
  | .condBr condition whenTrue trueArgs whenFalse falseArgs => do
    let c ← evalOp env condition
    .inr <$> (if c = Val.int 0 then enter f env whenFalse falseArgs else enter f env whenTrue trueArgs)
  | .unreachable => .error .unreachable

def runBlocks (callFn : CallFn) (fuel : Nat) (f : Func) : Nat → Block → Env → M Val
  | 0, _, _ => fault .fuel
  | steps + 1, block, env => do
    let env ← execBody callFn fuel block.body env
    let next ← liftM (leave f env block.term)
    match next with
    | .inl v => pure v
    | .inr (block, env) => runBlocks callFn fuel f steps block env

/-- Run function `index` of `m` on `args`. `fuel` bounds the call depth and
    the block transitions of each call. -/
def runFn (m : Module) : Nat → CallFn
  | 0, _, _ => fault .fuel
  | fuel + 1, index, args =>
    match m.funcs[index]? with
    | none => fault .noFunction
    | some f =>
      match f.blocks with
      | [] => fault .noBlock
      | entry :: _ =>
        if entry.params.length = args.length then
          runBlocks (runFn m fuel) fuel f (fuel + 1) entry (entry.params.zip args)
        else fault .badArguments

end LlvmSubset
