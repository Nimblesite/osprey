# osprey: flavor=python
# Ported from xDSL (Apache-2.0 WITH LLVM-exception); required attribution and terms: ../NOTICE.md.
# osprey: namespace=xdsl
# osprey: module=Ir
"""The IR a parse produces: attributes, operations, blocks and regions.

The data half of `xdsl/ir/core.py` and `xdsl/dialects/builtin.py`, as closed
records. xDSL links an operand to the `SSAValue` object that defines it; here
an operand is the defining name (`%3`), and a successor is the block's name
(`^bb1`), so a parsed module is a plain tree. Types are attributes, as in
xDSL. The LLVM dialect's types come from `xdsl/dialects/llvm.py`.
"""

from xdsl import Lexer

__all__ = ["Attribute", "NamedAttr", "Operation", "Block", "Region", "Parsed", "peek", "is_punct", "is_word", "fail", "expect", "done", "get_attr", "int_attr", "string_attr", "symbol_attr", "dense_attr"]


@union
class Attribute:
    """A compile-time value attached to an operation. A type is an attribute."""
    class IntegerType:
        width: int
    class IntegerAttr:
        value: int
        type: Attribute
    class StringAttr:
        data: str
    class SymbolRefAttr:
        name: str
    class UnitAttr:
        pass
    class ArrayAttr:
        data: list[Attribute]
    class DenseArrayBase:
        elt_type: Attribute
        data: list[int]
    class DictionaryAttr:
        names: list[str]
        values: list[Attribute]
    class FunctionType:
        inputs: list[Attribute]
        outputs: list[Attribute]
    class LLVMPointerType:
        pass
    class LLVMStructType:
        struct_name: str
        elements: list[Attribute]
    class LLVMArrayType:
        size: int
        elt: Attribute
    class LLVMFunctionType:
        inputs: list[Attribute]
        output: Attribute
        variadic: bool
    class LLVMVoidType:
        pass
    class UnregisteredAttr:
        text: str


class NamedAttr:
    """One `name = value` entry of a property or attribute dictionary."""
    name: str
    value: Attribute


class Operation:
    """One operation in generic form. `results` and `operands` are value names
    with their `%`; `successors` are block names with their `^`."""
    name: str
    results: list[str]
    operands: list[str]
    successors: list[str]
    properties: list[NamedAttr]
    attributes: list[NamedAttr]
    regions: list[Region]
    operand_types: list[Attribute]
    result_types: list[Attribute]
    line: int


class Block:
    """A basic block: its name (empty for an unlabelled entry block), its
    arguments with their types, and its operations in order."""
    name: str
    args: list[str]
    arg_types: list[Attribute]
    ops: list[Operation]


class Region:
    """A list of blocks; the first is the entry block."""
    blocks: list[Block]


class Parsed[T]:
    """A parsed value and the index of the first token after it."""
    value: T
    next: int


def peek(ts, i):
    """Token `i`, or an end-of-file token past the end."""
    return unwrap_or(listGet(ts, i), Lexer.Token(kind=Lexer.EOF, text="", line=0))


def is_punct(ts, i, text):
    """Whether token `i` is the punctuation `text`."""
    return peek(ts, i).kind == Lexer.PUNCT and peek(ts, i).text == text


def is_word(ts, i, word):
    """Whether token `i` is the bare identifier `word`."""
    return peek(ts, i).kind == Lexer.BARE and peek(ts, i).text == word


def fail(ts, i, expected):
    """The parse error for token `i`: its line, what was expected and what was there."""
    return Error(message=f"line {peek(ts, i).line}: expected {expected}, found '{peek(ts, i).text}'")


def expect(ts, i, text) -> Result[int, str]:
    """The index after the punctuation `text` at `i`."""
    if is_punct(ts, i, text):
        return Success(value=wrapAdd(i, 1))
    else:
        return fail(ts, i, f"'{text}'")


def done(value, next):
    """A successful parse of `value` ending before token `next`."""
    return Success(value=Parsed(value=value, next=next))


def attr_at(entries, name, i):
    """The first entry named `name` from index `i` on."""
    match listGet(entries, i):
        case Success(value=entry):
            return Success(value=entry.value) if entry.name == name else attr_at(entries, name, wrapAdd(i, 1))
        case Error():
            return Error(message=f"no attribute '{name}'")


def get_attr(entries, name) -> Result[Attribute, str]:
    """The value of the entry named `name` in a property or attribute dictionary."""
    return attr_at(entries, name, 0)


def as_int(attr) -> Result[int, str]:
    """The value of an `IntegerAttr`."""
    match attr:
        case IntegerAttr(value=value):
            return Success(value=value)
        case _:
            return Error(message="not an integer attribute")


def as_string(attr) -> Result[str, str]:
    """The text of a `StringAttr`."""
    match attr:
        case StringAttr(data=data):
            return Success(value=data)
        case _:
            return Error(message="not a string attribute")


def as_symbol(attr) -> Result[str, str]:
    """The name a `SymbolRefAttr` refers to."""
    match attr:
        case SymbolRefAttr(name=name):
            return Success(value=name)
        case _:
            return Error(message="not a symbol reference")


def as_dense(attr) -> Result[list[int], str]:
    """The integers of a `DenseArrayBase`."""
    match attr:
        case DenseArrayBase(data=data):
            return Success(value=data)
        case _:
            return Error(message="not a dense array")


def int_attr(entries, name) -> Result[int, str]:
    """The integer entry `name` of a property or attribute dictionary."""
    return andThen(get_attr(entries, name), as_int)


def string_attr(entries, name) -> Result[str, str]:
    """The string entry `name` of a property or attribute dictionary."""
    return andThen(get_attr(entries, name), as_string)


def symbol_attr(entries, name) -> Result[str, str]:
    """The symbol entry `name` of a property or attribute dictionary."""
    return andThen(get_attr(entries, name), as_symbol)


def dense_attr(entries, name) -> Result[list[int], str]:
    """The dense integer array entry `name` of a property or attribute dictionary."""
    return andThen(get_attr(entries, name), as_dense)
