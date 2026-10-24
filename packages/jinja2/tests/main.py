# osprey: flavor=python
from jinja2 import Render as R
from jinja2 import Value as V
from jinja2 import Lexer as L
from jinja2 import Parser as P
from jinja2 import Expr as E
from jinja2 import Filters as F
from testing import Check
from jinja2_expected import Expected

def same(name, got, want):
    return Check.truth(name, structuralEquals(got, want), "values differ")


def parsed_same(name, result, want):
    match result:
        case Success(value=value):
            return same(name, value, want)
        case Error(message=message):
            return Check.failed(name, message)


def run(case, context):
    name = unwrap_or(listGet(case, 0), "")
    template = unwrap_or(listGet(case, 1), "")
    expected = unwrap_or(listGet(case, 2), "")
    match R.render(R.chat_environment(), template, context):
        case Success(value=value):
            return same(name, value, expected)
        case Error(message=message):
            return Check.failed(name, message)


def all_cases():
    context = {"name": V.text("world"), "nums": V.list_([V.int_(1), V.int_(2), V.int_(3)]), "enabled": V.bool_(True), "nothing": V.none(), "obj": V.dict_(["x"], [V.int_(4)])}
    renders = toList(map(range(len(Expected.RENDERS)), lambda i: run(unwrap_or(listGet(Expected.RENDERS, i), []), context)))
    return listConcat(renders, listConcat(kind_cases(), listConcat(string_cases(), listConcat(value_cases(), listConcat(scope_cases(), listConcat(syntax_cases(), macro_cases()))))))


def kind_cases():
    values = [V.none(), V.bool_(True), V.int_(2), V.float_(2.5), V.text("x"), V.list_([]), V.dict_([], []), V.undefined("x"), V.pair("x", V.int_(1)), V.func("range")]
    kinds = [V.K_NONE, V.K_BOOL, V.K_INT, V.K_FLOAT, V.K_STRING, V.K_LIST, V.K_DICT, V.K_UNDEFINED, V.K_PAIR, V.K_FUNC]
    return [
        same("value kinds", toList(map(range(len(values)), lambda i: unwrap_or(listGet(values, i), V.none()).kind)), kinds),
        same("value predicates", [V.is_none(V.none()), V.is_undefined(V.undefined("x")), V.is_string(V.text("x")), V.is_number(V.float_(2.0))], [True, True, True, True]),
        same("truthiness", [V.truthy(V.none()), V.truthy(V.list_([])), V.truthy(V.int_(1))], [False, False, True]),
        same("numeric equality", V.equal(V.int_(2), V.float_(2.0)), True),
        same("numeric ordering", V.less(V.int_(2), V.float_(2.5)), True),
        same("numeric view", V.num_of(V.int_(2)), 2.0),
        same("error propagation", V.is_error(V.halt("broken")), True),
        same("Value construction", V.Value(kind=V.K_INT, bval=False, ival=2, fval=0.0, sval="", elems=[], keys=[], vals=[]), V.int_(2)),
    ]


def string_cases():
    reverse = V.Slice(start=0, stop=0, step=-1, has_start=False, has_stop=False, has_step=True)
    return [
        same("unicode characters", V.chars("aé🙂"), Expected.CHARS),
        same("unicode length", V.str_len("aé🙂"), len(Expected.CHARS)),
        same("unicode negative index", V.str_get("aé🙂", V.int_(-1)), V.text("🙂")),
        same("character values", V.char_values("aé🙂"), [V.text("a"), V.text("é"), V.text("🙂")]),
        same("reverse string", V.slice_str("aé🙂", reverse), Expected.REVERSED),
        same("reverse values", V.slice_list([V.int_(1), V.int_(2)], reverse), [V.int_(2), V.int_(1)]),
        same("whitespace split", V.str_split(" a\t b\n c "), Expected.SPLIT),
        same("separator split", V.str_split_sep("a,,b,", ","), Expected.SEP_SPLIT),
        same("capitalize", V.capitalize("hELLO"), Expected.CAPITALIZED),
        same("repr text", V.repr_text(V.text("a\nb")), Expected.REPR),
        same("concat boolean", V.concat_text(V.bool_(True)), Expected.BOOL_TEXT),
    ]


def value_cases():
    original = V.dict_of([V.pair("x", V.int_(1))])
    updated = V.dict_set(original, "x", V.int_(2))
    return [
        same("dictionary get", V.dict_get(updated, V.text("x")), V.int_(2)),
        same("attribute get", V.attr_get(updated, "x"), V.int_(2)),
        same("key lookup", V.key_of(original, "x"), V.int_(1)),
        same("dictionary persistence", V.dict_get(original, V.text("x")), V.int_(1)),
        same("membership", V.contains(updated, V.text("x")), True),
        same("compact json", V.json_text(updated, 0), Expected.JSON),
        same("pretty json", V.json_text(updated, 2), Expected.PRETTY_JSON),
        same("floor division", V.py_floordiv(-7, 3), Expected.FLOORDIV),
        same("modulo", V.py_mod(-7, 3), Expected.MODULO),
    ]


def scope_cases():
    env = V.Env(frames=[{"x": V.int_(1)}, {}])
    bound = V.bind(env, "y", V.int_(2))
    changed = V.bind_found(bound, "x", V.int_(3))
    return [
        same("scope lookup", V.lookup(bound, "y"), V.int_(2)),
        same("outer binding", V.lookup(changed, "x"), V.int_(3)),
        same("scope persistence", V.lookup(env, "x"), V.int_(1)),
        same("explicit frame binding", V.lookup(V.bind_at(env, 0, "x", V.int_(4)), "x"), V.int_(4)),
        same("expression evaluation", E.eval_expr("x + y", bound), V.int_(3)),
        same("filter", F.apply_filter("upper", V.text("hello"), [], []), V.text("HELLO")),
        same("keyword filter", F.apply_filter("tojson", V.dict_(["x"], [V.int_(2)]), [], [V.KwVal(name="indent", value=V.int_(2))]), V.text(Expected.PRETTY_JSON)),
        same("test", F.apply_test("integer", V.int_(1), []), V.bool_(True)),
        same("method", F.call_method(V.text("hello"), "startswith", [V.text("he")]), V.bool_(True)),
    ]


def syntax_cases():
    node = P.Node(kind="text", text="", name="", attr="", var="", var2="", cond="", iterable="", branches=[], body=[], else_body=[], params=[])
    return [
        same("environment", R.Environment(trim_blocks=True, lstrip_blocks=True), R.chat_environment()),
        parsed_same("lexer", L.scan("hello", True, True), [L.Chunk(kind="data", text="hello")]),
        parsed_same("parser", P.parse([L.Chunk(kind="variable", text="x")]), [P.Node(kind="output", text="x", name="", attr="", var="", var2="", cond="", iterable="", branches=[], body=[], else_body=[], params=[])]),
        same("blank node", P.blank("text"), node),
        same("branch", P.Branch(cond="x", body=[node]).cond, "x"),
        same("parser step", P.Step(nodes=[node], next=1).next, 1),
        same("tag head", L.TagHead(pos=1, kind="var", sign="-", content_start=4).content_start, 4),
        same("tag end", L.TagEnd(content="x", end=7, rsign="-").content, "x"),
        same("consumed newline", L.Consumed(pos=2, ended_nl=True).ended_nl, True),
    ]


def macro_node(chunks):
    match P.parse(chunks):
        case Success(value=value):
            node = unwrap_or(listGet(value, 0), P.blank("missing"))
            return [same("macro name", node.name, Expected.MACRO_NAME), same("macro parameters", node.params, Expected.MACRO_PARAMS)]
        case Error(message=message):
            return [Check.failed("macro parse", message)]


def macro_parse():
    match L.scan(Expected.MACRO_SOURCE, True, True):
        case Success(value=value):
            return macro_node(value)
        case Error(message=message):
            return [Check.failed("macro lex", message)]


def macro_rejected():
    match R.render(R.chat_environment(), Expected.MACRO_SOURCE, {}):
        case Error(message=message):
            return same("unsupported macro rendering", message, "{% macro %} is not supported by @osprey/jinja2")
        case Success(value=value):
            return Check.failed("unsupported macro rendering", value)


def macro_cases():
    return listAppend(macro_parse(), macro_rejected())


def main():
    match Check.strictly(lambda: Check.report(all_cases())):
        case Success(value=value):
            return value
        case Error(message=message):
            return print("suite stopped: " + message)
