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
    context = {"name": V.text("world"), "nums": V.list_([V.int_(1), V.int_(2), V.int_(3)]), "enabled": V.bool_(True), "nothing": V.none(), "obj": V.typed_dict([V.text("x")], [V.int_(4)])}
    renders = toList(map(range(len(Expected.RENDERS)), lambda i: run(unwrap_or(listGet(Expected.RENDERS, i), []), context)))
    rest = listConcat(kind_cases(), listConcat(string_cases(), listConcat(value_cases(), listConcat(scope_cases(), listConcat(syntax_cases(), macro_cases())))))
    return listConcat(renders, listConcat(error_chain_cases(), listConcat(rest, expanded_cases(context))))


def error_chain_cases():
    failure = V.halt("original evaluation error")
    return [
        same("filter preserves error", F.apply_filter("list", failure, [], []), failure),
        same("predicate preserves error", F.apply_test("defined", failure, []), failure),
    ]


def kind_cases():
    values = [V.none(), V.bool_(True), V.int_(2), V.float_(2.5), V.text("x"), V.list_([]), V.dict_([], []), V.undefined("x"), V.typed_pair(V.text("x"), V.int_(1)), V.func("range")]
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
    env = V.Env(frames=[{"x": V.int_(1)}, {}], macros=[])
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
    node = P.Node(kind="text", text="", name="", attr="", var="", var2="", cond="", iterable="", branches=[], body=[], else_body=[], params=[], defaults=[])
    return [
        same("environment", R.Environment(trim_blocks=True, lstrip_blocks=True), R.chat_environment()),
        parsed_same("lexer", L.scan("hello", True, True), [L.Chunk(kind="data", text="hello")]),
        parsed_same("parser", P.parse([L.Chunk(kind="variable", text="x")]), [P.Node(kind="output", text="x", name="", attr="", var="", var2="", cond="", iterable="", branches=[], body=[], else_body=[], params=[], defaults=[])]),
        same("blank node", P.blank("text"), node),
        same("branch", P.Branch(cond="x", body=[node]).cond, "x"),
        same("parser step", P.Step(nodes=[node], next=1).next, 1),
        same("statement keyword", P.Kw(word="macro", rest="m()").word, "macro"),
        same("parameter record", P.Param(names=["a"], defaults=[""]).names, ["a"]),
        same("parameter split", P.Split(name="b", value="7").value, "7"),
        same("tag head", L.TagHead(pos=1, kind="var", sign="-", content_start=4).content_start, 4),
        same("tag end", L.TagEnd(content="x", end=7, rsign="-").content, "x"),
        same("consumed newline", L.Consumed(pos=2, ended_nl=True).ended_nl, True),
    ]


def macro_node(chunks):
    """The name, parameters and defaults of the first macro in `chunks`."""
    match P.parse(chunks):
        case Success(value=value):
            node = unwrap_or(listGet(value, 0), P.blank("missing"))
            return [same("macro name", node.name, Expected.MACRO_NAME),
                    same("macro parameters", node.params, Expected.MACRO_PARAMS),
                    same("macro defaults", node.defaults, [""])]
        case Error(message=message):
            return [Check.failed("macro parse", message)]


def macro_parse():
    match L.scan(Expected.MACRO_SOURCE, True, True):
        case Success(value=value):
            return macro_node(value)
        case Error(message=message):
            return [Check.failed("macro lex", message)]


def defaulted_macro_node(chunks):
    """The parameters and defaults of a macro that declares a default."""
    match P.parse(chunks):
        case Success(value=value):
            node = unwrap_or(listGet(value, 0), P.blank("missing"))
            return [same("defaulted macro parameters", node.params, ["a", "b", "c"]),
                    same("defaulted macro defaults", node.defaults, ["", "7", "a + b"])]
        case Error(message=message):
            return [Check.failed("defaulted macro parse", message)]


def defaulted_macro_parse():
    match L.scan("{% macro m(a, b=7, c = a + b) %}{% endmacro %}", True, True):
        case Success(value=value):
            return defaulted_macro_node(value)
        case Error(message=message):
            return [Check.failed("defaulted macro lex", message)]


def macro_table_cases():
    """The macro table API the renderer's expansion path is built on."""
    match L.scan("{% macro m(a) %}{% endmacro %}", True, True):
        case Success(value=chunks):
            return macro_table_from(chunks)
        case Error(message=message):
            return [Check.failed("macro table lex", message)]


def macro_table_from(chunks):
    match P.parse(chunks):
        case Success(value=nodes):
            node = unwrap_or(listGet(nodes, 0), P.blank("missing"))
            empty = V.Env(frames=[{}], macros=[])
            env = V.bind_macro(empty, node)
            return [
                same("macro declaration", V.Macro(node=node, frames=[]).node.name, "m"),
                same("macro table bind", len(env.macros), 1),
                same("macro table lookup", V.find_macro(env, 0, "m").node.name, "m"),
                same("macro table miss", V.find_macro(env, 0, "absent").node.name, ""),
                same("macro reference kind", V.macro_ref("m").kind, V.K_MACRO),
                same("environment copy keeps macros", len(V.env_with(env, [{}]).macros), 1),
                same("undefined macro expansion", V.is_error(R.expand_macro(empty, "absent", [], [])), True),
            ]
        case Error(message=message):
            return [Check.failed("macro table parse", message)]


def macro_cases():
    return listConcat(macro_parse(), listConcat(defaulted_macro_parse(), macro_table_cases()))


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


# Port-documented statement and lexer diagnostics (src/osprey/packages/jinja2/src/parser.py
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


# Port-documented name-lookup and block diagnostics (src/osprey/packages/jinja2/src/
# filters.py, expr.py, render.py). CPython fails each template too (Template
# Assertion/Runtime/SyntaxError); the port wording names its own subset limit.
LOOKUP_ERRORS = [
    ["negated unknown test", "{{ 1 is not nosuchtest }}", "No test named 'nosuchtest'"],
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
        same("bind fallback innermost", V.lookup(V.bind_found(V.Env(frames=[{}, {}], macros=[]), "x", V.int_(1)), "x"), V.int_(1)),
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


def json_value(doc):
    """A JSON document as a template value: the chat-template input boundary."""
    match doc:
        case JsonNull:
            return V.none()
        case JsonBool(flag):
            return V.bool_(flag)
        case JsonInt(number):
            return V.int_(number)
        case JsonFloat(decimal):
            return V.float_(decimal)
        case JsonString(content):
            return V.text(content)
        case JsonArray(values):
            return V.list_(toList(map(range(len(values)), lambda i: json_value(unwrap_or(listGet(values, i), JsonNull)))))
        case JsonObject(members):
            return V.dict_of(toList(map(range(len(members)), lambda i: json_member(unwrap_or(listGet(members, i), JsonMember("", JsonNull))))))


def json_member(member):
    """One JSON object member as a `(key, value)` pair."""
    match member:
        case JsonMember(name, value):
            return V.pair(name, json_value(value))


def json_context(doc):
    """A JSON object as the renderer's top-level context; anything else is an `Error`."""
    match doc:
        case JsonObject(members):
            return context_members(members, 0, {})
        case _:
            return Error(message="the chat context must be a JSON object")


def context_members(members, i, frame):
    """Move object member `i` into the context frame."""
    if i >= len(members):
        return Success(value=frame)
    else:
        match unwrap_or(listGet(members, i), JsonMember("", JsonNull)):
            case JsonMember(name, value):
                return context_members(members, i + 1, mapSet(frame, name, json_value(value)))


def render_chat(context_text):
    """Render the checked-in Qwen3.8 template over one JSON context."""
    match jsonDecode(context_text):
        case Success(value=doc):
            return render_context(doc)
        case Error(message=message):
            return Error(message=message)


def render_context(doc):
    """Render the template over an already decoded context document."""
    match json_context(doc):
        case Success(value=context):
            return R.render(R.chat_environment(), Expected.CHAT_TEMPLATE, context)
        case Error(message=message):
            return Error(message=message)


def chat_case(entry):
    """One `CHAT_RENDERS` row: the template's output over the oracle's context."""
    name = unwrap_or(listGet(entry, 0), "")
    match render_chat(unwrap_or(listGet(entry, 1), "{}")):
        case Success(value=value):
            return same(name, value, unwrap_or(listGet(entry, 2), ""))
        case Error(message=message):
            return Check.failed(name, message)


def chat_error_case(entry):
    """One `CHAT_ERRORS` row: the template rejects the oracle's context."""
    name = unwrap_or(listGet(entry, 0), "")
    match render_chat(unwrap_or(listGet(entry, 1), "{}")):
        case Error(message=message):
            return same(name, message, unwrap_or(listGet(entry, 2), ""))
        case Success(value=value):
            return Check.failed(name, value)


def chat_cases():
    renders = toList(map(range(len(Expected.CHAT_RENDERS)), lambda i: chat_case(unwrap_or(listGet(Expected.CHAT_RENDERS, i), []))))
    return listConcat(renders, toList(map(range(len(Expected.CHAT_ERRORS)), lambda i: chat_error_case(unwrap_or(listGet(Expected.CHAT_ERRORS, i), [])))))


# Divergences from CPython Jinja 3.1.6 that @osprey/jinja2 documents in its README.
# Each case pins the port's own result beside the reason it cannot match: values are
# immutable, so a namespace write cannot cross a macro call (CPython adds it to the
# caller's namespace and renders ":1"), and a parenthesised tuple has no representation
# distinct from a list (CPython renders "('a', 'b'):('a',)").
VISION_CONTEXT = "{\"add_vision_id\": true, \"messages\": [{\"role\": \"user\", \"content\": [{\"type\": \"image\", \"image\": \"a\"}, {\"type\": \"image\", \"image\": \"b\"}]}, {\"role\": \"user\", \"content\": [{\"type\": \"image\", \"image\": \"c\"}]}]}"


def vision_counts(text):
    """How many times each picture number appears, by splitting on its prefix."""
    return toList(map(range(3), lambda i: len(V.str_split_sep(text, "Picture " + toString(i + 1) + ":")) - 1))


def vision_divergence():
    """Picture numbering restarts in each message (Jinja counts 1, 2, 3 across both)."""
    name = "vision picture numbering is call scoped (known divergence)"
    match render_chat(VISION_CONTEXT):
        case Success(value=value):
            return same(name, vision_counts(value), [2, 1, 0])
        case Error(message=message):
            return Check.failed(name, message)


def divergence_cases():
    context = {}
    return [
        vision_divergence(),
        run(["namespace write inside a macro (known divergence)", "{% macro bump(ns) %}{% set ns.n = ns.n + 1 %}{% endmacro %}{% set ns = namespace(n=0) %}{{ bump(ns) }}:{{ ns.n }}", ":0"], context),
        run(["tuple literal prints as a list (known divergence)", "{{ ('a', 'b') }}:{{ ('a',) }}", "['a', 'b']:['a']"], context),
    ]


def expanded_cases(context):
    return listConcat(error_cases(context), listConcat(fallback_cases(), listConcat(dict_write_cases(), listConcat(empty_macro_cases(), listConcat(chat_cases(), divergence_cases())))))


def main():
    match Check.strictly(lambda: Check.report(all_cases())):
        case Success(value=value):
            return value
        case Error(message=message):
            return print("suite stopped: " + message)