/- [IR-LEAN-SYNTAX] The LLVM subset `osprey --memory=ownership` emits, as data, in
   the shape of MLIR's LLVM dialect: every operand is a value, a constant is an
   operation, and a branch passes arguments to its target block instead of the
   target holding phis. `mlir-translate --import-llvm` produces that shape from
   the compiler's IR and `@osprey/xdsl` reads it; nothing here is typed by hand.
   Values, blocks and functions are numbered in order of first definition. -/
namespace LlvmSubset

/-- First-class types, needed for sizes and `getelementptr` offsets. -/
inductive Ty where
  | int (bits : Nat)
  | ptr
  | struct (fields : List Ty)
  | array (count : Nat) (elem : Ty)
  deriving Repr

/-- A runtime value: a plain number, or an address inside one allocation.
    `ptrtoint`, `inttoptr` and `bitcast` keep the value, so a pointer boxed
    into an `i64` still names its allocation. `null` is `int 0`. -/
inductive Val where
  | int (v : Int)
  | ptr (block : Nat) (off : Nat)
  deriving DecidableEq, Repr

inductive CastOp where
  | bitcast | ptrtoint | inttoptr | zext | sext | trunc
  deriving Repr

inductive BinOp where
  | add | sub | mul | and | or | xor | shl
  deriving Repr

<<<<<<< HEAD
=======
/-- What an `atomicrmw` does to the word: the dialect's `bin_op` 0, 1 and 2. -/
inductive RmwOp where
  | xchg | add | sub
  deriving Repr

>>>>>>> origin/fixes
inductive Pred where
  | eq | ne | slt | sle | sgt | sge | ult | ule | ugt | uge
  deriving Repr

/-- The runtime entry points the model gives a meaning to. A call to anything
    else has no constructor, so its translation does not elaborate. -/
inductive Ext where
  | osp_alloc
  | osp_alloc_tagged
  | osp_alloc_tagged_noinit
  | osp_ownership_free
  | osprey_list_append_of
  | osprey_list_length
  deriving DecidableEq, Repr

inductive Callee where
  | fn (index : Nat)
  | ext (name : Ext)
<<<<<<< HEAD
=======
  /-- A call through a function pointer. A function's address is its number
      in the module, so the register holds that number. -/
  | reg (id : Nat)
>>>>>>> origin/fixes
  deriving Repr

/-- One index of a `getelementptr`: a constant, or a value. -/
inductive Index where
  | const (v : Int)
  | reg (id : Nat)
  deriving Repr

inductive Instr where
  /-- `llvm.mlir.constant` and `llvm.mlir.zero`. -/
  | const (dst : Nat) (v : Int)
  | alloca (dst : Nat) (ty : Ty)
  | store (ty : Ty) (value pointer : Nat)
  | load (dst : Nat) (ty : Ty) (pointer : Nat)
  | gep (dst : Nat) (ty : Ty) (base : Nat) (indices : List Index)
  | cast (dst : Nat) (op : CastOp) (source : Ty) (value : Nat) (target : Ty)
  | bin (dst : Nat) (op : BinOp) (ty : Ty) (left right : Nat)
  | icmp (dst : Nat) (pred : Pred) (ty : Ty) (left right : Nat)
  | select (dst : Nat) (condition whenTrue whenFalse : Nat)
<<<<<<< HEAD
=======
  /-- `atomicrmw`: one indivisible read-modify-write of the word at `pointer`
      ([IR-LEAN-ATOMIC]). `dst` is the word as it was. -/
  | atomicrmw (dst : Nat) (op : RmwOp) (ty : Ty) (pointer operand : Nat)
>>>>>>> origin/fixes
  | call (dst : Option Nat) (callee : Callee) (args : List Nat)
  deriving Repr

inductive Term where
  | ret (value : Option Nat)
  | br (target : Nat) (args : List Nat)
  | condBr (condition : Nat) (whenTrue : Nat) (trueArgs : List Nat)
      (whenFalse : Nat) (falseArgs : List Nat)
  | unreachable
  deriving Repr

/-- A basic block. A branch to it binds `params` to the arguments it passes,
    all read before any is bound. -/
structure Block where
  params : List Nat
  body : List Instr
  term : Term
  deriving Repr

/-- Block 0 is the entry block; its `params` bind the function's arguments. -/
structure Func where
  blocks : List Block
  deriving Repr

structure Module where
  funcs : List Func
  deriving Repr

/-- Why an execution stopped. Every memory-safety violation is a fault, so a
    run that answers a value committed none. -/
inductive Fault where
  | fuel | undefinedRegister | badPointer | useAfterFree | doubleFree
  | outOfBounds | uninitialised | badPhi | noBlock | noFunction | unreachable
  | unsupported | badArguments | freeOfStack | freeInside | movedFrom
  deriving DecidableEq, Repr

mutual
  /-- Alignment in bytes on a 64-bit target. -/
  def Ty.align : Ty → Nat
    | .int bits => if bits ≤ 8 then 1 else if bits ≤ 16 then 2 else if bits ≤ 32 then 4 else 8
    | .ptr => 8
    | .struct fields => Ty.alignAll fields
    | .array _ elem => elem.align
  def Ty.alignAll : List Ty → Nat
    | [] => 1
    | field :: rest => max field.align (Ty.alignAll rest)
end

/-- `offset` rounded up to a multiple of `align`. -/
def roundUp (offset align : Nat) : Nat := (offset + align - 1) / align * align

mutual
  /-- Allocation size in bytes, padded as clang lays a struct out. -/
  def Ty.size : Ty → Nat
    | .int bits => if bits ≤ 8 then 1 else if bits ≤ 16 then 2 else if bits ≤ 32 then 4 else 8
    | .ptr => 8
    | .struct fields => roundUp (Ty.endOf fields 0) (Ty.alignAll fields)
    | .array count elem => count * elem.size
  /-- The offset just past the last of `fields`, laid out from `offset`. -/
  def Ty.endOf : List Ty → Nat → Nat
    | [], offset => offset
    | field :: rest, offset => Ty.endOf rest (roundUp offset field.align + field.size)
end

/-- Byte offset of field `index` of a struct laid out from `offset`. -/
def fieldOffset : List Ty → Nat → Nat → Option (Nat × Ty)
  | [], _, _ => none
  | field :: _, 0, offset => some (roundUp offset field.align, field)
  | field :: rest, index + 1, offset =>
    fieldOffset rest index (roundUp offset field.align + field.size)

end LlvmSubset
