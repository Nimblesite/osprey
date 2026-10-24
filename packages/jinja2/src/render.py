# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
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

__all__ = ["Environment", "chat_environment", "render"]


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
            return render_result(value, V.Env(frames=[context]))
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
        case _:
            return Error(message="{% macro %} is not supported by @osprey/jinja2")


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
            return toList(map(range(len(v.keys)), lambda i: V.text(unwrap_or(listGet(v.keys, i), ""))))
        case "string":
            return V.char_values(v.sval)
        case _:
            return []


def for_loop(node, elems, i, env, acc) -> Result[Out, str]:
    """Iterate the body, threading the outer environment across iterations."""
    if i >= len(elems):
        return Success(value=Out(text=acc, env=env))
    else:
        item = unwrap_or(listGet(elems, i), V.none())
        loop_env = push_loop(env, node, item, i, len(elems))
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


def push_loop(env, node, item, i, n):
    """Bind the loop variable(s) and the `loop` object in a new child frame."""
    frame0: dict[str, V.Value] = {}
    frame = bind_var(frame0, node, item)
    return V.Env(frames=listAppend(env.frames, mapSet(frame, "loop", loop_value(i, n))))


def bind_var(frame, node, item):
    """Bind the loop variable(s) of `node` to `item` in `frame`."""
    if node.var2 == "":
        return mapSet(frame, node.var, item)
    else:
        f = mapSet(frame, node.var, V.text(item.sval))
        return mapSet(f, node.var2, unwrap_or(listGet(item.vals, 0), V.none()))


def loop_value(i, n):
    """The `loop` object: index, index0, first, last, length."""
    return V.dict_(["index", "index0", "first", "last", "length"], [V.int_(i + 1), V.int_(i), V.bool_(i == 0), V.bool_(i == n - 1), V.int_(n)])


def pop_frame(env):
    """The environment without its innermost frame."""
    return V.Env(frames=drop_last(env.frames))


def drop_last(frames):
    """`frames` without its last element."""
    return toList(map(range(len(frames) - 1), lambda k: unwrap_or(listGet(frames, k), {})))
