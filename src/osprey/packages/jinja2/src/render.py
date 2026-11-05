# osprey: flavor=python
# osprey: namespace=jinja2
# osprey: module=Render
"""The renderer: walks the node tree over a scope chain of frames, evaluating
expressions with `Expr` and threading the environment so namespace writes (and
plain `set` bindings) are visible where Jinja makes them visible.

`render` is the public entry: it lexes, parses and renders; a `raise_exception`
or a broken template is the `Error` payload, never a substituted value.
Implements [PORT-SCOPE] for @osprey/jinja2.
"""

from jinja2 import Value as V
from jinja2 import Lexer as L
from jinja2 import Parser as P
from jinja2 import Expr as E

__all__ = ["Environment", "chat_environment", "render", "expand_macro"]


class Environment:
    """The whitespace configuration a template is rendered under."""
    trim_blocks: bool
    lstrip_blocks: bool


class Out:
    """A render step: the text produced and the environment it ends with."""
    text: str
    env: V.Env


def chat_environment():
    """transformers' chat-template environment: trim_blocks=True, lstrip_blocks=True."""
    return Environment(trim_blocks=True, lstrip_blocks=True)


def render(env, template, context) -> Result[str, str]:
    """Render `template` over `context` (a dict of Values) under `env`'s
    whitespace rules. A template fault is the `Error` payload."""
    return render_internal(env, template, context)


def render_internal(env, template, context) -> Result[str, str]:
    """Lex, parse and render in one pass."""
    match L.scan(template, env.trim_blocks, env.lstrip_blocks):
        case Success(value=value):
            return parse_and_render(value, env, context)
        case Error(message=message):
            return Error(message=message)


def parse_and_render(chunks, env, context) -> Result[str, str]:
    """Parse chunks and render the resulting tree."""
    match P.parse(chunks):
        case Success(value=value):
            return render_result(value, V.Env(frames=[context], macros=[]))
        case Error(message=message):
            return Error(message=message)


def render_result(nodes, env) -> Result[str, str]:
    """Render a node tree, converting the first fault into `Error`."""
    match render_nodes(nodes, env):
        case Success(value=value):
            return Success(value=value.text)
        case Error(message=message):
            return Error(message=message)


def render_nodes(nodes, env) -> Result[Out, str]:
    """Render every node, threading the environment."""
    return render_nodes_from(nodes, 0, env, "")


def render_nodes_from(nodes, i, env, acc) -> Result[Out, str]:
    """Render nodes from `i`, appending to `acc`."""
    if i >= len(nodes):
        return Success(value=Out(text=acc, env=env))
    else:
        match render_node(unwrap_or(listGet(nodes, i), P.blank("text")), env):
            case Success(value=value):
                return render_nodes_from(nodes, i + 1, value.env, acc + value.text)
            case Error(message=message):
                return Error(message=message)


def render_node(node, env) -> Result[Out, str]:
    """Render one node."""
    match node.kind:
        case "text":
            return Success(value=Out(text=node.text, env=env))
        case "output":
            return render_output(node, env)
        case "set":
            return render_set(node, env)
        case "if":
            return render_branches(node.branches, 0, env)
        case "for":
            return render_for(node, env)
        case "macro":
            return Success(value=Out(text="", env=V.bind_macro(env, node)))
        case _:
            return Error(message="unknown node kind `" + node.kind + "`")


def eval_ok(s, env) -> Result[V.Value, str]:
    """Evaluate expression `s`, surfacing an error Value as `Error`."""
    v = E.eval_expr(s, env)
    if V.is_error(v):
        return Error(message=v.sval)
    else:
        return Success(value=v)


def render_output(node, env) -> Result[Out, str]:
    """A `{{ ... }}` node."""
    match eval_ok(node.text, env):
        case Success(value=value):
            return Success(value=Out(text=V.concat_text(value), env=env))
        case Error(message=message):
            return Error(message=message)


def render_set(node, env) -> Result[Out, str]:
    """A `{% set %}`: bind a value, or write a namespace attribute."""
    match eval_ok(node.text, env):
        case Success(value=value):
            return Success(value=Out(text="", env=set_binding(env, node, value)))
        case Error(message=message):
            return Error(message=message)


def set_binding(env, node, v):
    """The environment after a `set` (plain bind or namespace attribute write)."""
    if node.attr == "":
        return V.bind(env, node.name, v)
    else:
        base = V.lookup(env, node.name)
        if base.kind == "dict":
            return V.bind_found(env, node.name, V.dict_set(base, node.attr, v))
        else:
            return V.bind_found(env, node.name, v)


def render_branches(branches, i, env) -> Result[Out, str]:
    """An `if`/`elif`/`else` chain: render the first true branch."""
    if i >= len(branches):
        return Success(value=Out(text="", env=env))
    else:
        b = unwrap_or(listGet(branches, i), P.Branch(cond="", body=[]))
        if b.cond == "":
            return render_nodes(b.body, env)
        else:
            return render_branch_cond(b, branches, i, env)


def render_branch_cond(b, branches, i, env) -> Result[Out, str]:
    """Evaluate a branch condition and render its body when true."""
    match eval_ok(b.cond, env):
        case Success(value=value):
            if V.truthy(value):
                return render_nodes(b.body, env)
            else:
                return render_branches(branches, i + 1, env)
        case Error(message=message):
            return Error(message=message)


def render_for(node, env) -> Result[Out, str]:
    """A `{% for %}`: iterate with a fresh loop frame per item, threading
    namespace writes back to the outer frames."""
    match eval_ok(node.iterable, env):
        case Success(value=value):
            elems = for_elements(value)
            if len(elems) == 0:
                return render_nodes(node.else_body, env)
            else:
                return for_loop(node, elems, 0, env, "")
        case Error(message=message):
            return Error(message=message)


def for_elements(v):
    """The values a `for` iterates: list elements, dict keys, string characters."""
    match v.kind:
        case "list":
            return v.elems
        case "dict":
            return v.keys
        case "string":
            return V.char_values(v.sval)
        case _:
            return []


def for_loop(node, elems, i, env, acc) -> Result[Out, str]:
    """Iterate the body, threading the outer environment across iterations."""
    if i >= len(elems):
        return Success(value=Out(text=acc, env=env))
    else:
        loop_env = push_loop(env, node, elems, i)
        if node.cond != "":
            return for_loop_cond(node, elems, i, env, acc, loop_env)
        else:
            match render_nodes(node.body, loop_env):
                case Success(value=value):
                    return for_loop(node, elems, i + 1, pop_frame(value.env), acc + value.text)
                case Error(message=message):
                    return Error(message=message)


def for_loop_cond(node, elems, i, env, acc, loop_env) -> Result[Out, str]:
    """The `{% for x in xs if cond %}` filter: skip the body when false."""
    match eval_ok(node.cond, loop_env):
        case Success(value=value):
            if V.truthy(value):
                return for_loop_body(node, elems, i, env, acc, loop_env)
            else:
                return for_loop(node, elems, i + 1, env, acc)
        case Error(message=message):
            return Error(message=message)


def for_loop_body(node, elems, i, env, acc, loop_env) -> Result[Out, str]:
    """Render one loop body and continue."""
    match render_nodes(node.body, loop_env):
        case Success(value=value):
            return for_loop(node, elems, i + 1, pop_frame(value.env), acc + value.text)
        case Error(message=message):
            return Error(message=message)


def push_loop(env, node, elems, i):
    """Bind the loop variable(s) and the `loop` object in a new child frame."""
    frame0: dict[str, V.Value] = {}
    frame = bind_var(frame0, node, unwrap_or(listGet(elems, i), V.none()))
    return V.env_with(env, listAppend(env.frames, mapSet(frame, "loop", loop_value(elems, i))))


def bind_var(frame, node, item):
    """Bind the loop variable(s) of `node` to `item` in `frame`."""
    if node.var2 == "":
        return mapSet(frame, node.var, item)
    else:
        f = mapSet(frame, node.var, unwrap_or(listGet(item.elems, 0), V.none()))
        return mapSet(f, node.var2, unwrap_or(listGet(item.vals, 0), V.none()))


def loop_value(elems, i):
    """The `loop` object: index, length and the neighbouring items."""
    n = len(elems)
    return V.dict_(["index", "index0", "first", "last", "length", "previtem", "nextitem"],
                   [V.int_(i + 1), V.int_(i), V.bool_(i == 0), V.bool_(i == n - 1), V.int_(n),
                    neighbour(elems, i - 1, "previtem"), neighbour(elems, i + 1, "nextitem")])


def neighbour(elems, i, name):
    """The loop item at `i`, or undefined past either end."""
    if i < 0 or i >= len(elems):
        return V.undefined(name)
    else:
        return unwrap_or(listGet(elems, i), V.undefined(name))


def pop_frame(env):
    """The environment without its innermost frame."""
    return V.env_with(env, drop_last(env.frames))


def drop_last(frames):
    """`frames` without its last element."""
    return toList(map(range(len(frames) - 1), lambda k: unwrap_or(listGet(frames, k), {})))


def expand_macro(caller, name, args, kwargs):
    """Expand the macro declared as `name` for one call in `caller`'s scope. The
    value is text, or an error value carrying the fault ([PORT-SCOPE])."""
    m = V.find_macro(caller, len(caller.macros) - 1, name)
    if m.node.name != name:
        return V.halt("no macro named '" + name + "'")
    else:
        return macro_value(m, caller, args, kwargs)


def macro_value(m, caller, args, kwargs):
    """Render one call of macro `m`; a fault is an error value."""
    match bind_args(m.node, caller, args, kwargs, 0, {}):
        case Success(value=frame):
            return macro_text(m, caller, frame)
        case Error(message=message):
            return V.halt(message)


def macro_text(m, caller, frame):
    """The rendered body of a macro call whose arguments are bound in `frame`."""
    match render_nodes(m.node.body, macro_env(m, caller, frame)):
        case Success(value=value):
            return V.text(value.text)
        case Error(message=message):
            return V.halt(message)


def macro_env(m, caller, frame):
    """The scope a macro body runs in: the call's root frame, the frames the macro
    was declared in, then its parameters. Jinja resolves a template global late and
    a loop variable of the call site never, which this frame order reproduces."""
    return V.Env(frames=listConcat([unwrap_or(listGet(caller.frames, 0), {})], listAppend(drop_first(m.frames), frame)), macros=caller.macros)


def drop_first(xs):
    """`xs` without its first element."""
    return toList(map(range(len(xs) - 1), lambda k: unwrap_or(listGet(xs, k + 1), {})))


def bind_args(node, caller, args, kwargs, i, frame):
    """Bind positional argument `i`, then the keywords, then the defaults."""
    if i >= len(args):
        return bind_keywords(node, caller, kwargs, 0, frame)
    elif i >= len(node.params):
        return Error(message="macro '" + node.name + "' takes not more than " + toString(len(node.params)) + " argument(s)")
    else:
        name = unwrap_or(listGet(node.params, i), "")
        return bind_args(node, caller, args, kwargs, i + 1, mapSet(frame, name, unwrap_or(listGet(args, i), V.none())))


def bind_keywords(node, caller, kwargs, i, frame):
    """Bind keyword argument `i`, rejecting one the macro does not declare or binds twice."""
    if i >= len(kwargs):
        return bind_defaults(node, caller, 0, frame)
    else:
        kw = unwrap_or(listGet(kwargs, i), V.KwVal(name="", value=V.none()))
        return bind_keyword(node, caller, kwargs, i, frame, kw)


def bind_keyword(node, caller, kwargs, i, frame, kw):
    """One keyword argument against the declared parameters."""
    if not is_param(node, kw.name):
        return Error(message="macro '" + node.name + "' takes no keyword argument '" + kw.name + "'")
    elif mapContains(frame, kw.name):
        return Error(message="macro '" + node.name + "' got multiple values for argument '" + kw.name + "'")
    else:
        return bind_keywords(node, caller, kwargs, i + 1, mapSet(frame, kw.name, kw.value))


def is_param(node, name):
    """Whether `name` is one of the macro's parameters."""
    return fold(range(len(node.params)), False, lambda acc, i: acc or unwrap_or(listGet(node.params, i), "") == name)


def bind_defaults(node, caller, i, frame):
    """Give every unbound parameter its default, or leave it undefined as Jinja does."""
    if i >= len(node.params):
        return Success(value=frame)
    else:
        name = unwrap_or(listGet(node.params, i), "")
        if mapContains(frame, name):
            return bind_defaults(node, caller, i + 1, frame)
        else:
            match default_of(node, caller, i):
                case Success(value=value):
                    return bind_defaults(node, caller, i + 1, mapSet(frame, name, value))
                case Error(message=message):
                    return Error(message=message)


def default_of(node, caller, i):
    """Parameter `i`'s default evaluated in the call's scope, or undefined when it has none."""
    expr = unwrap_or(listGet(node.defaults, i), "")
    if expr == "":
        return Success(value=V.undefined(unwrap_or(listGet(node.params, i), "")))
    else:
        return eval_ok(expr, caller)
