# osprey: flavor=python
"""Loaded reader use cases: real `mlir-opt --mlir-print-op-generic` shapes of the
compiler's own LLVM IR, every attribute and type form, and every error path.
Implements [IR-LEAN-READER]."""
from xdsl import AttrParser, Ir, Lexer, Parser


def shows(xs):
    return join(mapList(xs, show), ", ")


def show(attr):
    match attr:
        case Ir.IntegerType(width=width):
            return f"i{width}"
        case Ir.IntegerAttr(value=value, type=ty):
            return f"{value}:{show(ty)}"
        case Ir.StringAttr(data=data):
            return f"str({data})"
        case Ir.SymbolRefAttr(name=name):
            return f"@{name}"
        case Ir.UnitAttr():
            return "unit"
        case Ir.ArrayAttr(data=data):
            return f"[{shows(data)}]"
        case Ir.DenseArrayBase(elt_type=ty, data=data):
            return f"dense<{show(ty)}: {join(mapList(data, toString), ' ')}>"
        case Ir.DictionaryAttr(names=names, values=values):
            return f"dict({join(names, ' ')} = {shows(values)})"
        case Ir.FunctionType(inputs=inputs, outputs=outputs):
            return f"({shows(inputs)}) -> ({shows(outputs)})"
        case Ir.LLVMPointerType():
            return "ptr"
        case Ir.LLVMStructType(struct_name=name, elements=elements):
            return f"struct {name}({shows(elements)})"
        case Ir.LLVMArrayType(size=size, elt=elt):
            return f"array {size} x {show(elt)}"
        case Ir.LLVMFunctionType(inputs=inputs, output=output, variadic=variadic):
            return f"func {show(output)}({shows(inputs)}{' ...' if variadic else ''})"
        case Ir.LLVMVoidType():
            return "void"
        case Ir.UnregisteredAttr(text=text):
            return f"raw({text})"


def outcome(parsed):
    match parsed:
        case Success(value=value):
            return show(value.value)
        case Error(message=message):
            return f"error {message}"


def kinds(source):
    match Lexer.tokens(source):
        case Success(value=ts):
            return join(mapList(ts, lambda t: f"{t.kind}:{t.text}"), " ")
        case Error(message=message):
            return f"error {message}"


def attr(source):
    return andThen(Lexer.tokens(source), lambda ts: Success(value=outcome(AttrParser.parse_attr(ts, 0))))


def ty(source):
    return andThen(Lexer.tokens(source), lambda ts: Success(value=outcome(AttrParser.parse_type(ts, 0))))


def lexer_cases():
    expect(kinds("%0 = \"llvm.add\"(%a, %b) : (i64, i64) -> i64"), "percent-ident:%0 punctuation:= string:\"llvm.add\" punctuation:( percent-ident:%a punctuation:, percent-ident:%b punctuation:) punctuation:: punctuation:( bare-ident:i64 punctuation:, bare-ident:i64 punctuation:) punctuation:-> bare-ident:i64 eof:")
    expect(kinds("^bb1 #llvm.linkage<external> !llvm.ptr @main @\"q x\""), "caret-ident:^bb1 hash-ident:#llvm.linkage punctuation:< bare-ident:external punctuation:> exclamation-ident:!llvm.ptr at-ident:@main at-ident:@\"q x\" eof:")
    expect(kinds("1.5 2.0e-3 0x1F 42 ... - + * / ? | [ ] { }"), "float:1.5 float:2.0e-3 integer:0x1F integer:42 punctuation:... punctuation:- punctuation:+ punctuation:* punctuation:/ punctuation:? punctuation:| punctuation:[ punctuation:] punctuation:{ punctuation:} eof:")
    expect(kinds("a // comment ignored\n\tb_c$d.e"), "bare-ident:a bare-ident:b_c$d.e eof:")
    expect(kinds("\"a \\\" b\""), "string:\"a \\\" b\" eof:")
    expect(kinds(""), "eof:")
    expect(kinds("\"open"), "error End of file reached before closing string literal.")
    expect(kinds("x\n@1"), "error line 2: @ identifier expected to start with letter, '_', or '\"'.")
    expect(kinds("% x"), "error line 1: Expected suffix identifier after %")
    expect(kinds("a ; b"), "error line 1: Unexpected character: ;")
    match Lexer.tokens("a\nb\n\nc"):
        case Success(value=ts):
            expect(join(mapList(ts, lambda t: toString(t.line)), ","), "1,2,4,5")
        case Error(message=message):
            expect(message, "no error")
    expect(Lexer.is_digit(55), True)
    return expect(Lexer.is_digit(97), False)


def type_cases():
    expect(ty("i64"), Success(value="i64"))
    expect(ty("i1"), Success(value="i1"))
    expect(ty("!llvm.ptr"), Success(value="ptr"))
    expect(ty("!llvm.ptr<1>"), Success(value="ptr"))
    expect(ty("!llvm.void"), Success(value="void"))
    expect(ty("!llvm.struct<(i64, ptr)>"), Success(value="struct (i64, ptr)"))
    expect(ty("!llvm.struct<\"Pair\", (i32, array<4 x i8>)>"), Success(value="struct Pair(i32, array 4 x i8)"))
    expect(ty("!llvm.array<3 x i64>"), Success(value="array 3 x i64"))
    expect(ty("!llvm.func<i32 (ptr, ...)>"), Success(value="func i32(ptr ...)"))
    expect(ty("!llvm.func<void ()>"), Success(value="func void()"))
    expect(ty("!llvm.func<ptr (i64, i8)>"), Success(value="func ptr(i64, i8)"))
    expect(ty("!llvm.array<x i8>"), Success(value="raw(!llvm.array < x i8 >)"))
    expect(ty("!llvm.vec<4 x i32>"), Success(value="raw(!llvm.vec < 4 x i32 >)"))
    expect(ty("!llvm.struct<(i64>"), Success(value="raw(!llvm.struct < ( i64 >)"))
    expect(ty("(i64, !llvm.ptr) -> i1"), Success(value="(i64, ptr) -> (i1)"))
    expect(ty("() -> ()"), Success(value="() -> ()"))
    expect(ty("(i8) -> (i8, i16)"), Success(value="(i8) -> (i8, i16)"))
    expect(ty("f64"), Success(value="raw(f64)"))
    expect(ty("tensor<1xi8>"), Success(value="raw(tensor < 1 xi8 >)"))
    expect(ty("!other.thing"), Success(value="raw(!other.thing)"))
    expect(ty("ix"), Success(value="raw(ix)"))
    expect(ty("42"), Success(value="error line 1: expected a type, found '42'"))
    expect(ty("(i64 i64) -> i1"), Success(value="error line 1: expected ',' or ')', found 'i64'"))
    expect(ty("(i64) i1"), Success(value="error line 1: expected '->', found 'i1'"))
    return expect(ty("tensor<1xi8"), Success(value="error line 2: expected '>', found ''"))


def attribute_cases():
    expect(attr("7"), Success(value="7:i64"))
    expect(attr("-3 : i32"), Success(value="-3:i32"))
    expect(attr("0x10 : i8"), Success(value="16:i8"))
    expect(attr("0x7FFFFFFFFFFFFFF"), Success(value="576460752303423487:i64"))
    expect(attr("0x7FF0000000000000 : f64"), Success(value="error line 1: expected an integer that fits 64 bits, found '0x7FF0000000000000'"))
    expect(attr("-0x1F : i16"), Success(value="-31:i16"))
    expect(attr("true"), Success(value="1:i1"))
    expect(attr("false"), Success(value="0:i1"))
    expect(attr("unit"), Success(value="unit"))
    expect(attr("\"grow\""), Success(value="str(grow)"))
    expect(attr("\"a\\nb\\t\\\\\\\"\\41\\zz\" : i8"), Success(value="str(a\nb\t\\\"A\\zz)"))
    expect(attr("@osprey_list_append_of"), Success(value="@osprey_list_append_of"))
    expect(attr("@\"quoted name\""), Success(value="@quoted name"))
    expect(attr("[1, \"x\", [unit]]"), Success(value="[1:i64, str(x), [unit]]"))
    expect(attr("[]"), Success(value="[]"))
    expect(attr("{a = 1, \"b c\", d = @f}"), Success(value="dict(a b c d = 1:i64, unit, @f)"))
    expect(attr("array<i32: 3, 0, -1>"), Success(value="dense<i32: 3 0 -1>"))
    expect(attr("array<i32>"), Success(value="dense<i32: >"))
    expect(attr("dense<0> : tensor<1xi8>"), Success(value="raw(dense < 0 >)"))
    expect(attr("#llvm.linkage<external>"), Success(value="raw(#llvm.linkage < external >)"))
    expect(attr("#llvm.framePointerKind<\"non-leaf\">"), Success(value="raw(#llvm.framePointerKind < \"non-leaf\" >)"))
    expect(attr("#loc"), Success(value="raw(#loc)"))
    expect(attr("1.5 : f64"), Success(value="raw(1.5)"))
    expect(attr("-2.5"), Success(value="raw(- 2.5)"))
    expect(attr("!llvm.ptr"), Success(value="ptr"))
    expect(attr("i64"), Success(value="i64"))
    expect(attr("99999999999999999999"), Success(value="error line 1: expected an integer that fits 64 bits, found '99999999999999999999'"))
    expect(attr("array<i32: 1, x>"), Success(value="error line 1: expected an integer, found 'x'"))
    expect(attr("array<i32 1>"), Success(value="error line 1: expected '>', found '1'"))
    expect(attr("{1 = 2}"), Success(value="error line 1: expected an attribute name, found '1'"))
    expect(attr("[1 2]"), Success(value="error line 1: expected ',' or ']', found '2'"))
    expect(attr("#x<a <b>"), Success(value="error line 2: expected '>', found ''"))
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
    expect(outcome(Ir.done(unwrap_or(Ir.get_attr(fn.properties, "function_type"), Ir.UnitAttr()), 0)), "func ptr(ptr, i64)")
    expect(Ir.int_attr(fn.properties, "visibility_"), Success(value=0))
    expect(Ir.int_attr(fn.properties, "sym_name"), Error(message="not an integer attribute"))
    expect(Ir.string_attr(fn.properties, "visibility_"), Error(message="not a string attribute"))
    expect(Ir.get_attr(fn.properties, "absent"), Error(message="no attribute 'absent'"))
    expect(f"{entry.name} {join(entry.args, ' ')} {shows(entry.arg_types)} {len(entry.ops)}", "^bb0 %arg0 %arg1 ptr, i64 4")
    call = op_at(body(fn, 0), 0, 1)
    expect(f"{call.name} {join(call.results, ',')} {join(call.operands, ',')} {shows(call.operand_types)} -> {shows(call.result_types)} line {call.line}", "llvm.call %1 %arg0,%arg1,%0 ptr, i64, i32 -> ptr line 5")
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
    expect(outcome(Ir.done(unwrap_or(Ir.get_attr(op_at(region, 0, 0).properties, "value"), Ir.UnitAttr()), 0)), "raw(dense < 0 >)")
    return expect(join(op_at(region, 2, 0).successors, ","), "^bb3")


def module_case():
    match Parser.parse_module(GROW):
        case Success(value=module):
            expect(f"{module.name} {len(module.regions)} {len(body(module, 0).blocks)}", "builtin.module 1 1")
            expect(Ir.string_attr(module.attributes, "llvm.target_triple"), Success(value="arm64-apple-macosx"))
            expect(outcome(Ir.done(unwrap_or(Ir.get_attr(module.attributes, "dlti.dl_spec"), Ir.UnitAttr()), 0)), "raw(#dlti.dl_spec < \"dlti.endianness\" = \"little\" >)")
            check_grow(op_at(body(module, 0), 0, 0))
            return check_pick(op_at(body(module, 0), 0, 1))
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
    expect(parsed("\"t.l\"() : () -> () loc(a(b)"), "error line 2: expected ')', found ''")
    expect(parsed("\"t.e\"() : () -> () extra"), "error line 1: expected the end of the module, found 'extra'")
    expect(parsed("%a:2 = \"t.p\"() : () -> i1"), "error line 1: expected a single result (packed results are not supported), found '%a'")
    expect(parsed("%a, 3 = \"t.p\"() : () -> i1"), "error line 1: expected a result name, found '3'")
    expect(parsed("t.bare() : () -> ()"), "error line 1: expected an operation name in generic form, found 't.bare'")
    expect(parsed("\"t.o\"(%a, 1) : (i1, i1) -> ()"), "error line 1: expected an operand, found '1'")
    expect(parsed("\"t.s\"()[^a, 1] : () -> ()"), "error line 1: expected a block name, found '1'")
    expect(parsed("\"t.t\"(%a) : () -> ()"), "error line 1: expected 1 operand types for 't.t', found ':'")
    expect(parsed("%r = \"t.t\"() : () -> ()"), "error line 1: expected 1 result types for 't.t', found ':'")
    expect(parsed("\"t.t\"()"), "error line 2: expected ':', found ''")
    expect(parsed("\"t.t\"() <{a = 1} : () -> ()"), "error line 1: expected '>', found ':'")
    expect(parsed("\"t.b\"() ({^b(%x i1): }) : () -> ()"), "error line 1: expected ':', found 'i1'")
    expect(parsed("\"t.b\"() ({^b(x: i1): }) : () -> ()"), "error line 1: expected a block argument, found 'x'")
    expect(parsed("\"t.b\"() ({^b \"t.x\"() : () -> ()}) : () -> ()"), "error line 1: expected ':', found '\"t.x\"'")
    expect(parsed("\"t.b\"() ({\"t.x\"() : () -> () ) : () -> ()"), "error line 1: expected an operation name in generic form, found ')'")
    expect(parsed("\"t.b\"() (\"t.x\") : () -> ()"), "error line 1: expected '{', found '\"t.x\"'")
    expect(parsed("\"t.b\"() ({^a: }, {^b:\n  \"t.y\"() : () -> ()\n  3}) : () -> ()"), "error line 3: expected an operation name in generic form, found '3'")
    return expect(parsed("\"t.q\"() ; "), "error line 1: Unexpected character: ;")


def main():
    test("lexer tokens, lines, comments and every lexical error", lexer_cases)
    test("builtin, LLVM dialect, function and unregistered types", type_cases)
    test("integer, string, symbol, array, dictionary, dense and dialect attributes", attribute_cases)
    test("the compiler's generic MLIR for grow and a branching function", module_case)
    return test("operation, region and block shapes with every parse error", shape_cases)
