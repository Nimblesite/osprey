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
    rest = listConcat(kind_cases(), listConcat(string_cases(), listConcat(value_cases(), listConcat(scope_cases(), listConcat(syntax_cases(), macro_cases())))))
    return listConcat(renders, listConcat(rest, expanded_cases(context)))


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
        same("pretty list JSON", V.json_text(V.list_([V.int_(1), V.int_(2)]), 2), Expected.PRETTY_LIST_JSON),
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


# Expression diagnostics the port reports verbatim (CPython Jinja 3.1.6 also
# rejects each template; probes live in target/logs/jinja-coverage-work/).
EXPR_ERRORS = [
    ["unexpected token", "{{ ) }}", "unexpected token `)` in expression"],
    ["attribute name", "{{ obj.+ }}", "expected attribute name after '.'"],
    ["test name", "{{ 1 is }}", "expected test name after 'is'"],
]


# Malformed expressions whose inner diagnostic the port's trailing-token check
# overwrites (deterministically); only the rejection itself is asserted. CPython
# fails each template with its own syntax error text.
MALFORMED_EXPR = [
    ["unclosed paren", "{{ (1 + 2 }}"],
    ["unclosed list", "{{ [1, 2 }}"],
    ["dict missing colon", "{{ {1} }}"],
    ["dict pair missing colon", "{{ {'x': 1, 'y'} }}"],
    ["dict missing comma", "{{ {'x': 1 'y': 2} }}"],
    ["method arg missing comma", "{{ name.upper(1 2) }}"],
    ["call arg missing comma", "{{ range(1 2) }}"],
    ["subscript close", "{{ nums[1 2] }}"],
    ["slice close", "{{ nums[1:2 3] }}"],
    ["slice step close", "{{ nums[::2 3] }}"],
    ["filter name", "{{ 1|2 }}"],
    ["filter args", "{{ nums|join(1 2) }}"],
    ["test args", "{{ 1 is divisibleby(2 3) }}"],
]


# Port-documented statement and lexer diagnostics (packages/jinja2/src/parser.py
# and lexer.py). CPython Jinja 3.1.6 fails every template too, with its own
# 'Encountered unknown tag' / 'unexpected end of template' wording.
STATEMENT_ERRORS = [
    ["unknown statement", "{% bogus %}", "unknown statement `bogus`"],
    ["unknown in if body", "{% if enabled %}{% bogus %}{% endif %}", "unknown statement `bogus`"],
    ["endfor closes if", "{% if enabled %}{% endfor %}", "unknown statement `endfor`"],
    ["unknown in elif body", "{% if enabled %}{% elif nothing %}{% bogus %}{% endif %}", "unknown statement `bogus`"],
    ["unknown in else body", "{% if enabled %}{% else %}{% bogus %}{% endif %}", "unknown statement `bogus`"],
    ["unknown in for body", "{% for n in nums %}{% bogus %}{% endfor %}", "unknown statement `bogus`"],
    ["unknown in for else", "{% for n in nums %}{% else %}{% bogus %}{% endfor %}", "unknown statement `bogus`"],
    ["unknown in macro body", "{% macro m() %}{% bogus %}{% endmacro %}", "unknown statement `bogus`"],
    ["unterminated tag", "{{ name", "unexpected end of template, expected 'end of print statement'"],
    ["unterminated string", "{{ 'abc", "unexpected end of template, expected 'end of print statement'"],
]


# Port-documented name-lookup and block diagnostics (packages/jinja2/src/
# filters.py, expr.py, render.py). CPython fails each template too (Template
# Assertion/Runtime/SyntaxError); the port wording names its own subset limit.
LOOKUP_ERRORS = [
    ["unknown filter", "{{ 1|nosuchfilter }}", "No filter named 'nosuchfilter'"],
    ["unknown test", "{{ 1 is nosuchtest }}", "No test named 'nosuchtest'"],
    ["unknown string method", "{{ 'a'.foo() }}", "method `foo` is not supported on strings"],
    ["unknown dict method", "{{ obj.foo() }}", "method `foo` is not supported on dicts"],
    ["raise exception", "{{ raise_exception('boom') }}", "boom"],
    ["strftime_now", "{{ strftime_now('%Y') }}", "strftime_now is not available in the Osprey port"],
    ["loop cond error", "{% for n in nums if ) %}{% endfor %}", "unexpected token `)` in expression"],
    ["loop filtered body error", "{% for n in nums if true %}{{ ) }}{% endfor %}", "unexpected token `)` in expression"],
    ["loop iterable error", "{% for n in ) %}{% endfor %}", "unexpected token `)` in expression"],
    ["loop body error", "{% for n in nums %}{{ ) }}{% endfor %}", "unexpected token `)` in expression"],
    ["set expression error", "{% set x = ) %}", "unexpected token `)` in expression"],
    ["branch cond error", "{% if ) %}a{% endif %}", "unexpected token `)` in expression"],
]


# Templates both implementations reject with different wordings (CPython raises
# TypeError with its own operand text); only the rejection itself is asserted.
DIVERGENT_ERRORS = [
    ["int plus string", "{{ 1 + 'a' }}"],
    ["string times string", "{{ 'a' * 'b' }}"],
    ["list plus int", "{{ [1] + 1 }}"],
    ["call non function", "{{ 1(2) }}"],
    ["string divided", "{{ 'a' / 'b' }}"],
    ["string floordiv", "{{ 'a' // 2 }}"],
    ["string modulo", "{{ 'a' % 2 }}"],
    ["pair concatenated", "{{ 'a' + (obj|items|first) }}"],
]


def error_run(context, entry):
    name = unwrap_or(listGet(entry, 0), "")
    template = unwrap_or(listGet(entry, 1), "")
    want = unwrap_or(listGet(entry, 2), "")
    match R.render(R.chat_environment(), template, context):
        case Error(message=message):
            return same(name, message, want)
        case Success(value=value):
            return Check.failed(name, value)


def error_table(context, table):
    return toList(map(range(len(table)), lambda i: error_run(context, unwrap_or(listGet(table, i), []))))


def oracle_errors(context):
    return error_table(context, Expected.ERRORS)


def rejects_like_python(context, entry):
    name = unwrap_or(listGet(entry, 0), "")
    template = unwrap_or(listGet(entry, 1), "")
    match R.render(R.chat_environment(), template, context):
        case Error(message=message):
            return Check.truth(name, len(message) > 0, "rejected with a diagnostic")
        case Success(value=value):
            return Check.failed(name, value)


def divergent_errors(context):
    return toList(map(range(len(DIVERGENT_ERRORS)), lambda i: rejects_like_python(context, unwrap_or(listGet(DIVERGENT_ERRORS, i), []))))


def error_cases(context):
    return listConcat(error_table(context, EXPR_ERRORS), listConcat(error_table(context, STATEMENT_ERRORS), listConcat(error_table(context, LOOKUP_ERRORS), listConcat(oracle_errors(context), listConcat(divergent_errors(context), malformed_errors(context))))))


def malformed_errors(context):
    return toList(map(range(len(MALFORMED_EXPR)), lambda i: rejects_like_python(context, unwrap_or(listGet(MALFORMED_EXPR, i), []))))


def fallback_cases():
    return [
        same("chars of empty", V.chars(""), []),
        same("length of undefined", F.apply_filter("length", V.undefined("x"), [], []), V.int_(0)),
        same("method on int", V.is_error(F.call_method(V.int_(1), "upper", [])), True),
        same("bind fallback innermost", V.lookup(V.bind_found(V.Env(frames=[{}, {}]), "x", V.int_(1)), "x"), V.int_(1)),
    ]


def dict_write_cases():
    d = V.dict_(["a", "b", "c"], [V.int_(1), V.int_(2), V.int_(3)])
    appended = V.dict_set(d, "d", V.int_(4))
    replaced = V.dict_set(d, "b", V.int_(9))
    return [
        same("dict append key", V.dict_get(appended, V.text("d")), V.int_(4)),
        same("dict append count", len(appended.keys), 4),
        same("dict replace key", V.dict_get(replaced, V.text("b")), V.int_(9)),
        same("dict replace persistence", [V.dict_get(replaced, V.text("a")), V.dict_get(replaced, V.text("c"))], [V.int_(1), V.int_(3)]),
    ]


def empty_macro_node(nodes):
    node = unwrap_or(listGet(nodes, 0), P.blank("missing"))
    return [same("empty macro name", node.name, "m"), same("empty macro params", node.params, [])]


def empty_macro_cases():
    match L.scan("{% macro m() %}Hi{% endmacro %}", True, True):
        case Success(value=value):
            match P.parse(value):
                case Success(value=parsed):
                    return empty_macro_node(parsed)
                case Error(message=message):
                    return [Check.failed("empty macro parse", message)]
        case Error(message=message):
            return [Check.failed("empty macro lex", message)]


def expanded_cases(context):
    return listConcat(error_cases(context), listConcat(fallback_cases(), listConcat(dict_write_cases(), empty_macro_cases())))


def main():
    match Check.strictly(lambda: Check.report(all_cases())):
        case Success(value=value):
            return value
        case Error(message=message):
            return print("suite stopped: " + message)
