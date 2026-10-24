# osprey: flavor=python
"""@osprey/re's suite: the port against CPython's verdicts.

Every case in `re_expected.Expected` was computed by CPython 3.12's own `re`
(scripts/oracles/re.py) and is string-encoded with `\\x1f` between fields and
`\\x1e` inside a field. Each check below decodes one case and compares the
port's answer with the oracle's, through `testing.Check.strictly` so an
integer fault stops the suite.
"""
from re import Api, Builder, Program
from re_expected import Expected
from testing import Check

SEP = "\x1f"
SEP2 = "\x1e"


def f(xs, i):
    """Element `i` of a string list, or "" past the end."""
    return unwrap_or(listGet(xs, i), "")


def fields(s):
    """A SEP-joined string as its fields."""
    return unwrap_or(split(s, SEP), [])


def int_of(s):
    """A decimal string as an int."""
    return unwrap_or(parseInt(s), 0)


def run_op(op, pattern, text, flags):
    """The port's match/search/fullmatch verdict for one operation."""
    if op == "match":
        return Api.match(pattern, text, flags)
    elif op == "search":
        return Api.search(pattern, text, flags)
    else:
        return Api.fullmatch(pattern, text, flags)


def match_case(case):
    """One match/search/fullmatch case: a list of assertions."""
    op = f(case, 0)
    pattern = f(case, 1)
    flags = int_of(f(case, 2))
    text = f(case, 3)
    enc = fields(f(case, 4))
    match run_op(op, pattern, text, flags):
        case Error(message=message):
            return [Check.failed(op + " " + pattern, "compile error: " + message)]
        case Success(value=value):
            return match_verdict(op, pattern, text, value, enc)


def match_verdict(op, pattern, text, mr, enc):
    """A list of assertions for one match verdict."""
    name = op + " " + pattern + " / " + text
    if f(enc, 0) == "0":
        return [Check.equal(name, mr.kind, "none")]
    elif mr.kind != "some":
        return [Check.failed(name, "expected a match")]
    else:
        return match_body(name, mr.match, enc)


def match_body(name, m, enc):
    """Assertions for a Match's span, groups and named groups."""
    span = [Check.equal(name + ".start", m.start, int_of(f(enc, 1))),
            Check.equal(name + ".end", m.end, int_of(f(enc, 2))),
            Check.equal(name + ".lastindex", Api.lastindex(m), int_of(f(enc, 3)))]
    ng = int_of(f(enc, 4))
    groups = toList(map(range(1, ng + 1), lambda i: group_check(name, m, i, f(enc, 5 + 2 * (i - 1)), f(enc, 6 + 2 * (i - 1)))))
    nn = int_of(f(enc, 5 + 2 * ng))
    named = toList(map(range(1, nn + 1), lambda j: named_check(name, m, enc, ng, j)))
    return listConcat(span, listConcat(groups, named))


def group_check(name, m, i, present, want):
    """One group's text against the oracle."""
    got = Api.group(m, i)
    label = name + ".g" + toString(i)
    match got:
        case Success(value=value):
            return Check.equal(label, value, want) if present == "1" else Check.failed(label + ".none", "unexpected participating group")
        case Error(message=message):
            return Check.failed(label, "expected a participating group: " + message) if present == "1" else Check.equal(label + ".none", message, "")


def named_check(name, m, enc, ng, j):
    """One named group's text against the oracle's groupdict."""
    base = 5 + 2 * ng + 1 + 3 * (j - 1)
    gname = f(enc, base)
    want = f(enc, base + 2)
    gd = Api.groupdict(m, "")
    got = unwrap_or(mapGet(gd, gname), "")
    return Check.equal(name + "." + gname, got, want)


def findall_case(case):
    """One findall case: a list of assertions."""
    pattern = f(case, 0)
    flags = int_of(f(case, 1))
    text = f(case, 2)
    enc = fields(f(case, 3))
    match Api.findall(pattern, text, flags):
        case Error(message=message):
            return [Check.failed("findall " + pattern, "compile error: " + message)]
        case Success(value=value):
            return findall_verdict(pattern, text, value, enc)


def findall_verdict(pattern, text, fr, enc):
    """Assertions for a FindallResult."""
    name = "findall " + pattern + " / " + text
    kind = f(enc, 0)
    n = int_of(f(enc, 1))
    if kind == "2":
        return [Check.equal(name + ".kind", fr.kind, "rows"), Check.equal(name + ".count", len(fr.rows), n)] + rows_ok(fr, enc, 2, n)
    else:
        return [Check.equal(name + ".kind", fr.kind, "texts"), Check.equal(name + ".count", len(fr.texts), n)] + texts_ok(fr, enc, 2, n, name)


def texts_ok(fr, enc, start, n, name):
    """Assertions for a flat findall text list."""
    return toList(map(range(n), lambda i: Check.equal(name + ".t" + toString(i), unwrap_or(listGet(fr.texts, i), ""), f(enc, start + i))))


def rows_ok(fr, enc, start, n):
    """Assertions for a many-group findall result."""
    return fold(range(n), [], lambda acc, i: listConcat(acc, row_ok(fr, enc, start, i)))


def row_ok(fr, enc, start, i):
    """Assertions for one findall row against its SEP2-encoded pairs."""
    cell = f(enc, start + i)
    pair = unwrap_or(split(cell, SEP2), [])
    ng = intDiv(len(pair), 2)
    got = unwrap_or(listGet(fr.rows, i), [])
    return toList(map(range(ng), lambda g: Check.equal("row" + toString(i) + "g" + toString(g), unwrap_or(listGet(got, g), ""), f(pair, ng + g))))


def finditer_case(case):
    """One finditer case: a list of assertions."""
    pattern = f(case, 0)
    flags = int_of(f(case, 1))
    text = f(case, 2)
    enc = fields(f(case, 3))
    match Api.finditer(pattern, text, flags):
        case Error(message=message):
            return [Check.failed("finditer " + pattern, "compile error: " + message)]
        case Success(value=value):
            n = int_of(f(enc, 0))
            return [Check.equal("finditer " + pattern + " / " + text + ".count", len(value), n)] + iter_ok(value, enc, n)


def iter_ok(matches, enc, n):
    """Assertions for each finditer Match's span and groups."""
    return iter_checks(matches, enc, n, 0, 1, [])


def iter_checks(matches, enc, n, i, offset, acc):
    """Consume each variable-length match from the oracle's flat field stream."""
    if i == n:
        return acc
    else:
        ng = int_of(f(enc, offset + 4))
        nn = int_of(f(enc, offset + 5 + 2 * ng))
        end = offset + 6 + 2 * ng + 3 * nn
        checks = iter_match(matches, enc, i, offset, end)
        return iter_checks(matches, enc, n, i + 1, end, listConcat(acc, checks))


def iter_match(matches, enc, i, offset, end):
    """Assertions for one finditer Match against its encoded verdict."""
    m = unwrap_or(listGet(matches, i), Api.Match(start=0, end=0, text="", group_texts=[""], group_set=[False], names=[""], lastindex=0))
    sub = toList(map(range(offset, end), lambda j: f(enc, j)))
    return match_body("finditer" + toString(i), m, sub)


def split_case(case):
    """One split case: a list of assertions."""
    pattern = f(case, 0)
    flags = int_of(f(case, 1))
    text = f(case, 2)
    maxsplit = int_of(f(case, 3))
    enc = fields(f(case, 4))
    match Api.split(pattern, text, maxsplit, flags):
        case Error(message=message):
            return [Check.failed("split " + pattern, "compile error: " + message)]
        case Success(value=value):
            n = int_of(f(enc, 0))
            return [Check.equal("split " + pattern + " / " + text + ".count", len(value), n)] + split_ok(value, enc, n)


def split_ok(got, enc, n):
    """Assertions for each split element."""
    return toList(map(range(n), lambda i: Check.equal("split" + toString(i), unwrap_or(listGet(got, i), ""), f(unwrap_or(split(f(enc, 1 + i), SEP2), []), 1))))


def sub_case(case):
    """One sub case: a list of assertions."""
    pattern = f(case, 0)
    repl = f(case, 1)
    text = f(case, 2)
    count = int_of(f(case, 3))
    flags = int_of(f(case, 4))
    want = f(case, 5)
    match Api.sub(pattern, repl, text, count, flags):
        case Error(message=message):
            return [Check.failed("sub " + pattern, "error: " + message)]
        case Success(value=value):
            return [Check.equal("sub " + pattern + " / " + text, value, want)]


def subn_case(case):
    """One subn case: a list of assertions."""
    pattern = f(case, 0)
    repl = f(case, 1)
    text = f(case, 2)
    count = int_of(f(case, 3))
    flags = int_of(f(case, 4))
    want = f(case, 5)
    wantn = int_of(f(case, 6))
    match Api.subn(pattern, repl, text, count, flags):
        case Error(message=message):
            return [Check.failed("subn " + pattern, "error: " + message)]
        case Success(value=value):
            return [Check.equal("subn " + pattern + " / " + text, value.text, want),
                    Check.equal("subn " + pattern + ".n", value.count, wantn)]


def escape_case(case):
    """One escape case: a list of assertions."""
    return [Check.equal("escape " + f(case, 0), Api.escape(f(case, 0)), f(case, 1))]


def error_case(case):
    """One compile-error case: the port must return CPython's exact message."""
    pattern = f(case, 0)
    flags = int_of(f(case, 1))
    want = f(case, 2)
    match Api.compile(pattern, flags):
        case Error(message=message):
            return [Check.equal("error " + pattern, message, want)]
        case Success(value=value):
            return [Check.failed("error " + pattern, "compiled but should fail")]


def cases_for(constant, check):
    """Every case of one constant, flattened into assertions."""
    return fold(range(len(constant)), [], lambda acc, i: listConcat(acc, check(unwrap_or(listGet(constant, i), []))))


def instruction_text(code):
    """Instruction addresses pin fragment relocation independently of matching."""
    return join(mapList(code, lambda ins: f"{ins.op}:{ins.a}:{ins.b}"), ";")


def builder_cases():
    body = [Program.ins_lit(97, 0)]
    return [
        Check.equal("builder.plus.targets", instruction_text(Builder.repeat_frag(body, 1, -1, True)), "lit:97:0;split:2:4;lit:97:0;jmp:1:0"),
        Check.equal("builder.alt.targets", instruction_text(Builder.alt_frag(body, body)), "split:1:3;lit:97:0;jmp:4:0;lit:97:0"),
        Check.equal("builder.count.targets", instruction_text(Builder.repeat_frag(body, 2, 3, True)), "lit:97:0;lit:97:0;split:3:4;lit:97:0"),
    ]


def all_cases():
    """Every assertion, grouped by operation."""
    matches = listConcat(builder_cases(), cases_for(Expected.MATCH, match_case))
    findalls = cases_for(Expected.FINDALL, findall_case)
    finditers = cases_for(Expected.FINDITER, finditer_case)
    splits = cases_for(Expected.SPLIT, split_case)
    subs = cases_for(Expected.SUB, sub_case)
    subns = cases_for(Expected.SUBN, subn_case)
    escapes = cases_for(Expected.ESCAPE, escape_case)
    errors = cases_for(Expected.ERRORS, error_case)
    return listConcat(listConcat(listConcat(matches, findalls), listConcat(finditers, splits)), listConcat(listConcat(subs, subns), listConcat(escapes, errors)))


def main():
    match Check.strictly(lambda: Check.report(all_cases())):
        case Success(value=value):
            return value
        case Error(message=message):
            return print("suite stopped: " + message)
