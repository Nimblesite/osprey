# osprey: flavor=python
# osprey: namespace=ir_lean
# osprey: module=Types
"""Names and types as Lean text ([IR-LEAN-TRANSLATE]).

A function's values and blocks are numbered in order of first definition; the
Lean semantics looks registers up by number. A type or a name the subset has
no form for is an `Error` naming it, never a guess.
"""

from xdsl import Ir

__all__ = ["Scope", "ty", "reg", "regs", "block", "each", "collect"]


class Scope:
    """What one function's operations can name: its values, the values that
    are integer constants, its blocks, and the module's defined functions."""
    values: dict[str, int]
    consts: dict[str, int]
    blocks: dict[str, int]
    funcs: dict[str, int]


def collect(items, convert, i, acc):
    """`convert` applied to every item from index `i`, stopping at the first failure."""
    match listGet(items, i):
        case Success(value=item):
            return andThen(convert(item), lambda text: collect(items, convert, wrapAdd(i, 1), listAppend(acc, text)))
        case Error():
            return Success(value=acc)


def each(items, convert) -> Result[str, str]:
    """`[a, b, ...]`: every item converted, as a Lean list."""
    return andThen(collect(items, convert, 0, []), lambda texts: Success(value="[" + join(texts, ", ") + "]"))


def ty(attr) -> Result[str, str]:
    """A type as a Lean `LlvmSubset.Ty` term."""
    match attr:
        case Ir.IntegerType(width=width):
            return Success(value=f"(.int {width})")
        case Ir.LLVMPointerType():
            return Success(value=".ptr")
        case Ir.LLVMStructType(elements=elements):
            return andThen(each(elements, ty), lambda fields: Success(value=f"(.struct {fields})"))
        case Ir.LLVMArrayType(size=size, elt=elt):
            return andThen(ty(elt), lambda inner: Success(value=f"(.array {size} {inner})"))
        case Ir.UnregisteredAttr(text=text):
            return Error(message=f"type {text} is outside the subset")
        case _:
            return Error(message="an attribute that is not a first-class type")


def numbered(table, name, kind):
    """The number `table` gives `name`, or why it does not."""
    match mapGet(table, name):
        case Success(value=number):
            return Success(value=toString(number))
        case Error():
            return Error(message=f"{kind} {name} is not defined in this function")


def reg(scope, name) -> Result[str, str]:
    """The number of the value `name`."""
    return numbered(scope.values, name, "value")


def regs(scope, names) -> Result[str, str]:
    """`[n, ...]`: the numbers of the values `names`."""
    return each(names, lambda name: reg(scope, name))


def block(scope, name) -> Result[str, str]:
    """The number of the block `name`."""
    return numbered(scope.blocks, name, "block")
