# osprey: flavor=python
# osprey: namespace=ir_lean
# osprey: module=Ops
"""One LLVM-dialect operation as a Lean `LlvmSubset.Instr` or `Term`
([IR-LEAN-TRANSLATE]).

The table follows `xdsl/dialects/llvm.py`: operand order, property names
(`elem_type`, `rawConstantIndices`, `predicate`, `callee`) and the `icmp`
predicate numbering are the dialect's. An operation, an extern or a constant
the Lean semantics has no meaning for is an `Error` naming it.
"""

from ir_lean import Types
from xdsl import Ir

__all__ = ["instr", "term", "is_term", "EXTERNS"]

EXTERNS = ["osp_alloc", "osp_alloc_tagged", "osp_alloc_tagged_noinit", "osp_ownership_free", "osprey_list_append_of", "osprey_list_length"]
"""The runtime entry points `LlvmSubset.Ext` models. The two lists change together."""

CASTS = ["bitcast", "ptrtoint", "inttoptr", "zext", "sext", "trunc"]
BINARY = ["add", "sub", "mul", "and", "or", "xor", "shl"]
PREDICATES = ["eq", "ne", "slt", "sle", "sgt", "sge", "ult", "ule", "ugt", "uge"]
RMW = ["xchg", "add", "sub"]
"""`llvm.atomicrmw`'s `bin_op` numbers `LlvmSubset.RmwOp` models, in order."""
DYNAMIC_INDEX = -2147483648


def nth(names, i) -> Result[str, str]:
    """Operand, result or successor `i` of an operation."""
    match listGet(names, i):
        case Success(value=name):
            return Success(value=name)
        case Error():
            return Error(message=f"operand {i} is missing")


def operand(scope, op, i):
    """The register number of operand `i`."""
    return andThen(nth(op.operands, i), lambda name: Types.reg(scope, name))


def result(scope, op):
    """The register number of the operation's one result."""
    return andThen(nth(op.results, 0), lambda name: Types.reg(scope, name))


def type_at(types, i):
    """Type `i` of a type list, as Lean."""
    match listGet(types, i):
        case Success(value=attr):
            return Types.ty(attr)
        case Error():
            return Error(message=f"type {i} is missing")


def lean_int(value):
    """An integer as a Lean term; a negative one is parenthesised."""
    return f"({value})" if value < 0 else toString(value)


def constant(scope: Types.Scope, op: Ir.Operation):
    """`llvm.mlir.constant` with an integer value."""
    match Ir.int_attr(op.properties, "value"):
        case Success(value=value):
            return andThen(result(scope, op), lambda d: Success(value=f".const {d} {lean_int(value)}"))
        case Error():
            return Error(message="a constant that is not an integer")


def scalar(attr):
    """Whether `attr` is a pointer or integer type."""
    match attr:
        case Ir.LLVMPointerType() | Ir.IntegerType():
            return True
        case _:
            return False


def zero(scope: Types.Scope, op: Ir.Operation):
    """`llvm.mlir.zero` of a pointer or an integer: null, or 0."""
    if scalar(unwrap_or(listGet(op.result_types, 0), Ir.UnitAttr())):
        return andThen(result(scope, op), lambda d: Success(value=f".const {d} 0"))
    else:
        return Error(message="a zero of an aggregate type")


def alloca(scope: Types.Scope, op: Ir.Operation):
    """`llvm.alloca` of one element of `elem_type`."""
    count = andThen(nth(op.operands, 0), lambda name: mapGet(scope.consts, name))
    if unwrap_or(count, 0) != 1:
        return Error(message="an alloca whose element count is not the constant 1")
    else:
        return andThen(result(scope, op), lambda d: andThen(andThen(Ir.get_attr(op.properties, "elem_type"), Types.ty), lambda elem: Success(value=f".alloca {d} {elem}")))


def store(scope: Types.Scope, op: Ir.Operation):
    """`llvm.store value, pointer`."""
    return andThen(type_at(op.operand_types, 0), lambda t: andThen(operand(scope, op, 0), lambda v: andThen(operand(scope, op, 1), lambda p: Success(value=f".store {t} {v} {p}"))))


def load(scope: Types.Scope, op: Ir.Operation):
    """`llvm.load pointer`."""
    return andThen(result(scope, op), lambda d: andThen(type_at(op.result_types, 0), lambda t: andThen(operand(scope, op, 0), lambda p: Success(value=f".load {d} {t} {p}"))))


def indices(scope, op, raw, i, dynamic, acc):
    """The `getelementptr` indices: a constant, or the next dynamic operand where the marker stands."""
    match listGet(raw, i):
        case Success(value=index):
            if index == DYNAMIC_INDEX:
                return andThen(operand(scope, op, dynamic), lambda r: indices(scope, op, raw, wrapAdd(i, 1), wrapAdd(dynamic, 1), listAppend(acc, f".reg {r}")))
            else:
                return indices(scope, op, raw, wrapAdd(i, 1), dynamic, listAppend(acc, f".const {lean_int(index)}"))
        case Error():
            return Success(value="[" + join(acc, ", ") + "]")


def gep(scope: Types.Scope, op: Ir.Operation):
    """`llvm.getelementptr`: base operand, `elem_type`, `rawConstantIndices`."""
    match Ir.dense_attr(op.properties, "rawConstantIndices"):
        case Success(value=raw):
            return andThen(result(scope, op), lambda d: andThen(andThen(Ir.get_attr(op.properties, "elem_type"), Types.ty), lambda elem: andThen(operand(scope, op, 0), lambda base: andThen(indices(scope, op, raw, 0, 1, []), lambda at: Success(value=f".gep {d} {elem} {base} {at}")))))
        case Error():
            return Error(message="a getelementptr without rawConstantIndices")


def cast(scope, op, kind):
    """A conversion: `kind source value target`."""
    return andThen(result(scope, op), lambda d: andThen(type_at(op.operand_types, 0), lambda source: andThen(operand(scope, op, 0), lambda v: andThen(type_at(op.result_types, 0), lambda target: Success(value=f".cast {d} .{kind} {source} {v} {target}")))))


def two(scope, op, head):
    """`head type left right` for a two-operand operation."""
    return andThen(result(scope, op), lambda d: andThen(type_at(op.operand_types, 0), lambda t: andThen(operand(scope, op, 0), lambda a: andThen(operand(scope, op, 1), lambda b: Success(value=f"{head(d)} {t} {a} {b}")))))


def icmp(scope: Types.Scope, op: Ir.Operation):
    """`llvm.icmp` with the dialect's predicate number."""
    match Ir.int_attr(op.properties, "predicate"):
        case Success(value=number):
            return andThen(nth(PREDICATES, number), lambda pred: two(scope, op, lambda d: f".icmp {d} .{pred}"))
        case Error():
            return Error(message="an icmp without a predicate")


def select(scope: Types.Scope, op: Ir.Operation):
    """`llvm.select condition, whenTrue, whenFalse`."""
    return andThen(result(scope, op), lambda d: andThen(operand(scope, op, 0), lambda c: andThen(operand(scope, op, 1), lambda a: andThen(operand(scope, op, 2), lambda b: Success(value=f".select {d} {c} {a} {b}")))))


def callee(scope, name):
    """A defined function by number, or a modelled runtime entry point by name."""
    match mapGet(scope.funcs, name):
        case Success(value=index):
            return Success(value=f"(.fn {index})")
        case Error():
            return Success(value=f"(.ext .{name})") if listContains(EXTERNS, name) else Error(message=f"extern @{name} has no model in LlvmSubset.Ext")


def call(scope: Types.Scope, op: Ir.Operation):
    """`llvm.call` of a named callee, or through the function pointer in operand 0."""
    dst = "none" if isEmpty(op.results) else "(some " + unwrap_or(result(scope, op), "0") + ")"
    match Ir.symbol_attr(op.properties, "callee"):
        case Success(value=name):
            return andThen(callee(scope, name), lambda target: andThen(Types.regs(scope, op.operands), lambda args: Success(value=f".call {dst} {target} {args}")))
        case Error():
            return andThen(operand(scope, op, 0), lambda target: andThen(Types.regs(scope, segment(op.operands, 1, wrapSub(len(op.operands), 1))), lambda args: Success(value=f".call {dst} (.reg {target}) {args}")))


def atomicrmw(scope: Types.Scope, op: Ir.Operation):
    """`llvm.atomicrmw pointer, operand` with the dialect's `bin_op` number ([IR-LEAN-ATOMIC])."""
    match andThen(Ir.int_attr(op.properties, "bin_op"), lambda number: nth(RMW, number)):
        case Success(value=kind):
            return andThen(result(scope, op), lambda d: andThen(type_at(op.operand_types, 1), lambda t: andThen(operand(scope, op, 0), lambda p: andThen(operand(scope, op, 1), lambda by: Success(value=f".atomicrmw {d} .{kind} {t} {p} {by}")))))
        case Error():
            return Error(message="an atomicrmw whose operation is not xchg, add or sub")


def named(scope, op, kind):
    """An operation whose meaning is one of the cast or binary tables."""
    if listContains(CASTS, kind):
        return cast(scope, op, kind)
    elif listContains(BINARY, kind):
        return two(scope, op, lambda d: f".bin {d} .{kind}")
    else:
        return Error(message=f"operation {op.name} is outside the subset")


def routed(scope, op, names, steps, otherwise):
    """The first `steps` row whose `names` row names `op.name`, else `otherwise`.

    The two lists run in parallel, one row per operation the subset defines.
    """
    return routedAt(scope, op, names, steps, otherwise, 0)


def routedAt(scope, op, names, steps, otherwise, i) -> Result[str, str]:
    """Row `i` of the dispatch table, or `otherwise` past its end."""
    match listGet(names, i):
        case Success(value=name):
            if op.name == name:
                return unwrap_or(listGet(steps, i), otherwise)(scope, op)
            else:
                return routedAt(scope, op, names, steps, otherwise, wrapAdd(i, 1))
        case Error():
            return otherwise(scope, op)


def memory(scope, op):
    """An operation that is not a constant, a call or a comparison."""
    return routed(scope, op, ["llvm.alloca", "llvm.store", "llvm.load", "llvm.getelementptr"], [alloca, store, load, gep], lambda scope, op: named(scope, op, drop(op.name, 5)))


def instr(scope, op) -> Result[str, str]:
    """One non-terminator operation as a Lean `Instr`."""
    return routed(scope, op, ["llvm.mlir.constant", "llvm.mlir.zero", "llvm.call", "llvm.icmp", "llvm.select", "llvm.atomicrmw"], [constant, zero, call, icmp, select, atomicrmw], memory)


def is_term(op):
    """Whether `op` ends a block."""
    return op.name == "llvm.return" or op.name == "llvm.br" or op.name == "llvm.cond_br" or op.name == "llvm.unreachable"


def segment(names, start, count):
    """`count` names from index `start`."""
    return toList(map(range(start, wrapAdd(start, count)), lambda i: unwrap_or(listGet(names, i), "")))


def cond_br(scope, op):
    """`llvm.cond_br`: the condition, then each successor's arguments, split by `operandSegmentSizes`."""
    match Ir.dense_attr(op.properties, "operandSegmentSizes"):
        case Success(value=sizes):
            taken = unwrap_or(listGet(sizes, 1), 0)
            return andThen(operand(scope, op, 0), lambda c: andThen(andThen(nth(op.successors, 0), lambda name: Types.block(scope, name)), lambda t: andThen(Types.regs(scope, segment(op.operands, 1, taken)), lambda ta: andThen(andThen(nth(op.successors, 1), lambda name: Types.block(scope, name)), lambda f: andThen(Types.regs(scope, segment(op.operands, wrapAdd(1, taken), unwrap_or(listGet(sizes, 2), 0))), lambda fa: Success(value=f".condBr {c} {t} {ta} {f} {fa}"))))))
        case Error():
            return Error(message="a cond_br without operandSegmentSizes")


def term(scope, op) -> Result[str, str]:
    """A terminator as a Lean `Term`."""
    if op.name == "llvm.return" and isEmpty(op.operands):
        return Success(value=".ret none")
    elif op.name == "llvm.return":
        return andThen(operand(scope, op, 0), lambda v: Success(value=f".ret (some {v})"))
    elif op.name == "llvm.br":
        return andThen(andThen(nth(op.successors, 0), lambda name: Types.block(scope, name)), lambda t: andThen(Types.regs(scope, op.operands), lambda args: Success(value=f".br {t} {args}")))
    elif op.name == "llvm.cond_br":
        return cond_br(scope, op)
    else:
        return Success(value=".unreachable")
