# osprey: flavor=python
# Ported from xDSL (Apache-2.0 WITH LLVM-exception); required attribution and terms: ../NOTICE.md.
# osprey: namespace=xdsl
# osprey: module=AttrParser
"""Attribute and type parsing.

The part of `xdsl/parser/attribute_parser.py` that MLIR's generic syntax
needs, plus the LLVM dialect's type syntax from `xdsl/dialects/llvm.py`. An
attribute or type this port has no record for is kept as `UnregisteredAttr`
holding its tokens, the way xDSL keeps an attribute of an unregistered
dialect, so reading never drops or guesses a value.
"""

from xdsl import Ir, Lexer

__all__ = ["parse_type", "parse_attr", "parse_dictionary", "parse_function_type", "items", "string_contents"]


def items(ts, i, close, item, acc):
    """`item (, item)*` up to the punctuation `close`, which is consumed. `i` is just past the opener.

    # Examples
    ```osprey
    from xdsl import AttrParser, Ir, Lexer

    def show():
        ts = unwrap_or(Lexer.tokens("i32, i64)"), [])
        parsed = unwrap_or(AttrParser.items(ts, 0, ")", AttrParser.parse_type, []), Ir.Parsed(value=[], next=0))
        print(f"{len(parsed.value)} {parsed.next}")

    wrappingArithmetic(show)
    ```
    ```output
    2 4
    ```
    """
    if Ir.is_punct(ts, i, close):
        return Ir.done(acc, wrapAdd(i, 1))
    else:
        return andThen(item(ts, i), lambda p: more_items(ts, p.next, close, item, listAppend(acc, p.value)))


def more_items(ts, i, close, item, acc):
    """The rest of a comma-separated list after one item."""
    if Ir.is_punct(ts, i, ","):
        return andThen(item(ts, wrapAdd(i, 1)), lambda p: more_items(ts, p.next, close, item, listAppend(acc, p.value)))
    elif Ir.is_punct(ts, i, close):
        return Ir.done(acc, wrapAdd(i, 1))
    else:
        return Ir.fail(ts, i, f"',' or '{close}'")


def hex_value(b):
    """The value of one hexadecimal digit, or -1."""
    if b >= 48 and b <= 57:
        return wrapSub(b, 48)
    elif b >= 65 and b <= 70:
        return wrapSub(b, 55)
    elif b >= 97 and b <= 102:
        return wrapSub(b, 87)
    else:
        return -1


def escape_at(text, i):
    """[decoded text, bytes consumed] for the escape whose backslash is at `i`; the lexer
    admits only these four and two hexadecimal digits."""
    b = Lexer.at(text, wrapAdd(i, 1))
    if b == 110:
        return ["\n", "2"]
    elif b == 116:
        return ["\t", "2"]
    elif b == 92 or b == 34:
        return [Lexer.cut(text, wrapAdd(i, 1), wrapAdd(i, 2)), "2"]
    else:
        return [unwrap_or(fromCodePoint(wrapAdd(wrapMul(hex_value(b), 16), hex_value(Lexer.at(text, wrapAdd(i, 2))))), ""), "3"]


def decode(text, i, end, acc):
    """The bytes `[i, end)` of a string literal's body with escapes decoded."""
    if i >= end:
        return acc
    elif Lexer.at(text, i) == 92:
        piece = escape_at(text, i)
        return decode(text, wrapAdd(i, unwrap_or(parseInt(unwrap_or(listGet(piece, 1), "2")), 2)), end, acc + unwrap_or(listGet(piece, 0), ""))
    else:
        return decode(text, wrapAdd(i, 1), end, acc + Lexer.cut(text, i, wrapAdd(i, 1)))


def string_contents(literal):
    """A string literal's value: quotes removed, `\\n`, `\\t`, `\\\\`, `\\"` and `\\HH` decoded.

    # Examples
    ```osprey
    from xdsl import AttrParser

    def show():
        print(AttrParser.string_contents('"hello"'))

    wrappingArithmetic(show)
    ```
    ```output
    hello
    ```
    """
    return decode(literal, 1, wrapSub(len(literal), 1), "")


def balanced_end(ts, i, depth):
    """The index after the `>` closing `depth` open `<` before `i`."""
    if depth == 0:
        return Success(value=i)
    elif Ir.peek(ts, i).kind == Lexer.EOF:
        return Ir.fail(ts, i, "'>'")
    elif Ir.is_punct(ts, i, "<"):
        return balanced_end(ts, wrapAdd(i, 1), wrapAdd(depth, 1))
    elif Ir.is_punct(ts, i, ">"):
        return balanced_end(ts, wrapAdd(i, 1), wrapSub(depth, 1))
    else:
        return balanced_end(ts, wrapAdd(i, 1), depth)


def text_of(ts, start, end):
    """The tokens `[start, end)` joined by spaces."""
    return join(toList(map(range(start, end), lambda k: Ir.peek(ts, k).text)), " ")


def unregistered(ts, start, i):
    """An attribute kept as text: tokens from `start`, with a `<...>` body if one opens at `i`."""
    if Ir.is_punct(ts, i, "<"):
        return andThen(balanced_end(ts, wrapAdd(i, 1), 1), lambda end: Ir.done(Ir.UnregisteredAttr(text=text_of(ts, start, end)), end))
    else:
        return Ir.done(Ir.UnregisteredAttr(text=text_of(ts, start, i)), i)


def all_digits(text, i):
    """Whether every byte of `text` from `i` on is a digit, and there is one."""
    if i >= len(text):
        return i > 1
    else:
        return Lexer.is_digit(Lexer.at(text, i)) and all_digits(text, wrapAdd(i, 1))


def int_width(text):
    """The width of the integer type spelled `text` (`i64` is 64), or -1."""
    if startsWith(text, "i") and all_digits(text, 1):
        return unwrap_or(parseInt(drop(text, 1)), -1)
    else:
        return -1


def llvm_struct(ts, i):
    """The body of `struct<...>` after its `<`: `(types)` or `"name", (types)`."""
    if Ir.peek(ts, i).kind == Lexer.STRING:
        name = string_contents(Ir.peek(ts, i).text)
        return andThen(Ir.expect(ts, wrapAdd(i, 1), ","), lambda j: struct_fields(ts, j, name))
    else:
        return struct_fields(ts, i, "")


def struct_fields(ts, i, name):
    """`(types) >` of a struct."""
    return andThen(Ir.expect(ts, i, "("), lambda j: andThen(items(ts, j, ")", llvm_type, []), lambda fields: andThen(Ir.expect(ts, fields.next, ">"), lambda end: Ir.done(Ir.LLVMStructType(struct_name=name, elements=fields.value), end))))


def llvm_array(ts, i):
    """The body of `array<N x type>` after its `<`."""
    size = unwrap_or(parseInt(Ir.peek(ts, i).text), -1)
    if Ir.peek(ts, i).kind == Lexer.INT and size >= 0 and Ir.is_word(ts, wrapAdd(i, 1), "x"):
        return andThen(llvm_type(ts, wrapAdd(i, 2)), lambda elt: andThen(Ir.expect(ts, elt.next, ">"), lambda end: Ir.done(Ir.LLVMArrayType(size=size, elt=elt.value), end)))
    else:
        return Ir.fail(ts, i, "an array size and 'x'")


def llvm_params(ts, i, acc):
    """Function parameter types up to `)`, with a trailing `...` marking a variadic function."""
    if Ir.is_punct(ts, i, ")"):
        return Ir.done(Ir.LLVMFunctionType(inputs=acc, output=Ir.LLVMVoidType(), variadic=False), wrapAdd(i, 1))
    elif Ir.is_punct(ts, i, "...") and Ir.is_punct(ts, wrapAdd(i, 1), ")"):
        return Ir.done(Ir.LLVMFunctionType(inputs=acc, output=Ir.LLVMVoidType(), variadic=True), wrapAdd(i, 2))
    else:
        return andThen(llvm_type(ts, i), lambda p: llvm_params(ts, wrapAdd(p.next, 1) if Ir.is_punct(ts, p.next, ",") else p.next, listAppend(acc, p.value)))


def llvm_func(ts, i):
    """The body of `func<result (params)>` after its `<`."""
    return andThen(llvm_type(ts, i), lambda out: andThen(Ir.expect(ts, out.next, "("), lambda j: andThen(llvm_params(ts, j, []), lambda params: andThen(Ir.expect(ts, params.next, ">"), lambda end: Ir.done(func_with(params.value, out.value), end)))))


def func_with(params, output):
    """`params` (a function type with a placeholder result) with its real result type."""
    match params:
        case Ir.LLVMFunctionType(inputs=inputs, variadic=variadic):
            return Ir.LLVMFunctionType(inputs=inputs, output=output, variadic=variadic)
        case _:
            return params


def llvm_named(ts, i, name):
    """The LLVM dialect type spelled `name` at token `i`, without its `!llvm.` prefix."""
    after = wrapAdd(i, 1)
    if name == "ptr":
        return andThen(unregistered(ts, i, after), lambda p: Ir.done(Ir.LLVMPointerType(), p.next))
    elif name == "void":
        return Ir.done(Ir.LLVMVoidType(), after)
    elif name == "struct" and Ir.is_punct(ts, after, "<"):
        return llvm_struct(ts, wrapAdd(i, 2))
    elif name == "array" and Ir.is_punct(ts, after, "<"):
        return llvm_array(ts, wrapAdd(i, 2))
    elif name == "func" and Ir.is_punct(ts, after, "<"):
        return llvm_func(ts, wrapAdd(i, 2))
    else:
        return Ir.fail(ts, i, "an LLVM dialect type")


def or_unregistered(ts, i, parsed):
    """`parsed`, or the same tokens kept as text when the structured reading failed."""
    match parsed:
        case Success(value=value):
            return Success(value=value)
        case Error():
            return unregistered(ts, i, wrapAdd(i, 1))


def llvm_type(ts, i):
    """A type inside an LLVM dialect type, where LLVM's own types drop the `!llvm.` prefix."""
    token = Ir.peek(ts, i)
    width = int_width(token.text)
    if token.kind == Lexer.BARE and width >= 0:
        return Ir.done(Ir.IntegerType(width=width), wrapAdd(i, 1))
    elif token.kind == Lexer.BARE:
        return or_unregistered(ts, i, llvm_named(ts, i, token.text))
    else:
        return parse_type(ts, i)


def type_list(ts, i):
    """`(type, ...)` starting at its `(`."""
    return andThen(Ir.expect(ts, i, "("), lambda j: items(ts, j, ")", parse_type, []))


def parse_function_type(ts, i):
    """`(inputs) -> output` or `(inputs) -> (outputs)`.

    # Examples
    ```osprey
    from xdsl import AttrParser, Ir, Lexer

    def show():
        ts = unwrap_or(Lexer.tokens("(i32, i64) -> i1"), [])
        match AttrParser.parse_function_type(ts, 0):
            case Success(value=p):
                print(f"{p.value} {p.next}")
            case Error(message=m):
                print(m)

    wrappingArithmetic(show)
    ```
    ```output
    FunctionType { inputs: [IntegerType { width: 32 }, IntegerType { width: 64 }], outputs: [IntegerType { width: 1 }] } 7
    ```
    """
    return andThen(type_list(ts, i), lambda inputs: andThen(Ir.expect(ts, inputs.next, "->"), lambda j: function_outputs(ts, j, inputs.value)))


def function_outputs(ts, i, inputs):
    """The result side of a function type."""
    if Ir.is_punct(ts, i, "("):
        return andThen(type_list(ts, i), lambda outputs: Ir.done(Ir.FunctionType(inputs=inputs, outputs=outputs.value), outputs.next))
    else:
        return andThen(parse_type(ts, i), lambda output: Ir.done(Ir.FunctionType(inputs=inputs, outputs=[output.value]), output.next))


def parse_type(ts, i):
    """One type: a builtin integer, an LLVM dialect type, a function type, or text for any other.

    # Examples
    ```osprey
    from xdsl import AttrParser, Ir, Lexer

    def show():
        ts = unwrap_or(Lexer.tokens("i32"), [])
        match AttrParser.parse_type(ts, 0):
            case Success(value=p):
                print(f"{p.value} {p.next}")
            case Error(message=m):
                print(m)

    wrappingArithmetic(show)
    ```
    ```output
    IntegerType { width: 32 } 1
    ```
    """
    token = Ir.peek(ts, i)
    width = int_width(token.text)
    if token.kind == Lexer.BARE and width >= 0:
        return Ir.done(Ir.IntegerType(width=width), wrapAdd(i, 1))
    elif token.kind == Lexer.BANG and startsWith(token.text, "!llvm."):
        return or_unregistered(ts, i, llvm_named(ts, i, drop(token.text, 6)))
    elif Ir.is_punct(ts, i, "("):
        return parse_function_type(ts, i)
    elif token.kind == Lexer.BARE or token.kind == Lexer.BANG:
        return unregistered(ts, i, wrapAdd(i, 1))
    else:
        return Ir.fail(ts, i, "a type")


HEX_DIGITS_MAX = 15
"""The most hexadecimal digits a literal may have and still fit a signed 64-bit value without wrapping."""


def hex_int(text, i, acc):
    """The value of the hexadecimal digits of `text` from `i` on, added to `acc`."""
    digit = hex_value(Lexer.at(text, i))
    if i >= len(text):
        return Success(value=acc)
    elif digit < 0:
        return Error(message="not a hexadecimal digit")
    else:
        return hex_int(text, wrapAdd(i, 1), wrapAdd(wrapMul(acc, 16), digit))


def int_literal(text):
    """A decimal literal, or a `0x` literal of at most `HEX_DIGITS_MAX` digits."""
    if startsWith(text, "0x") and len(text) > 2 and len(text) <= wrapAdd(HEX_DIGITS_MAX, 2):
        return hex_int(text, 2, 0)
    elif startsWith(text, "0x"):
        return Error(message="hexadecimal literal too long")
    else:
        return parseInt(text)


def integer_attr(ts, i, sign):
    """An integer literal at `i` with an optional `: type`; the default type is `i64`."""
    match int_literal(Ir.peek(ts, i).text):
        case Success(value=value):
            return typed_integer(ts, wrapAdd(i, 1), wrapMul(sign, value))
        case Error():
            return Ir.fail(ts, i, "an integer that fits 64 bits")


def typed_integer(ts, i, value):
    """`value` with the type after a `:` at `i`, or `i64`."""
    if Ir.is_punct(ts, i, ":"):
        return andThen(parse_type(ts, wrapAdd(i, 1)), lambda ty: Ir.done(Ir.IntegerAttr(value=value, type=ty.value), ty.next))
    else:
        return Ir.done(Ir.IntegerAttr(value=value, type=Ir.IntegerType(width=64)), i)


def skip_type_suffix(ts, i, value):
    """`value`, ending after an optional `: type` at `i`."""
    if Ir.is_punct(ts, i, ":"):
        return andThen(parse_type(ts, wrapAdd(i, 1)), lambda ty: Ir.done(value, ty.next))
    else:
        return Ir.done(value, i)


def dense_int(ts, i):
    """One possibly negative integer of a dense array."""
    if Ir.is_punct(ts, i, "-"):
        return andThen(dense_int(ts, wrapAdd(i, 1)), lambda p: Ir.done(wrapSub(0, p.value), p.next))
    else:
        match parseInt(Ir.peek(ts, i).text):
            case Success(value=value):
                return Ir.done(value, wrapAdd(i, 1))
            case Error():
                return Ir.fail(ts, i, "an integer")


def dense_array(ts, i):
    """`array<type>` or `array<type: n, ...>`, starting after its `<`."""
    return andThen(parse_type(ts, i), lambda ty: dense_values(ts, ty.next, ty.value))


def dense_values(ts, i, elt_type):
    """The values of a dense array after its element type."""
    if Ir.is_punct(ts, i, ":"):
        return andThen(items(ts, wrapAdd(i, 1), ">", dense_int, []), lambda values: Ir.done(Ir.DenseArrayBase(elt_type=elt_type, data=values.value), values.next))
    else:
        return andThen(Ir.expect(ts, i, ">"), lambda end: Ir.done(Ir.DenseArrayBase(elt_type=elt_type, data=[]), end))


def entry(ts, i):
    """One dictionary entry: `name = attribute`, or a bare `name` for a unit attribute."""
    token = Ir.peek(ts, i)
    name = string_contents(token.text) if token.kind == Lexer.STRING else token.text
    if token.kind != Lexer.STRING and token.kind != Lexer.BARE:
        return Ir.fail(ts, i, "an attribute name")
    elif Ir.is_punct(ts, wrapAdd(i, 1), "="):
        return andThen(parse_attr(ts, wrapAdd(i, 2)), lambda value: Ir.done(Ir.NamedAttr(name=name, value=value.value), value.next))
    else:
        return Ir.done(Ir.NamedAttr(name=name, value=Ir.UnitAttr()), wrapAdd(i, 1))


def parse_dictionary(ts, i):
    """`{ entry, ... }` starting at its `{`, as the list of its entries.

    # Examples
    ```osprey
    from xdsl import AttrParser, Ir, Lexer

    def show():
        ts = unwrap_or(Lexer.tokens('{width = 32 : i64, name = "x", sym = @main, dims = array<i64: 1, 2>}'), [])
        parsed = unwrap_or(AttrParser.parse_dictionary(ts, 0), Ir.Parsed(value=[], next=0))
        print(f"{len(parsed.value)} entries, next token {parsed.next}")

    wrappingArithmetic(show)
    ```
    ```output
    4 entries, next token 26
    ```
    """
    return andThen(Ir.expect(ts, i, "{"), lambda j: items(ts, j, "}", entry, []))


def dictionary_attr(entries):
    """A dictionary's entries as a `DictionaryAttr`."""
    return Ir.DictionaryAttr(names=mapList(entries, lambda e: e.name), values=mapList(entries, lambda e: e.value))


def symbol_name(text):
    """The name of an `@` identifier, unquoted when it was written as a string."""
    return string_contents(drop(text, 1)) if Lexer.at(text, 1) == 34 else drop(text, 1)


def parse_attr(ts, i):
    """One attribute value.

    # Examples
    ```osprey
    from xdsl import AttrParser, Ir, Lexer

    def show():
        ts = unwrap_or(Lexer.tokens("42 : i64"), [])
        match AttrParser.parse_attr(ts, 0):
            case Success(value=p):
                print(f"{p.value} {p.next}")
            case Error(message=m):
                print(m)

    wrappingArithmetic(show)
    ```
    ```output
    IntegerAttr { value: 42, type: IntegerType { width: 64 } } 3
    ```
    """
    token = Ir.peek(ts, i)
    after = wrapAdd(i, 1)
    if token.kind == Lexer.STRING:
        return skip_type_suffix(ts, after, Ir.StringAttr(data=string_contents(token.text)))
    elif token.kind == Lexer.INT:
        return integer_attr(ts, i, 1)
    elif Ir.is_punct(ts, i, "-") and Ir.peek(ts, after).kind == Lexer.INT:
        return integer_attr(ts, after, -1)
    elif token.kind == Lexer.AT:
        return Ir.done(Ir.SymbolRefAttr(name=symbol_name(token.text)), after)
    else:
        return compound_attr(ts, i, token, after)


def compound_attr(ts, i, token, after):
    """An attribute that is not a string, integer or symbol."""
    if Ir.is_punct(ts, i, "["):
        return andThen(items(ts, after, "]", parse_attr, []), lambda values: Ir.done(Ir.ArrayAttr(data=values.value), values.next))
    elif Ir.is_punct(ts, i, "{"):
        return andThen(parse_dictionary(ts, i), lambda entries: Ir.done(dictionary_attr(entries.value), entries.next))
    elif Ir.is_word(ts, i, "true") or Ir.is_word(ts, i, "false"):
        return Ir.done(Ir.IntegerAttr(value=1 if token.text == "true" else 0, type=Ir.IntegerType(width=1)), after)
    elif Ir.is_word(ts, i, "unit"):
        return Ir.done(Ir.UnitAttr(), after)
    elif Ir.is_word(ts, i, "array") and Ir.is_punct(ts, after, "<"):
        return dense_array(ts, wrapAdd(i, 2))
    else:
        return other_attr(ts, i, token, after)


def other_attr(ts, i, token, after):
    """A float, a dialect attribute or a type used as an attribute."""
    if token.kind == Lexer.HASH:
        return unregistered(ts, i, after)
    elif token.kind == Lexer.FLOAT or (Ir.is_punct(ts, i, "-") and Ir.peek(ts, after).kind == Lexer.FLOAT):
        end = after if token.kind == Lexer.FLOAT else wrapAdd(i, 2)
        return skip_type_suffix(ts, end, Ir.UnregisteredAttr(text=text_of(ts, i, end)))
    else:
        return andThen(parse_type(ts, i), lambda ty: skip_type_suffix(ts, ty.next, ty.value))
