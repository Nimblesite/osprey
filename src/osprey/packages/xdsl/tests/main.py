# osprey: flavor=python
"""Loaded reader use cases: real `mlir-opt --mlir-print-op-generic` shapes of the
compiler's own LLVM IR, every attribute and type form, and every error path, each
compared with what the real xDSL reader makes of the same input
(src/osprey/packages/xdsl/oracle/xdsl.cpython writes tests/expected.py).
Implements [IR-LEAN-READER] and [PORT-TEST-ORACLE]."""
from xdsl import AttrParser, Ir, Lexer, Parser
from xdsl_expected import Expected


# Where the reader deliberately differs from xDSL at efd7a0e7 (README "Differences from
# xDSL"): a type or attribute it does not model stays raw text, so an undefined `#loc`
# alias and an `ix` or malformed `!llvm` type are kept where xDSL refuses them; MLIR's
# typed string `"…" : i8` is read, which xDSL refuses; and a hex `f64` bit pattern is
# refused, where xDSL reads a float.
TYPE_DIFFERENCES = ["!llvm.array<x i8>", "!llvm.struct<(i64>", "ix"]
ATTR_DIFFERENCES = ["0x7FF0000000000000 : f64", "\"a\\nb\\t\\\\\\\"\\41\" : i8", "#loc"]


def shows(xs, kept):
    return join(mapList(xs, lambda x: show(x, kept)), ", ")


def show(attr, kept):
    """`attr` in the oracle's vocabulary; `kept` renders the text of a form the port keeps raw."""
    match attr:
        case Ir.IntegerType(width=width):
            return f"i{width}"
        case Ir.IntegerAttr(value=value, type=ty):
            return f"{value}:{show(ty, kept)}"
        case Ir.StringAttr(data=data):
            return f"str({data})"
        case Ir.SymbolRefAttr(name=name):
            return f"@{name}"
        case Ir.UnitAttr():
            return "unit"
        case Ir.ArrayAttr(data=data):
            return f"[{shows(data, kept)}]"
        case Ir.DenseArrayBase(elt_type=ty, data=data):
            return f"dense<{show(ty, kept)}: {join(mapList(data, toString), ' ')}>"
        case Ir.DictionaryAttr(names=names, values=values):
            return f"dict({join(names, ' ')} = {shows(values, kept)})"
        case Ir.FunctionType(inputs=inputs, outputs=outputs):
            return f"({shows(inputs, kept)}) -> ({shows(outputs, kept)})"
        case Ir.LLVMPointerType():
            return "ptr"
        case Ir.LLVMStructType(struct_name=name, elements=elements):
            return f"struct {name}({shows(elements, kept)})"
        case Ir.LLVMArrayType(size=size, elt=elt):
            return f"array {size} x {show(elt, kept)}"
        case Ir.LLVMFunctionType(inputs=inputs, output=output, variadic=variadic):
            return f"func {show(output, kept)}({shows(inputs, kept)}{' ...' if variadic else ''})"
        case Ir.LLVMVoidType():
            return "void"
        case Ir.UnregisteredAttr(text=text):
            return kept(text)


def visible(text):
    """A raw form with the text the port kept."""
    return f"raw({text})"


def hidden(text):
    """A raw form as the oracle prints it, without its text."""
    return "raw"


def written(attr):
    """The port's rendering, with the text of a raw form."""
    return show(attr, visible)


def canon(attr):
    """The oracle's rendering: any raw form is just `raw`."""
    return show(attr, hidden)


def outcome(parsed, render):
    match parsed:
        case Success(value=value):
            return render(value.value)
        case Error(message=message):
            return f"error {message}"


def whole(ts, parsed, render):
    """A parse that must consume every token, as xDSL's verdict does."""
    match parsed:
        case Success(value=value):
            return render(value.value) if Ir.peek(ts, value.next).kind == Lexer.EOF else "error"
        case Error(message=_):
            return "error"


def kinds(source):
    match Lexer.tokens(source):
        case Success(value=ts):
            return join(mapList(ts, lambda t: f"{t.kind}:{t.text}"), " ")
        case Error(message=message):
            return f"error {message}"


def lines(source):
    match Lexer.tokens(source):
        case Success(value=ts):
            return join(mapList(ts, lambda t: toString(t.line)), ",")
        case Error(message=message):
            return f"error {message}"


def attr(source):
    return andThen(Lexer.tokens(source), lambda ts: Success(value=outcome(AttrParser.parse_attr(ts, 0), written)))


def ty(source):
    return andThen(Lexer.tokens(source), lambda ts: Success(value=outcome(AttrParser.parse_type(ts, 0), written)))


def verdict(source, read):
    """The port's oracle-vocabulary verdict on the whole of `source`."""
    match Lexer.tokens(source):
        case Success(value=ts):
            return whole(ts, read(ts, 0), canon)
        case Error(message=_):
            return "error"


def at(xs, i, fallback):
    return unwrap_or(listGet(xs, i), fallback)


def agrees(sources, wanted, differences, read, label):
    """Every source the port reads as xDSL does, except the listed differences, which
    xDSL refuses or reads differently while the port keeps them."""
    return forEachList(toList(range(0, len(sources))), lambda i: agreement(at(sources, i, ""), at(wanted, i, ""), differences, read, label))


def agreement(source, wanted, differences, read, label):
    if listContains(differences, source):
        return expect(f"{label} {source}: {wanted != verdict(source, read)}", f"{label} {source}: true")
    else:
        return expect(f"{label} {source}: {verdict(source, read)}", f"{label} {source}: {wanted}")


def oracle_lexer_cases():
    forEachList(toList(range(0, len(Expected.LEX_SOURCES))), lambda i: expect(kinds(at(Expected.LEX_SOURCES, i, "")), at(Expected.LEX_TOKENS, i, "")))
    forEachList(toList(range(0, len(Expected.LEX_SOURCES))), lambda i: expect(lines(at(Expected.LEX_SOURCES, i, "")), at(Expected.LEX_LINES, i, "")))
    # xDSL at efd7a0e7 prints its `{first_char}` template unformatted; the port names the `%`.
    return forEachList(toList(range(0, len(Expected.LEX_ERROR_SOURCES))), lambda i: expect(endsWith(kinds(at(Expected.LEX_ERROR_SOURCES, i, "")), unwrap_or(replace(at(Expected.LEX_ERRORS, i, ""), "{first_char}", "%"), "")), True))


def lexer_cases():
    expect(kinds("x\n@1"), "error line 2: @ identifier expected to start with letter, '_', or '\"'.")
    expect(kinds("% x"), "error line 1: Expected suffix identifier after %")
    expect(kinds("a ; b"), "error line 1: Unexpected character: ;")
    expect(kinds("\"a\\zz\""), f"error {Lexer.UNCLOSED}")
    expect(kinds("\"a\\41\\\"\""), "string:\"a\\41\\\"\" eof:")
    expect([Lexer.BARE, Lexer.AT, Lexer.HASH, Lexer.PERCENT, Lexer.CARET, Lexer.BANG, Lexer.INT, Lexer.FLOAT, Lexer.STRING, Lexer.PUNCT, Lexer.EOF], ["bare-ident", "at-ident", "hash-ident", "percent-ident", "caret-ident", "exclamation-ident", "integer", "float", "string", "punctuation", "eof"])
    expect(Lexer.tokens("a"), Success(value=[Lexer.Token(kind=Lexer.BARE, text="a", line=1), Lexer.Token(kind=Lexer.EOF, text="", line=1)]))
    expect(Lexer.cut("%arg0", 1, 5), "arg0")
    expect(Lexer.is_digit(55), True)
    return expect(Lexer.is_digit(97), False)


def helper_cases():
    """The token cursor and record helpers every parser module shares."""
    match Lexer.tokens("f(1, 2) {a = 1}"):
        case Success(value=ts):
            expect(Ir.peek(ts, 1).text, "(")
            expect(Ir.peek(ts, 99).kind, Lexer.EOF)
            expect(Ir.is_punct(ts, 1, "(") and Ir.is_word(ts, 0, "f") and not Ir.is_word(ts, 2, "f") and not Ir.is_punct(ts, 0, "("), True)
            expect(Ir.fail(ts, 2, "a name"), Error(message="line 1: expected a name, found '1'"))
            expect(outcome(AttrParser.items(ts, 2, ")", AttrParser.parse_attr, []), lambda values: join(mapList(values, canon), ",")), "1:i64,2:i64")
            expect(outcome(AttrParser.items(ts, 3, ")", AttrParser.parse_attr, []), lambda values: join(mapList(values, canon), ",")), "error line 1: expected a type, found ','")
            entries = AttrParser.parse_dictionary(ts, 6)
            expect(outcome(entries, lambda named: join(mapList(named, lambda e: f"{e.name}={canon(e.value)}"), ",")), "a=1:i64")
        case Error(message=message):
            expect(message, "no error")
    named = [Ir.NamedAttr(name="k", value=Ir.UnitAttr())]
    expect(Ir.get_attr(named, "k"), Success(value=Ir.UnitAttr()))
    expect(Ir.done(Ir.UnitAttr(), 3), Success(value=Ir.Parsed(value=Ir.UnitAttr(), next=3)))
    none: list[Ir.Attribute] = []
    expect(shows(none, lambda text: text), "")
    expect(AttrParser.string_contents("\"tab\\09\\22\""), "tab\t\"")
    match Lexer.tokens("(i1, !llvm.ptr) -> (i8)"):
        case Success(value=ts):
            expect(outcome(AttrParser.parse_function_type(ts, 0), canon), "(i1, ptr) -> (i8)")
        case Error(message=message):
            expect(message, "no error")
    match Lexer.tokens("%r = \"t.op\"(%a) : (i1) -> i8"):
        case Success(value=ts):
            return expect(outcome(Parser.parse_operation(ts, 0), lambda op: f"{op.name} {join(op.results, ',')} {join(op.operands, ',')}"), "t.op %r %a")
        case Error(message=message):
            return expect(message, "no error")


def oracle_type_cases():
    return agrees(Expected.TYPE_SOURCES, Expected.TYPES, TYPE_DIFFERENCES, AttrParser.parse_type, "type")


def type_cases():
    expect(ty("!llvm.ptr<1>"), Success(value="ptr"))
    expect(ty("!llvm.array<x i8>"), Success(value="raw(!llvm.array < x i8 >)"))
    expect(ty("!llvm.vec<4 x i32>"), Success(value="raw(!llvm.vec < 4 x i32 >)"))
    expect(ty("!llvm.struct<(i64>"), Success(value="raw(!llvm.struct < ( i64 >)"))
    expect(ty("f64"), Success(value="raw(f64)"))
    expect(ty("tensor<1xi8>"), Success(value="raw(tensor < 1 xi8 >)"))
    expect(ty("!other.thing"), Success(value="raw(!other.thing)"))
    expect(ty("ix"), Success(value="raw(ix)"))
    expect(ty("42"), Success(value="error line 1: expected a type, found '42'"))
    expect(ty("(i64 i64) -> i1"), Success(value="error line 1: expected ',' or ')', found 'i64'"))
    expect(ty("(i64) i1"), Success(value="error line 1: expected '->', found 'i1'"))
    return expect(ty("tensor<1xi8"), Success(value="error line 1: expected '>', found ''"))


def oracle_attribute_cases():
    return agrees(Expected.ATTR_SOURCES, Expected.ATTRS, ATTR_DIFFERENCES, AttrParser.parse_attr, "attribute")


def attribute_cases():
    expect(attr("0x7FF0000000000000 : f64"), Success(value="error line 1: expected an integer that fits 64 bits, found '0x7FF0000000000000'"))
    expect(attr("\"a\\nb\\t\\\\\\\"\\41\" : i8"), Success(value="str(a\nb\t\\\"A)"))
    expect(attr("dense<0> : tensor<1xi8>"), Success(value="raw(dense < 0 >)"))
    expect(attr("#llvm.linkage<external>"), Success(value="raw(#llvm.linkage < external >)"))
    expect(attr("#llvm.framePointerKind<\"non-leaf\">"), Success(value="raw(#llvm.framePointerKind < \"non-leaf\" >)"))
    expect(attr("#loc"), Success(value="raw(#loc)"))
    expect(attr("1.5 : f64"), Success(value="raw(1.5)"))
    expect(attr("-2.5"), Success(value="raw(- 2.5)"))
    expect(attr("99999999999999999999"), Success(value="error line 1: expected an integer that fits 64 bits, found '99999999999999999999'"))
    expect(attr("array<i32: 1, x>"), Success(value="error line 1: expected an integer, found 'x'"))
    expect(attr("array<i32 1>"), Success(value="error line 1: expected '>', found '1'"))
    expect(attr("{1 = 2}"), Success(value="error line 1: expected an attribute name, found '1'"))
    expect(attr("[1 2]"), Success(value="error line 1: expected ',' or ']', found '2'"))
    expect(attr("#x<a <b>"), Success(value="error line 1: expected '>', found ''"))
    return expect(attr("}"), Success(value="error line 1: expected a type, found '}'"))


GROW = """"builtin.module"() ({
  "llvm.func"() <{CConv = #llvm.cconv<ccc>, function_type = !llvm.func<ptr (ptr, i64)>, linkage = #llvm.linkage<external>, sym_name = "grow", visibility_ = 0 : i64}> ({
  ^bb0(%arg0: !llvm.ptr, %arg1: i64):
    %0 = "llvm.mlir.constant"() <{value = 0 : i32}> : () -> i32
    %1 = "llvm.call"(%arg0, %arg1, %0) <{callee = @osprey_list_append_of, operandSegmentSizes = array<i32: 3, 0>, op_bundle_sizes = array<i32>}> : (!llvm.ptr, i64, i32) -> !llvm.ptr
    "llvm.call"(%arg0) <{callee = @osp_ownership_free, operandSegmentSizes = array<i32: 1, 0>, op_bundle_sizes = array<i32>}> : (!llvm.ptr) -> ()
    "llvm.return"(%1) : (!llvm.ptr) -> () loc(#loc3)
  }) : () -> ()
  "llvm.func"() <{function_type = !llvm.func<i1 (i1)>, sym_name = "pick"}> ({
  ^bb0(%arg0: i1):
    %2 = "llvm.mlir.constant"() <{value = dense<0> : tensor<1xi8>}> : () -> i1
    "llvm.cond_br"(%arg0)[^bb1, ^bb2] <{operandSegmentSizes = array<i32: 1, 0, 0>}> : (i1) -> ()
  ^bb1:
    "llvm.br"(%arg0)[^bb3] : (i1) -> ()
  ^bb2:
    "llvm.br"(%2)[^bb3] : (i1) -> ()
  ^bb3(%3: i1):
    "llvm.return"(%3) : (i1) -> ()
  }) : () -> ()
}) {dlti.dl_spec = #dlti.dl_spec<"dlti.endianness" = "little">, llvm.target_triple = "arm64-apple-macosx"} : () -> () loc(fused<#di>[#loc1, callsite(#loc2 at #loc4)])
"""


def op_at(region, b, o):
    return unwrap_or(listGet(unwrap_or(listGet(region.blocks, b), Ir.Block(name="", args=[], arg_types=[], ops=[])).ops, o), Ir.Operation(name="missing", results=[], operands=[], successors=[], properties=[], attributes=[], regions=[], operand_types=[], result_types=[], line=0))


def body(op, r):
    return unwrap_or(listGet(op.regions, r), Ir.Region(blocks=[]))


def check_grow(fn):
    entry = unwrap_or(listGet(body(fn, 0).blocks, 0), Ir.Block(name="", args=[], arg_types=[], ops=[]))
    expect(Ir.string_attr(fn.properties, "sym_name"), Success(value="grow"))
    expect(outcome(Ir.done(unwrap_or(Ir.get_attr(fn.properties, "function_type"), Ir.UnitAttr()), 0), written), "func ptr(ptr, i64)")
    expect(Ir.int_attr(fn.properties, "visibility_"), Success(value=0))
    expect(Ir.int_attr(fn.properties, "sym_name"), Error(message="not an integer attribute"))
    expect(Ir.string_attr(fn.properties, "visibility_"), Error(message="not a string attribute"))
    expect(Ir.get_attr(fn.properties, "absent"), Error(message="no attribute 'absent'"))
    expect(f"{entry.name} {join(entry.args, ' ')} {shows(entry.arg_types, hidden)} {len(entry.ops)}", "^bb0 %arg0 %arg1 ptr, i64 4")
    call = op_at(body(fn, 0), 0, 1)
    expect(f"{call.name} {join(call.results, ',')} {join(call.operands, ',')} {shows(call.operand_types, hidden)} -> {shows(call.result_types, hidden)} line {call.line}", "llvm.call %1 %arg0,%arg1,%0 ptr, i64, i32 -> ptr line 5")
    expect(Ir.symbol_attr(call.properties, "callee"), Success(value="osprey_list_append_of"))
    expect(Ir.dense_attr(call.properties, "operandSegmentSizes"), Success(value=[3, 0]))
    expect(Ir.dense_attr(call.properties, "op_bundle_sizes"), Success(value=[]))
    expect(Ir.dense_attr(call.properties, "callee"), Error(message="not a dense array"))
    expect(Ir.symbol_attr(call.properties, "operandSegmentSizes"), Error(message="not a symbol reference"))
    expect(Ir.int_attr(op_at(body(fn, 0), 0, 0).properties, "value"), Success(value=0))
    free = op_at(body(fn, 0), 0, 2)
    expect(f"{free.name} {len(free.results)} {join(free.operands, ',')} {len(free.result_types)}", "llvm.call 0 %arg0 0")
    return expect(Ir.symbol_attr(free.properties, "callee"), Success(value="osp_ownership_free"))


def check_pick(fn):
    region = body(fn, 0)
    expect(join(mapList(region.blocks, lambda b: f"{b.name}({join(b.args, ',')})={len(b.ops)}"), " "), "^bb0(%arg0)=2 ^bb1()=1 ^bb2()=1 ^bb3(%3)=1")
    branch = op_at(region, 0, 1)
    expect(f"{branch.name} {join(branch.successors, ',')}", "llvm.cond_br ^bb1,^bb2")
    expect(Ir.dense_attr(branch.properties, "operandSegmentSizes"), Success(value=[1, 0, 0]))
    expect(outcome(Ir.done(unwrap_or(Ir.get_attr(op_at(region, 0, 0).properties, "value"), Ir.UnitAttr()), 0), written), "raw(dense < 0 >)")
    return expect(join(op_at(region, 2, 0).successors, ","), "^bb3")


def module_case():
    match Parser.parse_module(GROW):
        case Success(value=module):
            expect(f"{module.name} {len(module.regions)} {len(body(module, 0).blocks)}", "builtin.module 1 1")
            expect(Ir.string_attr(module.attributes, "llvm.target_triple"), Success(value="arm64-apple-macosx"))
            expect(outcome(Ir.done(unwrap_or(Ir.get_attr(module.attributes, "dlti.dl_spec"), Ir.UnitAttr()), 0), written), "raw(#dlti.dl_spec < \"dlti.endianness\" = \"little\" >)")
            check_grow(op_at(body(module, 0), 0, 0))
            return check_pick(op_at(body(module, 0), 0, 1))
        case Error(message=message):
            return expect(message, "no error")


def block_index(blocks, name, i):
    match listGet(blocks, i):
        case Success(value=block):
            return i if block.name == name else block_index(blocks, name, wrapAdd(i, 1))
        case Error(message=_):
            return -1


def entries(named):
    return join(mapList(named, lambda e: f"{e.name}={canon(e.value)}"), ";")


def op_shape(op, region):
    """One operation's shape exactly as the oracle prints xDSL's."""
    successors = join(mapList(op.successors, lambda s: toString(block_index(region.blocks, s, 0))), ",")
    regions = join(mapList(op.regions, lambda r: join(mapList(r.blocks, lambda b: f"{len(b.args)}:{shows(b.arg_types, hidden)}"), ",")), ";")
    types = f"({shows(op.operand_types, hidden)}) -> ({shows(op.result_types, hidden)})"
    return f"{op.name} {len(op.operands)}/{len(op.results)} {types} [{successors}] " + "{" + regions + "} <" + entries(op.properties) + "> {" + entries(op.attributes) + "}"


def flattened(lists):
    return foldList(lists, [], lambda all, xs: listConcat(all, xs))


def op_shapes(op, region):
    """`op` and every operation nested in it, in xDSL's walk order."""
    return listConcat([op_shape(op, region)], flattened(mapList(op.regions, lambda r: flattened(mapList(r.blocks, lambda b: flattened(mapList(b.ops, lambda o: op_shapes(o, r))))))))


def oracle_module_case():
    match Parser.parse_module(Expected.MODULE_SOURCE):
        case Success(value=module):
            return expect(op_shapes(module, Ir.Region(blocks=[])), Expected.MODULE_OPS)
        case Error(message=message):
            return expect(message, "no error")


def parsed(source):
    match Parser.parse_module(source):
        case Success(value=op):
            return f"{op.name}: {len(op.regions)} regions, {join(op.results, ',')}"
        case Error(message=message):
            return f"error {message}"


def shape_cases():
    expect(parsed("\"builtin.module\"() ({}) : () -> ()"), "builtin.module: 1 regions, ")
    expect(parsed("%a, %b = \"t.two\"() : () -> (i1, i1)"), "t.two: 0 regions, %a,%b")
    expect(parsed("\"t.r\"() ({\"t.x\"() : () -> ()}, {^e: \"t.y\"() : () -> ()}) : () -> ()"), "t.r: 2 regions, ")
    expect(parsed("\"t.l\"() : () -> () loc(\"f\":1:2)"), "t.l: 0 regions, ")
    expect(parsed("\"t.l\"() : () -> () loc(a(b)"), "error line 1: expected ')', found ''")
    expect(parsed("\"t.e\"() : () -> () extra"), "error line 1: expected the end of the module, found 'extra'")
    expect(parsed("%a:2 = \"t.p\"() : () -> i1"), "error line 1: expected a single result (packed results are not supported), found '%a'")
    expect(parsed("%a, 3 = \"t.p\"() : () -> i1"), "error line 1: expected a result name, found '3'")
    expect(parsed("t.bare() : () -> ()"), "error line 1: expected an operation name in generic form, found 't.bare'")
    expect(parsed("\"t.o\"(%a, 1) : (i1, i1) -> ()"), "error line 1: expected an operand, found '1'")
    expect(parsed("\"t.s\"()[^a, 1] : () -> ()"), "error line 1: expected a block name, found '1'")
    expect(parsed("\"t.t\"(%a) : () -> ()"), "error line 1: expected 1 operand types for 't.t', found ':'")
    expect(parsed("%r = \"t.t\"() : () -> ()"), "error line 1: expected 1 result types for 't.t', found ':'")
    expect(parsed("\"t.t\"()"), "error line 1: expected ':', found ''")
    expect(parsed("\"t.t\"() <{a = 1} : () -> ()"), "error line 1: expected '>', found ':'")
    expect(parsed("\"t.b\"() ({^b(%x i1): }) : () -> ()"), "error line 1: expected ':', found 'i1'")
    expect(parsed("\"t.b\"() ({^b(x: i1): }) : () -> ()"), "error line 1: expected a block argument, found 'x'")
    expect(parsed("\"t.b\"() ({^b \"t.x\"() : () -> ()}) : () -> ()"), "error line 1: expected ':', found '\"t.x\"'")
    expect(parsed("\"t.b\"() ({\"t.x\"() : () -> () ) : () -> ()"), "error line 1: expected an operation name in generic form, found ')'")
    expect(parsed("\"t.b\"() (\"t.x\") : () -> ()"), "error line 1: expected '{', found '\"t.x\"'")
    expect(parsed("\"t.b\"() ({^a: }, {^b:\n  \"t.y\"() : () -> ()\n  3}) : () -> ()"), "error line 3: expected an operation name in generic form, found '3'")
    return expect(parsed("\"t.q\"() ; "), "error line 1: Unexpected character: ;")


def main():
    test("lexer tokens and lines match xDSL, and every lexical error carries xDSL's message", lambda: oracle_lexer_cases())
    test("lexer kinds, token records, cuts and the port's line-numbered errors", lexer_cases)
    test("the token cursor, item lists, dictionaries, function types, string contents and single operations", helper_cases)
    test("builtin, LLVM dialect and function types read as xDSL reads them, except the documented differences", oracle_type_cases)
    test("raw types and type errors name what was found and where", type_cases)
    test("integer, string, symbol, array, dictionary, dense and dialect attributes read as xDSL reads them, except the documented differences", oracle_attribute_cases)
    test("raw attributes and attribute errors name what was found and where", attribute_cases)
    test("the compiler's generic MLIR for grow and a branching function", module_case)
    test("every operation of the module has xDSL's name, operand and result types, successors, regions, properties and attributes", oracle_module_case)
    return test("operation, region and block shapes with every parse error", shape_cases)
