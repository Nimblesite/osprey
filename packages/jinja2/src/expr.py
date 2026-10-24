# osprey: flavor=python
# osprey: namespace=jinja2
# osprey: module=Expr
"""The expression engine: tokenizes a Jinja expression and evaluates it over an
`Env`, with Jinja 3.1.6 operator precedence, short-circuiting `and`/`or` that
return their operands, `is` tests, `|` filters and string/dict method calls.

Filters, tests and methods live in `Filters`; this module parses and dispatches.
"""

from jinja2.Value import halt
from jinja2 import Value as V
from jinja2 import Filters as F

__all__ = ["eval_expr"]


class Tok:
    """One lexed expression token."""
    kind: str
    sval: str
    ival: int
    fval: float


class P:
    """An evaluation position: the value and the next token index."""
    value: V.Value
    next: int


class T:
    """A scan step: the token and the next byte index."""
    tok: Tok
    next: int


class Args:
    """A parsed argument list."""
    args: list[V.Value]
    kwargs: list[V.KwVal]
    next: int
    error: str


def end_tok():
    """The end-of-input token."""
    return Tok(kind="end", sval="", ival=0, fval=0.0)


def tok(toks, i):
    """Token `i`, or end-of-input past the list."""
    return unwrap_or(listGet(toks, i), end_tok())


def is_op(t, s):
    """Whether `t` is operator `s`."""
    return t.kind == "op" and t.sval == s


def is_name(t, s):
    """Whether `t` is the name `s`."""
    return t.kind == "name" and t.sval == s


def byte(s, i):
    """The byte at `i`, or 0 past the end."""
    return unwrap_or(byteAt(s, i), 0)


def is_digit(b):
    """Whether byte `b` is an ASCII digit."""
    return b >= 48 and b <= 57


def is_ident_start(b):
    """Whether byte `b` can start an identifier."""
    return (b >= 97 and b <= 122) or (b >= 65 and b <= 90) or b == 95


def is_ident_char(b):
    """Whether byte `b` can continue an identifier."""
    return is_ident_start(b) or is_digit(b)


def is_ws(b):
    """Whether byte `b` is expression whitespace."""
    return b == 32 or b == 9 or b == 10 or b == 13


def tokenize(s):
    """Tokenize an expression string."""
    return tokenize_from(s, 0, [])


def tokenize_from(s, i, acc):
    """The tokenizing scan."""
    if i >= length(s):
        return listAppend(acc, end_tok())
    else:
        b = byte(s, i)
        if is_digit(b):
            t = scan_number(s, i)
            return tokenize_from(s, t.next, listAppend(acc, t.tok))
        elif is_ident_start(b):
            t = scan_ident(s, i)
            return tokenize_from(s, t.next, listAppend(acc, t.tok))
        elif b == 34 or b == 39:
            t = scan_string(s, i, b)
            return tokenize_from(s, t.next, listAppend(acc, t.tok))
        elif is_ws(b):
            return tokenize_from(s, i + 1, acc)
        else:
            t = scan_op(s, i)
            return tokenize_from(s, t.next, listAppend(acc, t.tok))


def digits(s, j):
    """Consume ASCII digits from `j`."""
    if is_digit(byte(s, j)):
        return digits(s, j + 1)
    else:
        return j


def exponent(s, j):
    """Consume an `e`/`E` exponent from `j` (at the `e`)."""
    return digits(s, exp_mantissa(s, j + 1))


def exp_mantissa(s, k):
    """The first mantissa digit after an exponent sign, if any."""
    if byte(s, k) == 43 or byte(s, k) == 45:
        return k + 1
    else:
        return k


class Num:
    """A scanned number: its end byte and whether it was a float."""
    end: int
    is_f: bool


def scan_num(s, i):
    """Scan the number starting at `i`, reporting its end and floatness."""
    j = digits(s, i)
    if byte(s, j) == 46 and is_digit(byte(s, j + 1)):
        k = digits(s, j + 1)
        return exponent_or(s, k, True)
    elif byte(s, j) == 101 or byte(s, j) == 69:
        return Num(end=exponent(s, j), is_f=True)
    else:
        return Num(end=j, is_f=False)


def exponent_or(s, k, is_f):
    """Whether an exponent follows at `k`, extending the number."""
    if byte(s, k) == 101 or byte(s, k) == 69:
        return Num(end=exponent(s, k), is_f=True)
    else:
        return Num(end=k, is_f=is_f)


def scan_number(s, i):
    """A number token at `i`."""
    n = scan_num(s, i)
    text = unwrap_or(substring(s, i, n.end), "0")
    if n.is_f:
        return T(tok=Tok(kind="float", sval="", ival=0, fval=unwrap_or(parseFloat(text), 0.0)), next=n.end)
    else:
        return T(tok=Tok(kind="int", sval="", ival=unwrap_or(parseInt(text), 0), fval=0.0), next=n.end)


def scan_ident(s, i):
    """An identifier token at `i`."""
    j = ident_end(s, i)
    return T(tok=Tok(kind="name", sval=unwrap_or(substring(s, i, j), ""), ival=0, fval=0.0), next=j)


def ident_end(s, j):
    """The byte after the identifier starting at `j`."""
    if is_ident_char(byte(s, j)):
        return ident_end(s, j + 1)
    else:
        return j


def scan_string(s, i, quote):
    """A string literal at `i` (the opening quote), decoded."""
    return T(tok=Tok(kind="string", sval=string_body(s, i + 1, quote, ""), ival=0, fval=0.0), next=string_end(s, i + 1, quote))


def string_body(s, j, quote, acc):
    """Decode a string body from `j`, honouring escapes."""
    if j >= length(s):
        return acc
    else:
        c = byte(s, j)
        if c == 92:
            return escape_body(s, j + 1, quote, acc)
        elif c == quote:
            return acc
        else:
            cp = unwrap_or(codePointAt(s, j), c)
            return string_body(s, j + unwrap_or(codePointWidth(cp), 1), quote, acc + unwrap_or(fromCodePoint(cp), ""))


def string_end(s, j, quote):
    """The byte after a string body's closing quote."""
    if j >= length(s):
        return length(s)
    else:
        c = byte(s, j)
        if c == 92:
            return string_end(s, j + 2, quote)
        elif c == quote:
            return j + 1
        else:
            return string_end(s, j + 1, quote)


def escape_body(s, j, quote, acc):
    """Decode one backslash escape at `j`."""
    e = byte(s, j)
    if e == 110:
        return string_body(s, j + 1, quote, acc + "\n")
    elif e == 114:
        return string_body(s, j + 1, quote, acc + "\r")
    elif e == 116:
        return string_body(s, j + 1, quote, acc + "\t")
    elif e == 98:
        return string_body(s, j + 1, quote, acc + "\b")
    elif e == 102:
        return string_body(s, j + 1, quote, acc + "\f")
    elif e == 118:
        return string_body(s, j + 1, quote, acc + "\x0b")
    elif e == 92:
        return string_body(s, j + 1, quote, acc + "\\")
    elif e == 39:
        return string_body(s, j + 1, quote, acc + "'")
    elif e == 34:
        return string_body(s, j + 1, quote, acc + "\"")
    else:
        return string_body(s, j + 1, quote, acc + unwrap_or(substring(s, j, j + 1), ""))


def scan_op(s, i):
    """An operator token at `i` (longest match first)."""
    two = unwrap_or(substring(s, i, i + 2), "")
    if two == "//" or two == "==" or two == "!=" or two == ">=" or two == "<=" or two == "**":
        return T(tok=Tok(kind="op", sval=two, ival=0, fval=0.0), next=i + 2)
    else:
        one = unwrap_or(substring(s, i, i + 1), "")
        return T(tok=Tok(kind="op", sval=one, ival=0, fval=0.0), next=i + 1)


def eval_expr(s, env):
    """Evaluate the whole expression string over `env`."""
    toks = tokenize(s)
    p = eval_condexpr(toks, 0, env)
    if tok(toks, p.next).kind != "end":
        return halt("unexpected token `" + tok(toks, p.next).sval + "` in expression")

    else:
        return p.value


def eval_condexpr(toks, i, env):
    """`a if c else b` (loosest precedence)."""
    p1 = eval_or(toks, i, env)
    if is_name(tok(toks, p1.next), "if"):
        test = eval_or(toks, p1.next + 1, env)
        if is_name(tok(toks, test.next), "else"):
            other = eval_condexpr(toks, test.next + 1, env)
            return P(value=p1.value if V.truthy(test.value) else other.value, next=other.next)
        else:
            return P(value=halt("expected 'else' in conditional expression"), next=test.next)

    else:
        return p1


def eval_or(toks, i, env):
    """`a or b`: the first truthy operand, or the last."""
    p = eval_and(toks, i, env)
    if is_name(tok(toks, p.next), "or"):
        q = eval_or(toks, p.next + 1, env)
        return P(value=p.value if V.truthy(p.value) else q.value, next=q.next)
    else:
        return p


def eval_and(toks, i, env):
    """`a and b`: the first falsy operand, or the last."""
    p = eval_not(toks, i, env)
    if is_name(tok(toks, p.next), "and"):
        q = eval_and(toks, p.next + 1, env)
        return P(value=q.value if V.truthy(p.value) else p.value, next=q.next)
    else:
        return p


def eval_not(toks, i, env):
    """`not x`."""
    if is_name(tok(toks, i), "not"):
        p = eval_not(toks, i + 1, env)
        return P(value=V.bool_(not V.truthy(p.value)), next=p.next)
    else:
        return eval_compare(toks, i, env)


def eval_compare(toks, i, env):
    """`a == b`, `a < b`, `a in b`, `a not in b`, left-to-right."""
    p = eval_math1(toks, i, env)
    return compare_chain(toks, p.next, env, p.value)


def compare_chain(toks, i, env, left):
    """Collect comparison operands."""
    t = tok(toks, i)
    if is_op(t, "==") or is_op(t, "!=") or is_op(t, "<") or is_op(t, "<=") or is_op(t, ">") or is_op(t, ">=") or is_name(t, "in"):
        q = eval_math1(toks, i + 1, env)
        return compare_chain(toks, q.next, env, compare_op(t, left, q.value))
    elif is_name(t, "not") and is_name(tok(toks, i + 1), "in"):
        q = eval_math1(toks, i + 2, env)
        return compare_chain(toks, q.next, env, V.bool_(not V.contains(q.value, left)))
    else:
        return P(value=left, next=i)


def compare_op(t, a, b):
    """One comparison operator."""
    if is_op(t, "=="):
        return V.bool_(V.equal(a, b))
    elif is_op(t, "!="):
        return V.bool_(not V.equal(a, b))
    elif is_op(t, "<"):
        return V.bool_(V.less(a, b))
    elif is_op(t, "<="):
        return V.bool_(V.less(a, b) or V.equal(a, b))
    elif is_op(t, ">"):
        return V.bool_(V.less(b, a))
    elif is_op(t, ">="):
        return V.bool_(V.less(b, a) or V.equal(a, b))
    else:
        return V.bool_(V.contains(b, a))


def eval_math1(toks, i, env):
    """`a + b`, `a - b`."""
    p = eval_concat(toks, i, env)
    return math1_chain(toks, p.next, env, p.value)


def math1_chain(toks, i, env, left):
    """Collect `+`/`-` operands."""
    t = tok(toks, i)
    if is_op(t, "+"):
        q = eval_concat(toks, i + 1, env)
        return math1_chain(toks, q.next, env, add(left, q.value))
    elif is_op(t, "-"):
        q = eval_concat(toks, i + 1, env)
        return math1_chain(toks, q.next, env, sub(left, q.value))
    else:
        return P(value=left, next=i)


def eval_concat(toks, i, env):
    """`a ~ b`: string concatenation with coercion."""
    p = eval_math2(toks, i, env)
    return concat_chain(toks, p.next, env, V.concat_text(p.value))


def concat_chain(toks, i, env, acc):
    """Collect `~` operands into one string."""
    if is_op(tok(toks, i), "~"):
        q = eval_math2(toks, i + 1, env)
        return concat_chain(toks, q.next, env, acc + V.concat_text(q.value))
    else:
        return P(value=V.text(acc), next=i)


def eval_math2(toks, i, env):
    """`a * b`, `a / b`, `a // b`, `a % b`."""
    p = eval_unary(toks, i, env)
    return math2_chain(toks, p.next, env, p.value)


def math2_chain(toks, i, env, left):
    """Collect `*`/`/`/`//`/`%` operands."""
    t = tok(toks, i)
    if is_op(t, "*"):
        q = eval_unary(toks, i + 1, env)
        return math2_chain(toks, q.next, env, mul(left, q.value))
    elif is_op(t, "/"):
        q = eval_unary(toks, i + 1, env)
        return math2_chain(toks, q.next, env, truediv(left, q.value))
    elif is_op(t, "//"):
        q = eval_unary(toks, i + 1, env)
        return math2_chain(toks, q.next, env, floordiv(left, q.value))
    elif is_op(t, "%"):
        q = eval_unary(toks, i + 1, env)
        return math2_chain(toks, q.next, env, mod(left, q.value))
    else:
        return P(value=left, next=i)


def eval_unary(toks, i, env):
    """Unary `-`/`+`, then postfix and filter/test."""
    t = tok(toks, i)
    if is_op(t, "-"):
        p = eval_unary_no_filter(toks, i + 1, env)
        v = neg(p.value)
        p2 = eval_postfix(toks, p.next, env, v)
        return eval_filter(toks, p2.next, env, p2.value)
    elif is_op(t, "+"):
        p = eval_unary_no_filter(toks, i + 1, env)
        p2 = eval_postfix(toks, p.next, env, p.value)
        return eval_filter(toks, p2.next, env, p2.value)
    else:
        p = eval_primary(toks, i, env)
        p2 = eval_postfix(toks, p.next, env, p.value)
        return eval_filter(toks, p2.next, env, p2.value)


def eval_unary_no_filter(toks, i, env):
    """Unary minus/plus without the filter level (Jinja's `Neg` operand)."""
    t = tok(toks, i)
    if is_op(t, "-"):
        p = eval_unary_no_filter(toks, i + 1, env)
        return P(value=neg(p.value), next=p.next)
    elif is_op(t, "+"):
        return eval_unary_no_filter(toks, i + 1, env)
    else:
        p = eval_primary(toks, i, env)
        return eval_postfix(toks, p.next, env, p.value)


def eval_primary(toks, i, env):
    """A literal, name, parenthesised expression, list or dict."""
    t = tok(toks, i)
    if t.kind == "int":
        return P(value=V.int_(t.ival), next=i + 1)
    elif t.kind == "float":
        return P(value=V.float_(t.fval), next=i + 1)
    elif t.kind == "string":
        return P(value=V.text(t.sval), next=i + 1)
    elif t.kind == "name":
        if t.sval == "true" or t.sval == "True":
            return P(value=V.bool_(True), next=i + 1)
        elif t.sval == "false" or t.sval == "False":
            return P(value=V.bool_(False), next=i + 1)
        elif t.sval == "none" or t.sval == "None":
            return P(value=V.none(), next=i + 1)
        elif is_global(t.sval):
            return P(value=V.func(t.sval), next=i + 1)
        else:
            return P(value=V.lookup(env, t.sval), next=i + 1)
    elif is_op(t, "("):
        return eval_paren(toks, i, env)
    elif is_op(t, "["):
        return eval_list(toks, i, env)
    elif is_op(t, "{"):
        return eval_dict(toks, i, env)
    else:
        return P(value=halt("unexpected token `" + t.sval + "` in expression"), next=i + 1)



def is_global(name):
    """Whether `name` is a built-in function, not a context lookup."""
    return name == "raise_exception" or name == "namespace" or name == "dict" or name == "range" or name == "strftime_now"


def eval_paren(toks, i, env):
    """A parenthesised expression."""
    p = eval_condexpr(toks, i + 1, env)
    if is_op(tok(toks, p.next), ")"):
        return P(value=p.value, next=p.next + 1)
    else:
        return P(value=halt("expected ')' in expression"), next=p.next)



def eval_list(toks, i, env):
    """A `[...]` list literal."""
    if is_op(tok(toks, i + 1), "]"):
        return P(value=V.list_([]), next=i + 2)
    else:
        p = eval_condexpr(toks, i + 1, env)
        return list_tail(toks, p.next, env, [p.value])


def list_tail(toks, i, env, acc):
    """Collect list elements after the first."""
    t = tok(toks, i)
    if is_op(t, ","):
        p = eval_condexpr(toks, i + 1, env)
        return list_tail(toks, p.next, env, listAppend(acc, p.value))
    elif is_op(t, "]"):
        return P(value=V.list_(acc), next=i + 1)
    else:
        return P(value=halt("expected ',' or ']' in list"), next=i)



def eval_dict(toks, i, env):
    """A `{...}` dict literal."""
    if is_op(tok(toks, i + 1), "}"):
        return P(value=V.dict_([], []), next=i + 2)
    else:
        pk = eval_condexpr(toks, i + 1, env)
        if is_op(tok(toks, pk.next), ":"):
            pv = eval_condexpr(toks, pk.next + 1, env)
            return dict_tail(toks, pv.next, env, [key_text(pk.value)], [pv.value])
        else:
            return P(value=halt("expected ':' in dict literal"), next=pk.next)


def dict_tail(toks, i, env, keys, vals):
    """Collect dict pairs after the first."""
    t = tok(toks, i)
    if is_op(t, ","):
        if is_op(tok(toks, i + 1), "}"):
            return P(value=V.dict_(keys, vals), next=i + 2)
        else:
            pk = eval_condexpr(toks, i + 1, env)
            if is_op(tok(toks, pk.next), ":"):
                pv = eval_condexpr(toks, pk.next + 1, env)
                return dict_tail(toks, pv.next, env, listAppend(keys, key_text(pk.value)), listAppend(vals, pv.value))
            else:
                return P(value=halt("expected ':' in dict literal"), next=pk.next)
    elif is_op(t, "}"):
        return P(value=V.dict_(keys, vals), next=i + 1)
    else:
        return P(value=halt("expected ',' or '}' in dict"), next=i)


def key_text(v):
    """A dict key as text (string keys as themselves)."""
    if v.kind == V.K_STRING:
        return v.sval
    else:
        return V.concat_text(v)


def eval_postfix(toks, i, env, obj):
    """Attribute, subscript and call suffixes."""
    t = tok(toks, i)
    if is_op(t, "."):
        nxt = tok(toks, i + 1)
        after = tok(toks, i + 2)
        if nxt.kind == "name" and is_op(after, "("):
            p = parse_call_args(toks, i + 3, env)
            if p.error != "":
                return P(value=halt(p.error), next=p.next)
            else:
                return eval_postfix(toks, p.next, env, F.call_method(obj, nxt.sval, p.args))
        elif nxt.kind == "name":
            return eval_postfix(toks, i + 2, env, V.attr_get(obj, nxt.sval))
        elif nxt.kind == "int":
            return eval_postfix(toks, i + 2, env, V.dict_get(obj, V.int_(nxt.ival)))
        else:
            return P(value=halt("expected attribute name after '.'"), next=i + 1)

    elif is_op(t, "["):
        return eval_subscript(toks, i + 1, env, obj)
    elif is_op(t, "("):
        p = parse_call_args(toks, i + 1, env)
        if p.error != "":
            return P(value=halt(p.error), next=p.next)
        else:
            return eval_postfix(toks, p.next, env, call_value(obj, p.args, p.kwargs))
    else:
        return P(value=obj, next=i)


def eval_subscript(toks, i, env, obj):
    """`obj[i]` or `obj[a:b:c]`; `i` is just after the `[`."""
    if is_op(tok(toks, i), ":"):
        sl = V.Slice(start=0, stop=0, step=1, has_start=False, has_stop=False, has_step=False)
        return slice_rest(toks, i + 1, env, obj, sl)
    else:
        p = eval_condexpr(toks, i, env)
        if is_op(tok(toks, p.next), ":"):
            sl = V.Slice(start=int_of(p.value), stop=0, step=1, has_start=True, has_stop=False, has_step=False)
            return slice_rest(toks, p.next + 1, env, obj, sl)
        elif is_op(tok(toks, p.next), "]"):
            return P(value=V.dict_get(obj, p.value), next=p.next + 1)
        else:
            return P(value=halt("expected ']' or ':' after subscript"), next=p.next)



def slice_rest(toks, i, env, obj, sl):
    """After the first `:` of a slice."""
    t = tok(toks, i)
    if is_op(t, ":"):
        return slice_step(toks, i + 1, env, obj, sl)
    elif is_op(t, "]"):
        return P(value=apply_slice(obj, sl), next=i + 1)
    else:
        p = eval_condexpr(toks, i, env)
        sl2 = V.Slice(start=sl.start, stop=int_of(p.value), step=sl.step, has_start=sl.has_start, has_stop=True, has_step=sl.has_step)
        if is_op(tok(toks, p.next), ":"):
            return slice_step(toks, p.next + 1, env, obj, sl2)
        elif is_op(tok(toks, p.next), "]"):
            return P(value=apply_slice(obj, sl2), next=p.next + 1)
        else:
            return P(value=halt("expected ':' or ']' in slice"), next=p.next)



def slice_step(toks, i, env, obj, sl):
    """After the second `:` of a slice."""
    if is_op(tok(toks, i), "]"):
        return P(value=apply_slice(obj, sl), next=i + 1)
    else:
        p = eval_condexpr(toks, i, env)
        sl2 = V.Slice(start=sl.start, stop=sl.stop, step=int_of(p.value), has_start=sl.has_start, has_stop=sl.has_stop, has_step=True)
        if is_op(tok(toks, p.next), "]"):
            return P(value=apply_slice(obj, sl2), next=p.next + 1)
        else:
            return P(value=halt("expected ']' after slice step"), next=p.next)



def int_of(v):
    """A slice bound as an int (0 for non-integers)."""
    if v.kind == V.K_INT:
        return v.ival
    else:
        return 0


def apply_slice(obj, sl):
    """Slice a list or string."""
    if obj.kind == V.K_LIST:
        return V.list_(V.slice_list(obj.elems, sl))
    elif obj.kind == V.K_STRING:
        return V.text(V.slice_str(obj.sval, sl))
    else:
        return V.none()


def parse_call_args(toks, i, env):
    """Arguments inside `(...)`; `i` is just after the `(`."""
    if is_op(tok(toks, i), ")"):
        return Args(args=[], kwargs=[], next=i + 1, error="")
    else:
        return call_args_loop(toks, i, env, [], [], False)


def call_args_loop(toks, i, env, args, kwargs, need_comma):
    """Collect call arguments until `)`."""
    t = tok(toks, i)
    if is_op(t, ")"):
        return Args(args=args, kwargs=kwargs, next=i + 1, error="")
    elif need_comma and is_op(t, ","):
        return call_args_loop(toks, i + 1, env, args, kwargs, False)
    elif need_comma:
        return Args(args=[], kwargs=[], next=i, error=halt("expected ',' in argument list").sval)

    elif t.kind == "name" and is_op(tok(toks, i + 1), "="):
        p = eval_condexpr(toks, i + 2, env)
        return call_args_loop(toks, p.next, env, args, listAppend(kwargs, V.KwVal(name=t.sval, value=p.value)), True)
    else:
        p = eval_condexpr(toks, i, env)
        return call_args_loop(toks, p.next, env, listAppend(args, p.value), kwargs, True)


def eval_filter(toks, i, env, obj):
    """The postfix loop for `| filter`, `is test`, and calls."""
    t = tok(toks, i)
    if is_op(t, "|"):
        nxt = tok(toks, i + 1)
        if nxt.kind == "name":
            nm = dotted_name(toks, i + 2, nxt.sval)
            a = parse_filter_args(toks, nm.next, env)
            if a.error != "":
                return P(value=halt(a.error), next=a.next)
            else:
                return eval_filter(toks, a.next, env, F.apply_filter(nm.name, obj, a.args, a.kwargs))
        else:
            return P(value=halt("expected filter name after '|'"), next=i + 1)

    elif is_name(t, "is"):
        return eval_test(toks, i + 1, env, obj)
    elif is_op(t, "("):
        p = parse_call_args(toks, i + 1, env)
        if p.error != "":
            return P(value=halt(p.error), next=p.next)
        else:
            return eval_filter(toks, p.next, env, call_value(obj, p.args, p.kwargs))
    else:
        return P(value=obj, next=i)


def parse_filter_args(toks, j, env):
    """Filter arguments: `(...)` or none."""
    if is_op(tok(toks, j), "("):
        return parse_call_args(toks, j + 1, env)
    else:
        return Args(args=[], kwargs=[], next=j, error="")


def eval_test(toks, i, env, obj):
    """`is [not] test [args]`; `i` is just after `is`."""
    return eval_test_at(toks, i, env, obj, False)


def eval_test_at(toks, j, env, obj, negated):
    """The body of `eval_test`, threading the optional `not` flag."""
    if is_name(tok(toks, j), "not"):
        return eval_test_at(toks, j + 1, env, obj, True)
    else:
        name_tok = tok(toks, j)
        if name_tok.kind != "name":
            return P(value=halt("expected test name after 'is'"), next=j)

        else:
            nm = dotted_name(toks, j + 1, name_tok.sval)
            a = parse_test_args(toks, nm.next, env)
            if a.error != "":
                return P(value=halt(a.error), next=a.next)
            else:
                result = F.apply_test(nm.name, obj, a.args)
                final = V.bool_(not V.truthy(result)) if negated else result
                return eval_filter(toks, a.next, env, final)


def parse_test_args(toks, j, env):
    """Test arguments: `(...)` or a single bare primary."""
    t = tok(toks, j)
    if is_op(t, "("):
        return parse_call_args(toks, j + 1, env)
    elif t.kind == "name" or t.kind == "string" or t.kind == "int" or t.kind == "float" or is_op(t, "(") or is_op(t, "[") or is_op(t, "{"):
        p = eval_primary(toks, j, env)
        p2 = eval_postfix(toks, p.next, env, p.value)
        return Args(args=[p2.value], kwargs=[], next=p2.next, error="")
    else:
        return Args(args=[], kwargs=[], next=j, error="")


class Name:
    """A dotted name: its text and the next token index."""
    name: str
    next: int


def dotted_name(toks, j, name):
    """Collect a possibly dotted name starting at token `j`."""
    if is_op(tok(toks, j), "."):
        return dotted_name(toks, j + 2, name + "." + tok(toks, j + 1).sval)
    else:
        return Name(name=name, next=j)


def type_name(v):
    """A value's Python type name, for error messages."""
    match v.kind:
        case "string":
            return "str"
        case "int":
            return "int"
        case "float":
            return "float"
        case "list":
            return "list"
        case "dict":
            return "dict"
        case "bool":
            return "bool"
        case "none":
            return "NoneType"
        case _:
            return "Undefined"


def operand_error(a, b):
    """The fault Value when `a` or `b` is undefined or already an error, else `none`."""
    if a.kind == "undefined":
        return halt("'" + a.sval + "' is undefined")
    elif V.is_error(a):
        return a
    elif b.kind == "undefined":
        return halt("'" + b.sval + "' is undefined")
    elif V.is_error(b):
        return b
    else:
        return V.none()


def add(a, b):
    """`a + b`."""
    e = operand_error(a, b)
    if e.kind == "none":
        return add_ok(a, b)
    else:
        return e


def add_ok(a, b):
    """`a + b` on non-undefined operands."""
    if a.kind == V.K_STRING and b.kind == V.K_STRING:
        return V.text(a.sval + b.sval)
    elif a.kind == V.K_STRING:
        return halt("can only concatenate str (not \"" + type_name(b) + "\") to str")
    elif b.kind == V.K_STRING:
        return halt("can only concatenate str (not \"" + type_name(a) + "\") to str")
    else:
        return bin_num(a, b, lambda x, y: x + y, lambda x, y: x + y, "+")


def sub(a, b):
    """`a - b`."""
    e = operand_error(a, b)
    if e.kind == "none":
        return bin_num(a, b, lambda x, y: x - y, lambda x, y: x - y, "-")
    else:
        return e


def mul(a, b):
    """`a * b`."""
    e = operand_error(a, b)
    if e.kind == "none":
        return bin_num(a, b, lambda x, y: x * y, lambda x, y: x * y, "*")
    else:
        return e


def truediv(a, b):
    """`a / b`: always a float."""
    e = operand_error(a, b)
    if e.kind == "none":
        if V.is_number(a) and V.is_number(b):
            return V.float_(V.num_of(a) / V.num_of(b))
        else:
            return halt("unsupported operand type(s) for /")
    else:
        return e


def floordiv(a, b):
    """`a // b`: Python floor division."""
    e = operand_error(a, b)
    if e.kind == "none":
        if a.kind == V.K_INT and b.kind == V.K_INT:
            return V.int_(V.py_floordiv(a.ival, b.ival))
        else:
            return halt("unsupported operand type(s) for //")
    else:
        return e


def mod(a, b):
    """`a % b`: Python modulo."""
    e = operand_error(a, b)
    if e.kind == "none":
        if a.kind == V.K_INT and b.kind == V.K_INT:
            return V.int_(V.py_mod(a.ival, b.ival))
        else:
            return halt("unsupported operand type(s) for %")
    else:
        return e


def bin_num(a, b, ifun, ffun, op):
    """Numeric binary operation with int/float promotion."""
    if a.kind == V.K_INT and b.kind == V.K_INT:
        return V.int_(ifun(a.ival, b.ival))
    elif V.is_number(a) and V.is_number(b):
        return V.float_(ffun(V.num_of(a), V.num_of(b)))
    else:
        return halt("unsupported operand type(s) for " + op + ": '" + type_name(a) + "' and '" + type_name(b) + "'")


def neg(v):
    """Unary minus."""
    if v.kind == "undefined":
        return halt("'" + v.sval + "' is undefined")
    elif V.is_error(v):
        return v
    elif v.kind == V.K_INT:
        return V.int_(0 - v.ival)
    elif v.kind == V.K_FLOAT:
        return V.float_(0.0 - v.fval)
    else:
        return halt("bad operand type for unary -: '" + type_name(v) + "'")



def call_value(callee, args, kwargs):
    """Call a global function reference."""
    if callee.kind == V.K_FUNC:
        return call_global(callee.sval, args, kwargs)
    else:
        return halt("'" + V.concat_text(callee) + "' is not callable")



def call_global(name, args, kwargs):
    """Dispatch a built-in function."""
    match name:
        case "raise_exception":
            return halt(arg0(args).sval)

        case "namespace":
            return kwargs_dict(kwargs)
        case "dict":
            return kwargs_dict(kwargs)
        case "range":
            return range_value(args)
        case "strftime_now":
            return halt("strftime_now is not available in the Osprey port")

        case _:
            return halt("Unknown function '" + name + "'")



def arg0(args):
    """Argument 0, or `none` when absent."""
    return unwrap_or(listGet(args, 0), V.none())


def kwargs_dict(kwargs):
    """A dict Value from keyword arguments."""
    keys = toList(map(range(len(kwargs)), lambda i: unwrap_or(listGet(kwargs, i), V.KwVal(name="", value=V.none())).name))
    vals = toList(map(range(len(kwargs)), lambda i: unwrap_or(listGet(kwargs, i), V.KwVal(name="", value=V.none())).value))
    return V.dict_(keys, vals)


def range_value(args):
    """`range(stop)` or `range(start, stop)` as a list of ints."""
    if len(args) >= 2:
        start = arg0(args).ival
        stop = unwrap_or(listGet(args, 1), V.int_(start)).ival
        return V.list_(range_from(start, stop, []))
    else:
        return V.list_(range_from(0, arg0(args).ival, []))


def range_from(i, stop, acc):
    """Collect `i, i+1, ...` below `stop`."""
    if i >= stop:
        return acc
    else:
        return range_from(i + 1, stop, listAppend(acc, V.int_(i)))
