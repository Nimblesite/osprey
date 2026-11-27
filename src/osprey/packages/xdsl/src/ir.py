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
    """Token `i`, or an end-of-file token past the end.

    # Examples
    ```osprey
    from xdsl import Ir, Lexer

    def show():
        ts = unwrap_or(Lexer.tokens("x = 1"), [])
        print(f"{Ir.peek(ts, 0).text} {Ir.peek(ts, 1).kind} {Ir.peek(ts, 99).kind}")

    wrappingArithmetic(show)
    ```
    ```output
    x punctuation eof
    ```
    """
    return unwrap_or(listGet(ts, i), Lexer.Token(kind=Lexer.EOF, text="", line=0))


def is_punct(ts, i, text):
    """Whether token `i` is the punctuation `text`.

    # Examples
    ```osprey
    from xdsl import Ir, Lexer

    def show():
        ts = unwrap_or(Lexer.tokens("x = 1"), [])
        print(f"{Ir.is_punct(ts, 1, '=')} {Ir.is_punct(ts, 0, '=')}")

    wrappingArithmetic(show)
    ```
    ```output
    true false
    ```
    """
    return peek(ts, i).kind == Lexer.PUNCT and peek(ts, i).text == text


def is_word(ts, i, word):
    """Whether token `i` is the bare identifier `word`.

    # Examples
    ```osprey
    from xdsl import Ir, Lexer

    def show():
        ts = unwrap_or(Lexer.tokens("module x"), [])
        print(f"{Ir.is_word(ts, 0, 'module')} {Ir.is_word(ts, 1, 'module')}")

    wrappingArithmetic(show)
    ```
    ```output
    true false
    ```
    """
    return peek(ts, i).kind == Lexer.BARE and peek(ts, i).text == word


def fail(ts, i, expected):
    """The parse error for token `i`: its line, what was expected and what was there.

    # Examples
    ```osprey
    from xdsl import Ir, Lexer

    def show():
        ts = unwrap_or(Lexer.tokens("x = 1"), [])
        match Ir.fail(ts, 2, "a type"):
            case Error(message=m):
                print(m)
            case Success():
                print("parsed")

    wrappingArithmetic(show)
    ```
    ```output
    line 1: expected a type, found '1'
    ```
    """
    return Error(message=f"line {peek(ts, i).line}: expected {expected}, found '{peek(ts, i).text}'")


def expect(ts, i, text) -> Result[int, str]:
    """The index after the punctuation `text` at `i`.

    # Examples
    ```osprey
    from xdsl import Ir, Lexer

    def show():
        ts = unwrap_or(Lexer.tokens("x = 1"), [])
        print(f"{unwrap_or(Ir.expect(ts, 1, '='), -1)} {unwrap_or(Ir.expect(ts, 0, '='), -1)}")

    wrappingArithmetic(show)
    ```
    ```output
    2 -1
    ```
    """
    if is_punct(ts, i, text):
        return Success(value=wrapAdd(i, 1))
    else:
        return fail(ts, i, f"'{text}'")


def done(value, next):
    """A successful parse of `value` ending before token `next`.

    # Examples
    ```osprey
    from xdsl import Ir

    def show():
        match Ir.done("value", 3):
            case Success(value=p):
                print(f"{p.value} {p.next}")
            case Error():
                print("failed")

    wrappingArithmetic(show)
    ```
    ```output
    value 3
    ```
    """
    return Success(value=Parsed(value=value, next=next))


def attr_at(entries, name, i):
    """The first entry named `name` from index `i` on."""
    match listGet(entries, i):
        case Success(value=entry):
            return Success(value=entry.value) if entry.name == name else attr_at(entries, name, wrapAdd(i, 1))
        case Error():
            return Error(message=f"no attribute '{name}'")


def get_attr(entries, name) -> Result[Attribute, str]:
    """The value of the entry named `name` in a property or attribute dictionary.

    # Examples
    ```osprey
    from xdsl import AttrParser, Ir, Lexer

    def show():
        ts = unwrap_or(Lexer.tokens('{width = 32 : i64, name = "x", sym = @main, dims = array<i64: 1, 2>}'), [])
        entries = unwrap_or(AttrParser.parse_dictionary(ts, 0), Ir.Parsed(value=[], next=0)).value
        print(f"{Ir.get_attr(entries, 'width')} {Ir.get_attr(entries, 'missing')}")

    wrappingArithmetic(show)
    ```
    ```output
    Success(IntegerAttr { value: 32, type: IntegerType { width: 64 } }) Error(no attribute 'missing')
    ```
    """
    return attr_at(entries, name, 0)


def as_int(attr):
    """The value of an `IntegerAttr`."""
    match attr:
        case IntegerAttr(value=value):
            return Success(value=value)
        case _:
            return Error(message="not an integer attribute")


def as_string(attr):
    """The text of a `StringAttr`."""
    match attr:
        case StringAttr(data=data):
            return Success(value=data)
        case _:
            return Error(message="not a string attribute")


def as_symbol(attr):
    """The name a `SymbolRefAttr` refers to."""
    match attr:
        case SymbolRefAttr(name=name):
            return Success(value=name)
        case _:
            return Error(message="not a symbol reference")


def as_dense(attr):
    """The integers of a `DenseArrayBase`."""
    match attr:
        case DenseArrayBase(data=data):
            return Success(value=data)
        case _:
            return Error(message="not a dense array")


def int_attr(entries, name) -> Result[int, str]:
    """The integer entry `name` of a property or attribute dictionary.

    # Examples
    ```osprey
    from xdsl import Ir

    width = [Ir.NamedAttr(name='w', value=Ir.IntegerAttr(value=32, type=Ir.UnitAttr()))]
    print(f"{Ir.int_attr(width, 'w')} {Ir.int_attr(width, 'h')}")
    ```
    ```output
    Success(32) Error(no attribute 'h')
    ```
    """
    return andThen(get_attr(entries, name), as_int)


def string_attr(entries, name) -> Result[str, str]:
    """The string entry `name` of a property or attribute dictionary.

    # Examples
    ```osprey
    from xdsl import Ir

    print(f"{Ir.string_attr([Ir.NamedAttr(name='n', value=Ir.StringAttr(data='x'))], 'n')} {Ir.string_attr([Ir.NamedAttr(name='n', value=Ir.UnitAttr())], 'n')}")
    ```
    ```output
    Success(x) Error(not a string attribute)
    ```
    """
    return andThen(get_attr(entries, name), as_string)


def symbol_attr(entries, name) -> Result[str, str]:
    """The symbol entry `name` of a property or attribute dictionary.

    # Examples
    ```osprey
    from xdsl import Ir

    entries = [Ir.NamedAttr(name='sym', value=Ir.SymbolRefAttr(name='main'))]
    print(f"{Ir.symbol_attr(entries, 'sym')} {Ir.symbol_attr(entries, 'other')}")
    ```
    ```output
    Success(main) Error(no attribute 'other')
    ```
    """
    return andThen(get_attr(entries, name), as_symbol)


def dense_attr(entries, name) -> Result[list[int], str]:
    """The dense integer array entry `name` of a property or attribute dictionary.

    # Examples
    ```osprey
    from xdsl import Ir

    dims = Ir.DenseArrayBase(elt_type=Ir.IntegerType(width=64), data=[1, 2])
    print(Ir.dense_attr([Ir.NamedAttr(name='dims', value=dims)], 'dims'))
    ```
    ```output
    Success([1, 2])
    ```
    """
    return andThen(get_attr(entries, name), as_dense)
