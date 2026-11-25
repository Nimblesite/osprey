# osprey: flavor=python
# osprey: namespace=ir_lean
# osprey: module=Emit
"""A parsed module as a Lean `LlvmSubset.Module` ([IR-LEAN-TRANSLATE]).

Every defined function keeps its position in the module, so a call's
`(.fn n)` means the same function whether or not that function could be
translated. A function with an operation outside the subset becomes the empty
function `⟨[]⟩`, which faults when run, with the reason beside it.
"""

from ir_lean import Ops, Types
from xdsl import Ir

__all__ = ["lean_module", "Fn"]


class Fn:
    """One defined function: its symbol, its operation and its position."""
    name: str
    op: Ir.Operation
    index: int


def sym_name(op):
    """The `sym_name` of a function or global."""
    return unwrap_or(Ir.string_attr(op.properties, "sym_name"), "")


def body_blocks(op):
    """The blocks of an operation's first region."""
    match listGet(op.regions, 0):
        case Success(value=region):
            return region.blocks
        case Error():
            return []


def defined(op):
    """Whether `op` is an `llvm.func` with a body."""
    return op.name == "llvm.func" and not isEmpty(body_blocks(op))


def functions(module):
    """Every defined function of the module, numbered in order."""
    ops = foldList(body_blocks(module), [], lambda acc, blk: listConcat(acc, filterList(blk.ops, defined)))
    return toList(map(range(0, len(ops)), lambda i: fn_at(ops, i)))


def fn_at(ops, i):
    """Function `i` of the defined functions."""
    match listGet(ops, i):
        case Success(value=op):
            return [Fn(name=sym_name(op), op=op, index=i)]
        case Error():
            return []


def numbered(names, table):
    """`table` with each of `names` given the next free number."""
    return foldList(names, table, lambda acc, name: mapSet(acc, name, len(acc)))


def const_of(op, consts):
    """`consts` with the operation's result, when it is an integer constant."""
    match Ir.int_attr(op.properties, "value"):
        case Success(value=value):
            return mapSet(consts, unwrap_or(listGet(op.results, 0), ""), value) if op.name == "llvm.mlir.constant" else consts
        case Error():
            return consts


def scope_block(scope, blk):
    """`scope` after block `blk`: the block, its arguments and its operations' results."""
    return Types.Scope(values=foldList(blk.ops, numbered(blk.args, scope.values), lambda acc, op: numbered(op.results, acc)), consts=foldList(blk.ops, scope.consts, lambda acc, op: const_of(op, acc)), blocks=mapSet(scope.blocks, blk.name, len(scope.blocks)), funcs=scope.funcs)


def scope_of(op, funcs):
    """Everything the function `op` can name."""
    return foldList(body_blocks(op), Types.Scope(values={}, consts={}, blocks={}, funcs=funcs), scope_block)


def located(op, translated) -> Result[str, str]:
    """`translated`, with the operation's line added to a failure."""
    match translated:
        case Success(value=text):
            return Success(value=text)
        case Error(message=message):
            return Error(message=f"line {op.line}: {message}")


def instr_text(scope, op) -> Result[str, str]:
    """One operation of a block's body; a terminator there is a failure."""
    if Ops.is_term(op):
        return Error(message=f"line {op.line}: terminator {op.name} is not the last operation of its block")
    else:
        return located(op, Ops.instr(scope, op))


def end_text(scope, op) -> Result[str, str]:
    """A block's last operation, which must be a terminator."""
    if Ops.is_term(op):
        return located(op, Ops.term(scope, op))
    else:
        return Error(message=f"line {op.line}: a block ends in {op.name}, which is not a terminator")


def body_text(scope, ops, i, last, acc) -> Result[str, str]:
    """The operations `[i, last)` as a Lean list of `Instr`."""
    match listGet(ops, i):
        case Success(value=op):
            if i >= last:
                return Success(value="[" + join(acc, ",\n    ") + "]")
            else:
                return andThen(instr_text(scope, op), lambda text: body_text(scope, ops, wrapAdd(i, 1), last, listAppend(acc, text)))
        case Error():
            return Success(value="[" + join(acc, ",\n    ") + "]")


def block_text(scope, blk) -> Result[str, str]:
    """One block as a Lean `Block`: parameters, body, terminator."""
    last = wrapSub(len(blk.ops), 1)
    match listGet(blk.ops, last):
        case Success(value=end):
            return andThen(Types.regs(scope, blk.args), lambda params: andThen(body_text(scope, blk.ops, 0, last, []), lambda instrs: andThen(end_text(scope, end), lambda term: Success(value=f"⟨{params}, {instrs},\n    {term}⟩"))))
        case Error():
            return Error(message=f"block {blk.name} is empty")


def function_text(fn, funcs) -> Result[str, str]:
    """One function as a Lean `Func`."""
    scope = scope_of(fn.op, funcs)
    return andThen(Types.collect(body_blocks(fn.op), lambda blk: block_text(scope, blk), 0, []), lambda blocks: Success(value="⟨[\n  " + join(blocks, ",\n  ") + "]⟩"))


def selected(fn, wanted):
    """Whether `fn` is one the caller asked for; an empty request asks for all."""
    return isEmpty(wanted) or foldList(wanted, False, lambda hit, prefix: hit or startsWith(fn.name, prefix))


def definition(fn, funcs, wanted):
    """The Lean definition of `fn`: its body, or the empty function with the reason."""
    if not selected(fn, wanted):
        return f"-- @{fn.name}: not requested\ndef fn{fn.index} : Func := ⟨[]⟩"
    else:
        match function_text(fn, funcs):
            case Success(value=text):
                return f"/-- `@{fn.name}` -/\ndef fn{fn.index} : Func := {text}"
            case Error(message=message):
                return f"-- @{fn.name}: REFUSED: {message}\ndef fn{fn.index} : Func := ⟨[]⟩"


def spelled(attr):
    """A parameter type as it reads in a signature name."""
    match attr:
        case Ir.IntegerType(width=width):
            return f"i{width}"
        case Ir.LLVMPointerType():
            return "ptr"
        case _:
            return "other"


def signature(fn):
    """`name(types)`: the source name without its `$mono` suffix, and the parameter types."""
    base = unwrap_or(listGet(unwrap_or(split(fn.name, "$mono"), [fn.name]), 0), fn.name)
    match listGet(body_blocks(fn.op), 0):
        case Success(value=entry):
            return base + "(" + join(mapList(entry.arg_types, spelled), ",") + ")"
        case Error():
            return base + "()"


def index_names(fns):
    """`def «name» : Nat`, for each symbol and for each signature only one function has."""
    exact = mapList(fns, lambda fn: f"def «{fn.name}» : Nat := {fn.index}")
    unique = filterList(fns, lambda fn: len(filterList(fns, lambda other: signature(other) == signature(fn))) == 1 and signature(fn) != fn.name)
    return join(listConcat(exact, mapList(unique, lambda fn: f"def «{signature(fn)}» : Nat := {fn.index}")), "\n")


def translated(text):
    """Whether a definition holds a translated body."""
    return startsWith(text, "/--")


def lean_module(module, namespace, wanted):
    """The whole module as Lean source in `namespace`."""
    fns = foldList(functions(module), [], listConcat)
    funcs = foldList(fns, {}, lambda acc, fn: mapSet(acc, fn.name, fn.index))
    defs = mapList(fns, lambda fn: definition(fn, funcs, wanted))
    listed = join(mapList(fns, lambda fn: f"fn{fn.index}"), ", ")
    return f"import LlvmSubset\n/-! Generated by ir_lean. Translated {len(filterList(defs, translated))} of {len(fns)} defined functions. -/\nnamespace {namespace}\nopen LlvmSubset\n\n" + join(defs, "\n\n") + f"\n\ndef module : Module := ⟨[{listed}]⟩\n\n" + index_names(fns) + f"\n\nend {namespace}\n"
