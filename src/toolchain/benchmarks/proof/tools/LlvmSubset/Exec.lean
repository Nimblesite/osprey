/- [IR-LEAN-EXEC] An executable semantics for the subset: one function runs
   block by block against tracked memory. It is total (fuel bounds calls and
   block transitions), so Lean can evaluate it inside a proof and compile it
   for the differential check against the real binary. -/
import LlvmSubset.Memory
namespace LlvmSubset
variable {start : OwnershipLedger.Heap}

abbrev Env := List (Nat × Val)

/-- An integer brought back into `bits` bits, two's complement. -/
def wrap (bits : Nat) (v : Int) : Int := if bits = 1 then v % 2 else Int.bmod v (2 ^ bits)

/-- The same bits read as an unsigned number. -/
def unsigned (bits : Nat) (v : Int) : Int := if bits = 1 then v else v % (2 ^ bits : Nat)

def Ty.bits : Ty → Nat
  | .int bits => bits
  | _ => 64

def evalOp (env : Env) : Operand → Except Fault Val
  | .reg id => match env.lookup id with
    | some v => .ok v
    | none => .error .undefinedRegister
  | .const v => .ok (.int v)
  | .global _ => .error .unsupported

def evalAll (env : Env) : List Operand → Except Fault (List Val)
  | [] => .ok []
  | op :: rest => do
    let v ← evalOp env op
    let more ← evalAll env rest
    .ok (v :: more)

def loadAt (t : Tracked start) (ty : Ty) : Val → Except Fault (Val × Tracked start)
  | .int _ => .error .badPointer
  | .ptr b off => do
    let ⟨blk, found⟩ ← t.live b
    if off + ty.size ≤ blk.size then
      match blk.cells.lookup off with
      | some v => .ok (v, t.access b blk found blk.cells blk.obj)
      | none => .error .uninitialised
    else .error .outOfBounds

def storeAt (t : Tracked start) (ty : Ty) (v : Val) : Val → Except Fault (Tracked start)
  | .int _ => .error .badPointer
  | .ptr b off => do
    let ⟨blk, found⟩ ← t.live b
    if off + ty.size ≤ blk.size then
      .ok (t.access b blk found ((off, v) :: blk.cells.filter (fun cell => cell.1 != off)) blk.obj)
    else .error .outOfBounds

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

/-- `osp_ownership_free`: free the allocation, then everything it owns. A
    null or a foreign word has no owner to release. -/
def freeVal : Nat → Tracked start → Val → Except Fault (Tracked start)
  | 0, _, _ => .error .fuel
  | _ + 1, t, .int _ => .ok t
  | fuel + 1, t, .ptr b off =>
    if off ≠ 0 then .error .freeInside else
    match t.live b with
    | .error .useAfterFree => .error .doubleFree
    | .error fault => .error fault
    | .ok ⟨blk, found⟩ =>
      if blk.heap then do
        let owned ← children blk
        owned.foldlM (freeVal fuel) (t.kill b blk found)
      else .error .freeOfStack

/-- Zeroed words for `size` bytes. -/
def zeroed (size : Nat) : List (Nat × Val) := (List.range (size / 8)).map (fun word => (8 * word, .int 0))

def allocated (t : Tracked start) (size layout : Int) (cells : List (Nat × Val)) : Except Fault (Val × Tracked start) :=
  if size < 0 ∨ layout < 0 then .error .badArguments else
  let (b, t) := t.alloc true size.toNat layout.toNat cells .raw
  .ok (.ptr b 0, t)

/-- The runtime entry points, each at the level the emitted code can observe. -/
def callExt (fuel : Nat) (t : Tracked start) : Ext → List Val → Except Fault (Val × Tracked start)
  | .osp_alloc, [.int size] => allocated t size 0 []
  | .osp_alloc_tagged, [.int size, .int layout] => allocated t size layout (zeroed size.toNat)
  | .osp_alloc_tagged_noinit, [.int size, .int layout] => allocated t size layout []
  | .osp_ownership_free, [v] => (fun t => (Val.int 0, t)) <$> freeVal fuel t v
  | .osprey_list_append_of, [.int 0, v, .int managed] =>
    let (b, t) := t.alloc true 0 0 [] (.list [v] (managed != 0) false)
    .ok (.ptr b 0, t)
  | .osprey_list_append_of, [.ptr source 0, v, .int managed] => do
    let ⟨blk, found⟩ ← t.live source
    match blk.obj with
    | .list elems _ false =>
      let t := t.access source blk found blk.cells (.list elems (managed != 0) true)
      let (b, t) := t.alloc true 0 0 [] (.list (elems ++ [v]) (managed != 0) false)
      .ok (.ptr b 0, t)
    | .list _ _ true => .error .movedFrom
    | .raw => .error .badPointer
  | .osprey_list_length, [.ptr source 0] => do
    let ⟨blk, found⟩ ← t.live source
    match blk.obj with
    | .list elems _ _ => .ok (.int elems.length, t.access source blk found blk.cells blk.obj)
    | .raw => .error .badPointer
  | _, _ => .error .badArguments

abbrev CallFn (start : OwnershipLedger.Heap) :=
  Nat → List Val → Tracked start → Except Fault (Val × Tracked start)

def execInstr (callFn : CallFn start) (fuel : Nat) (env : Env) (t : Tracked start) :
    Instr → Except Fault (Env × Tracked start)
  | .alloca dst ty =>
    let (b, t) := t.alloc false ty.size 0 [] .raw
    .ok ((dst, .ptr b 0) :: env, t)
  | .store ty value pointer => do
    let v ← evalOp env value
    let p ← evalOp env pointer
    let t ← storeAt t ty v p
    .ok (env, t)
  | .load dst ty pointer => do
    let p ← evalOp env pointer
    let (v, t) ← loadAt t ty p
    .ok ((dst, v) :: env, t)
  | .gep dst ty base indices => do
    let p ← evalOp env base
    let at' ← evalAll env indices
    let v ← gep ty p at'
    .ok ((dst, v) :: env, t)
  | .cast dst op source value target => do
    let v ← evalOp env value
    .ok ((dst, cast op source target v) :: env, t)
  | .bin dst op ty left right => do
    let a ← evalOp env left
    let b ← evalOp env right
    let v ← binary op ty.bits a b
    .ok ((dst, v) :: env, t)
  | .icmp dst pred ty left right => do
    let a ← evalOp env left
    let b ← evalOp env right
    let v ← compare pred ty.bits a b
    .ok ((dst, v) :: env, t)
  | .select dst condition whenTrue whenFalse => do
    let c ← evalOp env condition
    let v ← evalOp env (if c = .int 0 then whenFalse else whenTrue)
    .ok ((dst, v) :: env, t)
  | .call dst callee args => do
    let values ← evalAll env args
    let (v, t) ← match callee with
      | .fn index => callFn index values t
      | .ext name => callExt fuel t name values
    .ok (match dst with
      | some dst => (dst, v) :: env
      | none => env, t)

def execBody (callFn : CallFn start) (fuel : Nat) : List Instr → Env → Tracked start → Except Fault (Env × Tracked start)
  | [], env, t => .ok (env, t)
  | instr :: rest, env, t => do
    let (env, t) ← execInstr callFn fuel env t instr
    execBody callFn fuel rest env t

/-- Every phi of a block, read against the registers of the incoming edge. -/
def execPhis (env : Env) (previous : Nat) : List Phi → Except Fault Env
  | [] => .ok []
  | phi :: rest =>
    match phi.incoming.lookup previous with
    | none => .error .badPhi
    | some op => do
      let v ← evalOp env op
      let more ← execPhis env previous rest
      .ok ((phi.dst, v) :: more)

def runBlocks (callFn : CallFn start) (fuel : Nat) (f : Func) :
    Nat → Nat → Nat → Env → Tracked start → Except Fault (Val × Tracked start)
  | 0, _, _, _, _ => .error .fuel
  | steps + 1, previous, current, env, t =>
    match f.blocks[current]? with
    | none => .error .noBlock
    | some block => do
      let bound ← execPhis env previous block.phis
      let (env, t) ← execBody callFn fuel block.body (bound ++ env) t
      match block.term with
      | .ret none => .ok (.int 0, t)
      | .ret (some op) => do
        let v ← evalOp env op
        .ok (v, t)
      | .br target => runBlocks callFn fuel f steps current target env t
      | .condBr condition whenTrue whenFalse => do
        let c ← evalOp env condition
        runBlocks callFn fuel f steps current (if c = .int 0 then whenFalse else whenTrue) env t
      | .unreachable => .error .unreachable

/-- Run function `index` of `m` on `args`. `fuel` bounds the call depth and
    the block transitions of each call. -/
def runFn (m : Module) : Nat → CallFn start
  | 0, _, _, _ => .error .fuel
  | fuel + 1, index, args, t =>
    match m.funcs[index]? with
    | none => .error .noFunction
    | some f =>
      if f.params.length = args.length then
        runBlocks (runFn m fuel) fuel f (fuel + 1) 0 0 (f.params.zip args) t
      else .error .badArguments

/-- Memory before anything is allocated. -/
def Tracked.empty : Tracked (fun _ => OwnershipLedger.dead) := ⟨[], [], .here _⟩

/-- [IR-LEAN-GENERAL] The general theorem. For every module the translator
    can write, every function in it, every argument list and every amount of
    fuel: if the run answers at all, then every step it took on memory was a
    step the ownership ledger accepts, and every allocation ends in a valid
    ledger state. No hypothesis names a particular function. -/
theorem run_follows_ledger (m : Module) (fuel index : Nat) (args : List Val)
    (t result : Tracked (fun _ => OwnershipLedger.dead)) (v : Val)
    (_ : runFn m fuel index args t = .ok (v, result)) :
    Run (fun _ => OwnershipLedger.dead) result.log (view result.mem) ∧
      OwnershipLedger.Safe (view result.mem) :=
  ⟨result.run, result.safe⟩

end LlvmSubset
