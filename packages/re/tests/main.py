# osprey: flavor=python
"""@osprey/re's suite: the port against CPython's verdicts.

Every case in `re_expected.Expected` was computed by CPython 3.12's own `re`
(scripts/oracles/re.py) and is string-encoded with `\\x1f` between fields and
`\\x1e` inside a field. Each check below decodes one case and compares the
port's answer with the oracle's, through `testing.Check.strictly` so an
integer fault stops the suite.
"""
from re import Api, Builder, Charset, Engine, Escapes, Flags, Parser, Program, Tables, Unicode, Width
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


def unicode_cases():
    cps = Unicode.code_points("Aé😀")
    return [
        Check.ints_equal("unicode.code_points", cps, [65, 233, 128512]),
        Check.equal("unicode.text_of", Unicode.text_of(cps, 1, 3), "é😀"),
        Check.equal("unicode.at", Unicode.at(cps, 2), 128512),
        Check.equal("unicode.at_str", Unicode.at_str(["é"], 0), "é"),
        Check.equal("unicode.utf8_of", Unicode.utf8_of(128512), "😀"),
        Check.equal("unicode.fold_lower", Unicode.fold_lower(65), 97),
        Check.equal("unicode.fold_upper", Unicode.fold_upper(97), 65),
        Check.equal("unicode.is_digit", Unicode.is_digit(1633), True),
        Check.equal("unicode.is_word", Unicode.is_word(26085), True),
        Check.equal("unicode.is_space", Unicode.is_space(160), True),
        Check.equal("unicode.is_newline", Unicode.is_newline(10), True),
        Check.equal("unicode.non_digit", Unicode.is_digit(65), False),
    ]


def table_cases():
    return [
        Check.equal("tables.DIGITS", Unicode.in_ranges(Tables.DIGITS, 1633), True),
        Check.equal("tables.WORDS", Unicode.in_ranges(Tables.WORDS, 26085), True),
        Check.equal("tables.SPACES", Unicode.in_ranges(Tables.SPACES, 160), True),
        Check.ints_equal("tables.LOWER", unwrap_or(listGet(Tables.LOWER, 0), []), [65, 97]),
        Check.ints_equal("tables.UPPER", unwrap_or(listGet(Tables.UPPER, 0), []), [97, 65]),
    ]


def program_cases():
    lit = Program.Ins(op="lit", a=97, b=0, ranges=[])
    jump = Program.ins_a("jmp", 0)
    return [
        Check.equal("program.concat", instruction_text(Program.concat([lit], [jump])), "lit:97:0;jmp:1:0"),
        Check.equal("program.shift", instruction_text(Program.shift([Program.ins_ab("split", 1, 2)], 3)), "split:4:5"),
        Check.equal("program.append_match", instruction_text(Program.append_match([lit])), "lit:97:0;match:0:0"),
        Check.equal("program.ins_class", instruction_text([Program.ins_class(1, 0, [Program.range_piece(97, 122)])]), "class:1:0"),
        Check.ints_equal("program.range_piece", Program.range_piece(97, 122), [0, 97, 122]),
        Check.ints_equal("program.cat_piece", Program.cat_piece(-2), [-2, 0, 0]),
        Check.equal("program.at", Program.at([], 0), 0),
        Check.equal("program.has", Program.has(10, Program.I_BIT), True),
        Check.equal("program.add_bit", Program.add_bit(2, Program.M_BIT), 10),
        Check.equal("program.drop_bits", Program.drop_bits(10, Program.I_BIT), 8),
    ]


def flag_cases():
    flags = [Api.IGNORECASE, Api.MULTILINE, Api.DOTALL, Api.VERBOSE, Api.I, Api.M, Api.S, Api.X]
    bits = [Program.I_BIT, Program.M_BIT, Program.S_BIT, Program.X_BIT]
    scanned = Flags.scan_global_flags(Unicode.code_points("(?im)a"), 0, 6, 0)
    return listConcat([
        Check.ints_equal("api.flags", flags, [2, 8, 16, 64, 2, 8, 16, 64]),
        Check.ints_equal("program.flags", bits, [2, 8, 16, 64]),
        Check.ints_equal("flags.scan_global_flags", scanned, [10, 5]),
        Check.equal("flags.flag_bit", Flags.flag_bit(120), 64),
    ], success_checks("flags.parse_flag_group", Flags.parse_flag_group(Unicode.code_points("(?i:a)"), 0, 6), flag_group_checks))


def flag_group_checks(value):
    want = Flags.FlagGroup(kind="scoped", add=2, dele=0, next=4)
    return [Check.equal("flags.kind", value.kind, want.kind), Check.equal("flags.next", value.next, want.next), Check.equal("flags.add", value.add, want.add), Check.equal("flags.dele", value.dele, want.dele)]


def success_checks(name, result, checks):
    match result:
        case Success(value=value):
            return checks(value)
        case Error(message=message):
            return [Check.failed(name, message)]


def parser_cases():
    gs = Program.GState(names={}, count=0, group_names=[""])
    escaped = success_checks("escapes.parse_escape", Escapes.parse_escape([92, 100], 0, 2, 0, gs), parsed_escape_checks)
    literal = [Check.equal("escapes.lit_frag", instruction_text([Escapes.lit_frag(97, 0)]), "lit:97:0"), Check.equal("escapes.cat_frag", instruction_text([Escapes.cat_frag(1)]), "class:0:0")]
    compiled = success_checks("parser.compile_pattern", Parser.compile_pattern("a", 0), compiled_checks)
    return listConcat(listConcat(escaped, literal), compiled)


def parsed_escape_checks(value):
    want = Program.PRes(frag=[Program.ins_class(0, 0, [Program.cat_piece(1)])], next=2, gs=Program.GState(names={}, count=0, group_names=[""]))
    name = Program.NameRead(name="digit", next=2)
    return [Check.equal("escapes.fragment", instruction_text(value.frag), instruction_text(want.frag)), Check.equal("escapes.next", value.next, name.next), Check.equal("escapes.groups", value.gs.count, want.gs.count)]


def compiled_checks(prog):
    pat = Api.Pattern(prog=prog, pattern="a", flags=0)
    none = Api.MatchResult(kind="none", match=Api.Match(start=0, end=0, text="", group_texts=[""], group_set=[False], names=[""], lastindex=0))
    subn = Api.SubnResult(text="x", count=1)
    return listConcat([
        Check.equal("parser.literal", instruction_text(pat.prog.code), "lit:97:0;match:0:0"),
        Check.equal("program.groups_len", Program.groups_len(prog), 2),
        Check.equal("api.MatchResult", none.kind, "none"),
        Check.equal("api.SubnResult", subn.count, 1),
        Check.equal("api.purge", Api.purge(), 0),
    ], success_checks("engine.run", Engine.run(prog, [97], 1, 0, -1), hit_checks))


def hit_checks(value):
    want = Engine.Hit(pos=1, saves=[-1, -1], lastindex=0)
    return [Check.equal("engine.Hit.pos", value.pos, want.pos), Check.equal("engine.Hit.lastindex", value.lastindex, want.lastindex), Check.ints_equal("engine.Hit.saves", value.saves, want.saves)]


def charset_cases():
    decoded = success_checks("charset.escape_cp", Charset.escape_cp([92, 110], 0, 2), lambda value: [Check.ints_equal("charset.newline", value, [10, 2])])
    parsed = success_checks("charset.parse_class", Charset.parse_class([91, 97, 93], 0, 3, 0), lambda value: [Check.equal("charset.class.next", value.next, 3), Check.equal("charset.class.code", instruction_text([value.ins]), "class:0:0")])
    return listConcat(listConcat(decoded, parsed), [
        Check.ints_equal("charset.parse_hex", Charset.parse_hex([52, 49], 0, 2, 2), [65, 2, 2]),
        Check.equal("charset.single_cp", Charset.single_cp([[0, 97, 97]]), 97),
        Check.ints_equal("width.sub_width", Width.sub_width(Unicode.code_points("ab"), 0, 2), [2, 2]),
    ])


def helper_cases():
    return listConcat(listConcat(unicode_cases(), table_cases()), listConcat(listConcat(program_cases(), flag_cases()), listConcat(parser_cases(), charset_cases())))


def all_cases():
    """Every assertion, grouped by operation."""
    matches = listConcat(listConcat(builder_cases(), helper_cases()), cases_for(Expected.MATCH, match_case))
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
