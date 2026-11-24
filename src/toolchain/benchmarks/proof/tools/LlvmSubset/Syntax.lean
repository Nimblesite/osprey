/- [IR-LEAN-SYNTAX] The LLVM subset `osprey --memory=ownership` emits, as data.
   `@osprey/ir_lean` writes one `Module` per compiled program; nothing here is
   typed by hand. Registers, labels and functions are numbered by the
   translator in order of first definition, so lookups compare numbers. -/
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

inductive Operand where
  | reg (id : Nat)
  | const (v : Int)
  /-- The address of a module global (a string constant). -/
  | global (id : Nat)
  deriving Repr

inductive CastOp where
  | bitcast | ptrtoint | inttoptr | zext | sext | trunc
  deriving Repr

inductive BinOp where
  | add | sub | mul | and | or | xor | shl
  deriving Repr

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
  deriving Repr

inductive Instr where
  | alloca (dst : Nat) (ty : Ty)
  | store (ty : Ty) (value pointer : Operand)
  | load (dst : Nat) (ty : Ty) (pointer : Operand)
  | gep (dst : Nat) (ty : Ty) (base : Operand) (indices : List Operand)
  | cast (dst : Nat) (op : CastOp) (source : Ty) (value : Operand) (target : Ty)
  | bin (dst : Nat) (op : BinOp) (ty : Ty) (left right : Operand)
  | icmp (dst : Nat) (pred : Pred) (ty : Ty) (left right : Operand)
  | select (dst : Nat) (condition whenTrue whenFalse : Operand)
  | call (dst : Option Nat) (callee : Callee) (args : List Operand)
  deriving Repr

inductive Term where
  | ret (value : Option Operand)
  | br (target : Nat)
  | condBr (condition : Operand) (whenTrue whenFalse : Nat)
  | unreachable
  deriving Repr

/-- `dst = phi [value, predecessor] ...`; every phi of a block reads the
    registers as they were on the incoming edge. -/
structure Phi where
  dst : Nat
  incoming : List (Nat × Operand)
  deriving Repr

structure Block where
  phis : List Phi
  body : List Instr
  term : Term
  deriving Repr

/-- Block 0 is the entry block; `params` are the registers the arguments bind. -/
structure Func where
  params : List Nat
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
