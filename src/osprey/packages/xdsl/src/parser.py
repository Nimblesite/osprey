# osprey: flavor=python
# Ported from xDSL (Apache-2.0 WITH LLVM-exception); required attribution and terms: ../NOTICE.md.
# osprey: namespace=xdsl
# osprey: module=Parser
"""The generic-syntax parser: MLIR text to a module.

A port of the generic half of `xdsl/parser/core.py` (`Parser.parse_module`,
`_parse_generic_operation`, `parse_region`, `_parse_block`), following the
same grammar:

    operation   ::= op-result-list? string-literal `(` value-use-list? `)`
                    successor-list? properties? region-list?
                    dictionary-attribute? `:` function-type location?
    region      ::= `{` entry-block? block* `}`
    block       ::= caret-id block-arg-list? `:` operation*

Only the generic form is read; the custom assembly of each dialect is not.
`mlir-opt --mlir-print-op-generic` prints any module in this form.
"""

from xdsl import AttrParser, Ir, Lexer

__all__ = ["parse_module", "parse_operation"]


def result_name(ts, i):
    """``op-result ::= value-id (`:` integer-literal)?``."""
    if Ir.peek(ts, i).kind != Lexer.PERCENT:
        return Ir.fail(ts, i, "a result name")
    elif Ir.is_punct(ts, wrapAdd(i, 1), ":") and Ir.peek(ts, wrapAdd(i, 2)).kind == Lexer.INT:
        return Ir.fail(ts, i, "a single result (packed results are not supported)")
    else:
        return Ir.done(Ir.peek(ts, i).text, wrapAdd(i, 1))


def result_list(ts, i):
    """``op-result-list ::= op-result (`,` op-result)* `=```, or no results."""
    if Ir.peek(ts, i).kind == Lexer.PERCENT:
        return AttrParser.items(ts, i, "=", result_name, [])
    else:
        return Ir.done([], i)


def value_use(ts, i):
    """One operand: a value name."""
    if Ir.peek(ts, i).kind == Lexer.PERCENT:
        return Ir.done(Ir.peek(ts, i).text, wrapAdd(i, 1))
    else:
        return Ir.fail(ts, i, "an operand")


def successor(ts, i):
    """One successor: a block name."""
    if Ir.peek(ts, i).kind == Lexer.CARET:
        return Ir.done(Ir.peek(ts, i).text, wrapAdd(i, 1))
    else:
        return Ir.fail(ts, i, "a block name")


def successors(ts, i):
    """``successor-list ::= `[` successor (`,` successor)* `]```, or none."""
    if Ir.is_punct(ts, i, "["):
        return AttrParser.items(ts, wrapAdd(i, 1), "]", successor, [])
    else:
        return Ir.done([], i)


def properties(ts, i):
    """``properties ::= `<` dictionary-attribute `>```, or none."""
    if Ir.is_punct(ts, i, "<"):
        return andThen(AttrParser.parse_dictionary(ts, wrapAdd(i, 1)), lambda entries: andThen(Ir.expect(ts, entries.next, ">"), lambda end: Ir.done(entries.value, end)))
    else:
        return Ir.done([], i)


def region_list(ts, i):
    """``region-list ::= `(` region (`,` region)* `)```, or none."""
    if Ir.is_punct(ts, i, "("):
        return AttrParser.items(ts, wrapAdd(i, 1), ")", parse_region, [])
    else:
        return Ir.done([], i)


def attributes(ts, i):
    """An optional attribute dictionary."""
    if Ir.is_punct(ts, i, "{"):
        return AttrParser.parse_dictionary(ts, i)
    else:
        return Ir.done([], i)


def skip_location(ts, i):
    """The index after an optional trailing `loc(...)`."""
    if Ir.is_word(ts, i, "loc") and Ir.is_punct(ts, wrapAdd(i, 1), "("):
        return close_paren(ts, wrapAdd(i, 2), 1)
    else:
        return Success(value=i)


def close_paren(ts, i, depth) -> Result[int, str]:
    """The index after the `)` closing `depth` open parentheses."""
    if depth == 0:
        return Success(value=i)
    elif Ir.peek(ts, i).kind == Lexer.EOF:
        return Ir.fail(ts, i, "')'")
    elif Ir.is_punct(ts, i, "("):
        return close_paren(ts, wrapAdd(i, 1), wrapAdd(depth, 1))
    elif Ir.is_punct(ts, i, ")"):
        return close_paren(ts, wrapAdd(i, 1), wrapSub(depth, 1))
    else:
        return close_paren(ts, wrapAdd(i, 1), depth)


def signature(ts, i, op):
    """The `: function-type` of an operation, checked against its operands and results."""
    return andThen(Ir.expect(ts, i, ":"), lambda j: andThen(AttrParser.parse_function_type(ts, j), lambda ty: andThen(skip_location(ts, ty.next), lambda end: typed(ts, i, op, ty.value, end))))


def typed(ts, i, op, ty, end):
    """`op` with the operand and result types of its function type."""
    match ty:
        case Ir.FunctionType(inputs=inputs, outputs=outputs):
            if len(inputs) != len(op.operands):
                return Ir.fail(ts, i, f"{len(op.operands)} operand types for '{op.name}'")
            elif len(outputs) != len(op.results):
                return Ir.fail(ts, i, f"{len(op.results)} result types for '{op.name}'")
            else:
                return Ir.done(update(op, operand_types=inputs, result_types=outputs), end)
        case _:
            return Ir.fail(ts, i, "a function type")


def parse_operation(ts, i):
    """One operation in generic form, starting at token `i`."""
    return andThen(result_list(ts, i), lambda results: generic_operation(ts, results.next, results.value))


def generic_operation(ts, i, results):
    """The operation after its result list: name, operands, then the optional parts."""
    name = Ir.peek(ts, i)
    if name.kind != Lexer.STRING:
        return Ir.fail(ts, i, "an operation name in generic form")
    else:
        return andThen(Ir.expect(ts, wrapAdd(i, 1), "("), lambda j: andThen(AttrParser.items(ts, j, ")", value_use, []), lambda operands: operation_tail(ts, operands.next, Ir.Operation(name=AttrParser.string_contents(name.text), results=results, operands=operands.value, successors=[], properties=[], attributes=[], regions=[], operand_types=[], result_types=[], line=name.line))))


def operation_tail(ts, i, op):
    """Successors, properties, regions and attributes, then the signature."""
    return andThen(successors(ts, i), lambda succ: andThen(properties(ts, succ.next), lambda props: andThen(region_list(ts, props.next), lambda regions: andThen(attributes(ts, regions.next), lambda attrs: signature(ts, attrs.next, update(op, successors=succ.value, properties=props.value, regions=regions.value, attributes=attrs.value))))))


def block_body(ts, i, ops):
    """Operations up to the next block label or the region's `}`."""
    if Ir.peek(ts, i).kind == Lexer.CARET or Ir.is_punct(ts, i, "}"):
        return Ir.done(ops, i)
    else:
        return andThen(parse_operation(ts, i), lambda op: block_body(ts, op.next, listAppend(ops, op.value)))


def block_arg(ts, i):
    """One `%name : type` block argument."""
    if Ir.peek(ts, i).kind != Lexer.PERCENT:
        return Ir.fail(ts, i, "a block argument")
    else:
        return andThen(Ir.expect(ts, wrapAdd(i, 1), ":"), lambda j: andThen(AttrParser.parse_type(ts, j), lambda ty: Ir.done(Ir.NamedAttr(name=Ir.peek(ts, i).text, value=ty.value), ty.next)))


def block_args(ts, i):
    """``block-arg-list ::= `(` ssa-id-and-type-list? `)```, or none."""
    if Ir.is_punct(ts, i, "("):
        return AttrParser.items(ts, wrapAdd(i, 1), ")", block_arg, [])
    else:
        return Ir.done([], i)


def labelled_block(ts, i):
    """``block ::= caret-id block-arg-list? `:` operation*``."""
    name = Ir.peek(ts, i).text
    return andThen(block_args(ts, wrapAdd(i, 1)), lambda args: andThen(Ir.expect(ts, args.next, ":"), lambda j: andThen(block_body(ts, j, []), lambda ops: Ir.done(Ir.Block(name=name, args=mapList(args.value, lambda a: a.name), arg_types=mapList(args.value, lambda a: a.value), ops=ops.value), ops.next))))


def blocks(ts, i, acc):
    """Labelled blocks up to the region's `}`, which is consumed."""
    if Ir.is_punct(ts, i, "}"):
        return Ir.done(Ir.Region(blocks=acc), wrapAdd(i, 1))
    elif Ir.peek(ts, i).kind == Lexer.CARET:
        return andThen(labelled_block(ts, i), lambda block: blocks(ts, block.next, listAppend(acc, block.value)))
    else:
        return Ir.fail(ts, i, "a block label or '}'")


def parse_region(ts, i):
    """``region ::= `{` entry-block? block* `}```. An unlabelled entry block has an empty name."""
    return andThen(Ir.expect(ts, i, "{"), lambda j: region_body(ts, j))


def region_body(ts, i):
    """A region after its `{`."""
    if Ir.peek(ts, i).kind == Lexer.CARET or Ir.is_punct(ts, i, "}"):
        return blocks(ts, i, [])
    else:
        return andThen(block_body(ts, i, []), lambda ops: blocks(ts, ops.next, [Ir.Block(name="", args=[], arg_types=[], ops=ops.value)]))


def whole(ts, parsed):
    """`parsed` when nothing but the end of input follows it."""
    if Ir.peek(ts, parsed.next).kind == Lexer.EOF:
        return Success(value=parsed.value)
    else:
        return Ir.fail(ts, parsed.next, "the end of the module")


def parse_module(source) -> Result[Ir.Operation, str]:
    """The single top-level operation of `source`, normally `builtin.module`."""
    return andThen(Lexer.tokens(source), lambda ts: andThen(parse_operation(ts, 0), lambda parsed: whole(ts, parsed)))
