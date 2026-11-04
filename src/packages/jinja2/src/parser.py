# osprey: flavor=python
# osprey: namespace=jinja2
# osprey: module=Parser
"""The template parser: chunks from `Lexer` become a tree of nodes.

The grammar is the chat-template subset: text, `{{ expr }}`, `{% if/elif/else/
endif %}`, `{% for x in xs %}` (with optional `if` filter and `k, v` unpacking,
plus an optional `{% else %}`), `{% set x = expr %}` (including `ns.attr` writes)
and `{% macro %}`. Statements are matched by their leading keyword; expression
strings are kept verbatim for `Expr` to evaluate.
"""

from jinja2.Lexer import Chunk

__all__ = ["Node", "Branch", "Step", "parse", "blank"]


class Node:
    """One template node; only the fields its `kind` names are meaningful."""
    kind: str
    text: str
    name: str
    attr: str
    var: str
    var2: str
    cond: str
    iterable: str
    branches: list[Branch]
    body: list[Node]
    else_body: list[Node]
    params: list[str]


class Branch:
    """One `if`/`elif` branch (`cond` is `""` for `else`)."""
    cond: str
    body: list[Node]


class Step:
    """A parser position: the nodes parsed and the next chunk index."""
    nodes: list[Node]
    next: int


class Kw:
    """A statement's leading keyword and its remainder."""
    word: str
    rest: str


def blank(kind):
    """A node with no fields set."""
    return leaf(kind, "")


def leaf(kind, text):
    """A literal-text, expression-output or empty node."""
    return Node(kind=kind, text=text, name="", attr="", var="", var2="", cond="", iterable="", branches=[], body=[], else_body=[], params=[])


def step_node(s):
    """The single node a block parser produced."""
    return unwrap_or(listGet(s.nodes, 0), blank("text"))


def kw_in(stops, kw):
    """Whether keyword `kw` is in the stop list."""
    return fold(range(len(stops)), False, lambda acc, i: acc or unwrap_or(listGet(stops, i), "") == kw)


def index_or(s, needle):
    """The index of `needle` in `s`, or `length(s)` when absent."""
    return unwrap_or(indexOf(s, needle), length(s))


def split_kw(s):
    """The statement's leading keyword and the rest (trimmed)."""
    sp = index_or(s, " ")
    word = unwrap_or(substring(s, 0, sp), s)
    rest = unwrap_or(substring(s, sp + 1, length(s)), "")
    return Kw(word=word, rest=trim(rest))


def parse(chunks) -> Result[list[Node], str]:
    """Parse chunks into the root node list."""
    match parse_nodes(chunks, 0, len(chunks), [], []):
        case Success(value=value):
            return Success(value=value.nodes)
        case Error(message=message):
            return Error(message=message)


def parse_nodes(chunks, i, n, stops, acc) -> Result[Step, str]:
    """Parse nodes until a block keyword in `stops` (which is left unconsumed)."""
    if i >= n:
        return Success(value=Step(nodes=acc, next=i)) if len(stops) == 0 else Error(message="unexpected end of template, expected 'end of statement block'")
    else:
        chunk = unwrap_or(listGet(chunks, i), Chunk(kind="data", text=""))
        if chunk.kind == "data":
            return parse_nodes(chunks, i + 1, n, stops, listAppend(acc, leaf("text", chunk.text)))
        elif chunk.kind == "variable":
            return parse_nodes(chunks, i + 1, n, stops, listAppend(acc, leaf("output", chunk.text)))
        else:
            k = split_kw(chunk.text)
            if kw_in(stops, k.word):
                return Success(value=Step(nodes=acc, next=i))
            elif k.word == "if":
                return parse_block_then(chunks, n, stops, acc, parse_if(chunks, i, n, k.rest))
            elif k.word == "for":
                return parse_block_then(chunks, n, stops, acc, parse_for(chunks, i, n, k.rest))
            elif k.word == "set":
                return parse_nodes(chunks, i + 1, n, stops, listAppend(acc, set_node(k.rest)))
            elif k.word == "macro":
                return parse_block_then(chunks, n, stops, acc, parse_macro(chunks, i, n, k.rest))
            else:
                return Error(message="unknown statement `" + k.word + "`")


def parse_block_then(chunks, n, stops, acc, parsed) -> Result[Step, str]:
    """Append a parsed block and resume its enclosing node sequence."""
    match parsed:
        case Success(value=value):
            return parse_nodes(chunks, value.next, n, stops, listAppend(acc, step_node(value)))
        case Error(message=message):
            return Error(message=message)


def parse_if(chunks, i, n, cond) -> Result[Step, str]:
    """Parse an `if` block from its opening tag at `i`."""
    match parse_nodes(chunks, i + 1, n, ["elif", "else", "endif"], []):
        case Success(value=value):
            return parse_if_tail(chunks, value.next, n, [Branch(cond=cond, body=value.nodes)])
        case Error(message=message):
            return Error(message=message)


def parse_if_tail(chunks, j, n, branches) -> Result[Step, str]:
    """Collect `elif`/`else` arms until `endif`."""
    k = split_kw(unwrap_or(listGet(chunks, j), Chunk(kind="block", text="")).text)
    if k.word == "elif":
        match parse_nodes(chunks, j + 1, n, ["elif", "else", "endif"], []):
            case Success(value=value):
                return parse_if_tail(chunks, value.next, n, listAppend(branches, Branch(cond=k.rest, body=value.nodes)))
            case Error(message=message):
                return Error(message=message)
    elif k.word == "else":
        match parse_nodes(chunks, j + 1, n, ["endif"], []):
            case Success(value=value):
                return Success(value=Step(nodes=[if_node(listAppend(branches, Branch(cond="", body=value.nodes)))], next=value.next + 1))
            case Error(message=message):
                return Error(message=message)
    else:
        return Success(value=Step(nodes=[if_node(branches)], next=j + 1))


def if_node(branches):
    """An `if` node from its branches."""
    return Node(kind="if", text="", name="", attr="", var="", var2="", cond="", iterable="", branches=branches, body=[], else_body=[], params=[])


def parse_for(chunks, i, n, rest) -> Result[Step, str]:
    """Parse a `for` block from its opening tag at `i`."""
    head = for_header(rest)
    match parse_nodes(chunks, i + 1, n, ["else", "endfor"], []):
        case Success(value=value):
            k = split_kw(unwrap_or(listGet(chunks, value.next), Chunk(kind="block", text="")).text)
            if k.word == "else":
                return parse_for_else(chunks, value, head, n)
            else:
                return Success(value=for_node(head, value.nodes, [], value.next + 1))
        case Error(message=message):
            return Error(message=message)


def parse_for_else(chunks, first, head, n) -> Result[Step, str]:
    """The `{% else %}` arm of a for loop."""
    match parse_nodes(chunks, first.next + 1, n, ["endfor"], []):
        case Success(value=value):
            return Success(value=for_node(head, first.nodes, value.nodes, value.next + 1))
        case Error(message=message):
            return Error(message=message)


def for_header(rest):
    """Split `x in xs [if cond]` or `k, v in xs [if cond]` into its parts."""
    in_pos = index_or(rest, " in ")
    left = unwrap_or(substring(rest, 0, in_pos), rest)
    right = unwrap_or(substring(rest, in_pos + 4, length(rest)), "")
    if_pos = index_or(right, " if ")
    iterable = unwrap_or(substring(right, 0, if_pos), right)
    cond = unwrap_or(substring(right, if_pos + 4, length(right)), "")
    comma = index_or(left, ",")
    return Node(kind="for", text="", name="", attr="", var=for_var(left, comma), var2=for_var2(left, comma), cond=trim(cond), iterable=trim(iterable), branches=[], body=[], else_body=[], params=[])


def for_node(head, body, else_body, next):
    """A `for` node from a header, its bodies, and the next chunk index."""
    return Step(nodes=[Node(kind="for", text="", name="", attr="", var=head.var, var2=head.var2, cond=head.cond, iterable=head.iterable, branches=[], body=body, else_body=else_body, params=[])], next=next)


def for_var(left, comma):
    """A for header's first loop variable."""
    if comma == length(left):
        return trim(left)
    else:
        return trim(unwrap_or(substring(left, 0, comma), left))


def for_var2(left, comma):
    """A for header's second loop variable, or `""` when there is none."""
    if comma == length(left):
        return ""
    else:
        return trim(unwrap_or(substring(left, comma + 1, length(left)), ""))


def set_node(rest):
    """Build a `set` node from `x = expr` or `ns.attr = expr`."""
    eq = index_or(rest, "=")
    target = trim(unwrap_or(substring(rest, 0, eq), rest))
    value = trim(unwrap_or(substring(rest, eq + 1, length(rest)), ""))
    dot = index_or(target, ".")
    if dot == length(target):
        return Node(kind="set", text=value, name=target, attr="", var="", var2="", cond="", iterable="", branches=[], body=[], else_body=[], params=[])
    else:
        return Node(kind="set", text=value, name=unwrap_or(substring(target, 0, dot), target), attr=unwrap_or(substring(target, dot + 1, length(target)), ""), var="", var2="", cond="", iterable="", branches=[], body=[], else_body=[], params=[])


def parse_macro(chunks, i, n, rest) -> Result[Step, str]:
    """Parse a `{% macro name(a, b) %} ... {% endmacro %}`."""
    lparen = index_or(rest, "(")
    name = trim(unwrap_or(substring(rest, 0, lparen), rest))
    rparen = index_or(rest, ")")
    params = split_params(unwrap_or(substring(rest, lparen + 1, rparen), ""))
    match parse_nodes(chunks, i + 1, n, ["endmacro"], []):
        case Success(value=value):
            return Success(value=Step(nodes=[Node(kind="macro", text="", name=name, attr="", var="", var2="", cond="", iterable="", branches=[], body=value.nodes, else_body=[], params=params)], next=value.next + 1))
        case Error(message=message):
            return Error(message=message)


def split_params(text):
    """A macro's parameter list into names."""
    if trim(text) == "":
        return []
    else:
        parts = unwrap_or(split(text, ","), [])
        return toList(map(range(len(parts)), lambda k: trim(unwrap_or(listGet(parts, k), ""))))
