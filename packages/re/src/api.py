# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
# osprey: namespace=re
# osprey: module=Api
"""The `re` module's public API, over the compiled backtracking matcher.

`compile` turns a pattern into a `Pattern` (or a CPython `re.error` message);
`match`, `fullmatch` and `search` return a `MatchResult` (a `Match`, or no
match); `findall` and `finditer` walk the non-overlapping matches with
CPython's group rule (one group -> that group, many -> lists); `split` cuts
the text with groups interleaved and 3.7+ empty-match handling; `sub`/`subn`
expand `\\1`, `\\g<1>` and `\\g<name>` templates. `escape` backslashes
metacharacters and `purge` is a no-op. Names, argument order and flag values
match CPython; the documented divergences are that `split` and `sub` take
explicit `maxsplit`/`count` (no default arguments), and non-participating
groups read as the empty string (Osprey has no null string).
"""

from re import Engine, Parser, Program, Unicode

__all__ = [
    "IGNORECASE", "MULTILINE", "DOTALL", "VERBOSE", "I", "M", "S", "X",
    "Pattern", "Match", "MatchResult", "SubnResult", "FindallResult",
    "compile", "match", "fullmatch", "search", "findall", "finditer",
    "split", "sub", "subn", "escape", "purge",
    "group", "groups", "groupdict", "lastindex", "span",
]

IGNORECASE = 2
MULTILINE = 8
DOTALL = 16
VERBOSE = 64
I = 2
M = 8
S = 16
X = 64


class Pattern:
    """A compiled pattern: its program, the source, and the flags."""
    prog: Program.Program
    pattern: str
    flags: int


class Match:
    """One match: its span, the matched text, and each group's text and
    participation. Group i's text is at index i (0 is the whole match); a
    group that did not participate has text "" and set[i] is False."""
    start: int
    end: int
    text: str
    group_texts: list[str]
    group_set: list[bool]
    names: list[str]


class MatchResult:
    """The outcome of match/search/fullmatch: a Match, or none at all."""
    kind: str
    match: Match


class SubnResult:
    """`subn`'s result: the substituted text and the number of replacements."""
    text: str
    count: int


class TplPiece:
    """One replacement-template piece: a literal (kind 0, in `text`) or a
    group reference (kind 1, in `gid`)."""
    kind: int
    text: str
    gid: int


class FindallResult:
    """`findall`'s result under CPython's group rule: kind "texts" (zero or
    one group, a flat list) or kind "rows" (many groups, a list of lists)."""
    kind: str
    texts: list[str]
    rows: list[list[str]]


def compile(pattern, flags):
    """Compile a pattern, or Error with CPython's `re.error` message."""
    match Parser.compile_pattern(pattern, flags):
        case Error(message=message):
            return Error(message=message)
        case Success(value=value):
            return Success(value=Pattern(prog=value, pattern=pattern, flags=flags))


def none_match():
    """A `MatchResult` for "no match"."""
    return MatchResult(kind="none", match=Match(start=0, end=0, text="", group_texts=[""], group_set=[False], names=[""]))


def match_record(prog, cps, start, hit):
    """A `Match` from a program, the text's code points, and a hit."""
    return Match(start=start, end=hit.pos, text=Unicode.text_of(cps, start, hit.pos),
                 group_texts=group_texts(prog, cps, hit.saves),
                 group_set=group_flags(prog, hit.saves),
                 names=prog.group_names)


def group_texts(prog, cps, saves):
    """The text of each group (index i = group i), "" when not participated."""
    return fold(range(1, prog.groups + 1), [""], lambda acc, i: listAppend(acc, group_text_at(prog, cps, saves, i)))


def group_text_at(prog, cps, saves, i):
    """Group i's text, or "" when it did not participate."""
    s = Unicode.at(saves, 2 * i)
    e = Unicode.at(saves, 2 * i + 1)
    return Unicode.text_of(cps, s, e) if s >= 0 else ""


def group_flags(prog, saves):
    """Whether each group participated (index i = group i)."""
    return fold(range(1, prog.groups + 1), [False], lambda acc, i: listAppend(acc, Unicode.at(saves, 2 * i) >= 0))


def search_from(prog, cps, n, i):
    """The leftmost match at or after position `i`, or none."""
    if i > n:
        return none_match()
    else:
        return search_at(prog, cps, n, i)


def search_at(prog, cps, n, i):
    """A match starting exactly at `i`, else the next start position."""
    match Engine.run(prog, cps, n, i, -1):
        case Success(value=value):
            return MatchResult(kind="some", match=match_record(prog, cps, i, value))
        case Error():
            return search_from(prog, cps, n, i + 1)


def match(pattern, text, flags):
    """Match the pattern at the start of the text."""
    return run_one(pattern, text, flags, "match")


def fullmatch(pattern, text, flags):
    """Match the pattern against the whole text."""
    return run_one(pattern, text, flags, "fullmatch")


def search(pattern, text, flags):
    """Find the first match anywhere in the text."""
    return run_one(pattern, text, flags, "search")


def run_one(pattern, text, flags, op):
    """One match/search/fullmatch verdict."""
    match compile(pattern, flags):
        case Error(message=message):
            return Error(message=message)
        case Success(value=value):
            cps = Unicode.code_points(text)
            n = len(cps)
            if op == "search":
                return Success(value=search_from(value.prog, cps, n, 0))
            else:
                return Success(value=run_fixed(value, cps, n, op))


def run_fixed(pat, cps, n, op):
    """A match/fullmatch verdict at the start of the text."""
    endpos = n if op == "fullmatch" else -1
    match Engine.run(pat.prog, cps, n, 0, endpos):
        case Success(value=value):
            return MatchResult(kind="some", match=match_record(pat.prog, cps, 0, value))
        case Error():
            return none_match()


def findall(pattern, text, flags):
    """All non-overlapping matches under CPython's group rule, as a
    `FindallResult` (flat `texts` for 0/1 groups, `rows` for many)."""
    match compile(pattern, flags):
        case Error(message=message):
            return Error(message=message)
        case Success(value=value):
            cps = Unicode.code_points(text)
            n = len(cps)
            matches = collect_matches(value.prog, cps, n, 0, [])
            return Success(value=findall_rule(value.prog, matches))


def findall_rule(prog, matches):
    """CPython's findall grouping: one group -> that group, many -> lists."""
    if prog.groups == 0:
        return FindallResult(kind="texts", texts=mapList(matches, lambda m: m.text), rows=[])
    elif prog.groups == 1:
        return FindallResult(kind="texts", texts=mapList(matches, lambda m: unwrap_or(listGet(m.group_texts, 1), "")), rows=[])
    else:
        return FindallResult(kind="rows", texts=[], rows=mapList(matches, lambda m: toList(map(range(1, prog.groups + 1), lambda i: unwrap_or(listGet(m.group_texts, i), "")))))


def finditer(pattern, text, flags):
    """All non-overlapping matches as a list of Match records."""
    match compile(pattern, flags):
        case Error(message=message):
            return Error(message=message)
        case Success(value=value):
            cps = Unicode.code_points(text)
            n = len(cps)
            return Success(value=collect_matches(value.prog, cps, n, 0, []))


def collect_matches(prog, cps, n, pos, acc):
    """The matches at or after `pos`, in order, advancing past empty matches."""
    mr = search_from(prog, cps, n, pos)
    if mr.kind == "none":
        return acc
    else:
        m = mr.match
        nxt = m.end if m.end > m.start else m.start + 1
        return collect_matches(prog, cps, n, nxt, listAppend(acc, m))


def split(pattern, text, maxsplit, flags):
    """Split text on matches, interleaving captured groups (3.7+ empties)."""
    match compile(pattern, flags):
        case Error(message=message):
            return Error(message=message)
        case Success(value=value):
            cps = Unicode.code_points(text)
            n = len(cps)
            return Success(value=split_prog(value, cps, n, maxsplit, 0, 0, 0, []))


def split_prog(p, cps, n, maxsplit, pos, last, done, acc):
    """The split result: segments and group texts, up to `maxsplit` cuts."""
    mr = search_from(p.prog, cps, n, pos)
    if mr.kind == "none" or (maxsplit > 0 and done >= maxsplit):
        return listAppend(acc, Unicode.text_of(cps, last, n))
    else:
        return split_cut(p, cps, n, maxsplit, last, done, acc, mr.match)


def split_cut(p, cps, n, maxsplit, last, done, acc, m):
    """One split cut: the segment before the match, then the groups."""
    before = listAppend(acc, Unicode.text_of(cps, last, m.start))
    with_groups = fold(range(1, p.prog.groups + 1), before, lambda a, i: listAppend(a, unwrap_or(listGet(m.group_texts, i), "")))
    nxt = m.end if m.end > m.start else m.start + 1
    return split_prog(p, cps, n, maxsplit, nxt, m.end, done + 1, with_groups)


def sub(pattern, repl, text, count, flags):
    """Replace matches with the template `repl` (at most `count` times)."""
    match subn(pattern, repl, text, count, flags):
        case Error(message=message):
            return Error(message=message)
        case Success(value=value):
            return Success(value=value.text)


def subn(pattern, repl, text, count, flags):
    """Replace matches, returning the new text and the number of replacements."""
    match compile(pattern, flags):
        case Error(message=message):
            return Error(message=message)
        case Success(value=value):
            return build_subn(value, repl, text, count)


def build_subn(p, repl, text, count):
    """Expand `repl` into a template and substitute over the text."""
    match parse_template(repl, p.prog):
        case Error(message=message):
            return Error(message=message)
        case Success(value=value):
            cps = Unicode.code_points(text)
            n = len(cps)
            return Success(value=sub_prog(p.prog, cps, n, value, count, 0, 0, "", 0))


def sub_prog(prog, cps, n, pieces, count, pos, last, acc, done):
    """Build the substituted string match by match."""
    if count > 0 and done >= count:
        return SubnResult(text=acc + Unicode.text_of(cps, last, n), count=done)
    else:
        return sub_step(prog, cps, n, pieces, count, pos, last, acc, done)


def sub_step(prog, cps, n, pieces, count, pos, last, acc, done):
    """One substitution step."""
    mr = search_from(prog, cps, n, pos)
    if mr.kind == "none":
        return SubnResult(text=acc + Unicode.text_of(cps, last, n), count=done)
    else:
        m = mr.match
        repl = expand(pieces, m)
        nxt = m.end if m.end > m.start else m.start + 1
        return sub_prog(prog, cps, n, pieces, count, nxt, m.end, acc + Unicode.text_of(cps, last, m.start) + repl, done + 1)


def expand(pieces, m):
    """The replacement string for one match: literals and group texts."""
    return foldList(pieces, "", lambda acc, p: acc + piece_text(p, m))


def piece_text(p, m):
    """One template piece: a literal, or a group's text ("" when absent)."""
    if p.kind == 0:
        return p.text
    else:
        if p.gid == 0:
            return m.text
        elif unwrap_or(listGet(m.group_set, p.gid), False):
            return unwrap_or(listGet(m.group_texts, p.gid), "")
        else:
            return ""


def parse_template(tpl, prog):
    """Compile a replacement template into [kind, value] pieces."""
    cps = Unicode.code_points(tpl)
    return parse_template_at(cps, len(cps), prog, 0, [])


def parse_template_at(cps, n, prog, i, acc) -> Result[list[TplPiece], str]:
    """Scan the template code points into pieces."""
    if i >= n:
        return Success(value=acc)
    elif Unicode.at(cps, i) == 92:
        return template_escape(cps, n, prog, i, acc)
    else:
        return parse_template_at(cps, n, prog, i + 1, listAppend(acc, TplPiece(kind=0, text=Unicode.utf8_of(Unicode.at(cps, i)), gid=0)))


def template_escape(cps, n, prog, i, acc):
    """One backslash escape inside a replacement template."""
    c = Unicode.at(cps, i + 1)
    if c >= 49 and c <= 57:
        return parse_template_at(cps, n, prog, i + 2, listAppend(acc, TplPiece(kind=1, text="", gid=c - 48)))
    elif c == 103:
        return template_group(cps, n, prog, i, acc)
    elif c == 110:
        return parse_template_at(cps, n, prog, i + 2, listAppend(acc, TplPiece(kind=0, text="\n", gid=0)))
    elif c == 116:
        return parse_template_at(cps, n, prog, i + 2, listAppend(acc, TplPiece(kind=0, text="\t", gid=0)))
    elif c == 114:
        return parse_template_at(cps, n, prog, i + 2, listAppend(acc, TplPiece(kind=0, text="\r", gid=0)))
    elif c == 92:
        return parse_template_at(cps, n, prog, i + 2, listAppend(acc, TplPiece(kind=0, text="\\", gid=0)))
    elif c >= 65 and c <= 122:
        return Error(message="bad escape \\" + Unicode.utf8_of(c) + " at position 0")
    else:
        return parse_template_at(cps, n, prog, i + 2, listAppend(acc, TplPiece(kind=0, text="\\" + Unicode.utf8_of(c), gid=0)))


def template_group(cps, n, prog, i, acc):
    """A `\\g<name>` or `\\g<number>` template group reference."""
    if Unicode.at(cps, i + 2) != 60:
        return Error(message="bad escape \\g at position 0")
    else:
        return template_group_name(cps, n, prog, i + 3, acc)


def template_group_name(cps, n, prog, j, acc):
    """Read the name of a `\\g<...>` reference and resolve it to a group."""
    name = read_group_name(cps, j, n, 62)
    text = name.name
    after = name.next
    if after >= n:
        return Error(message="missing group name")
    elif text == "":
        return Error(message="missing group name")
    elif is_digits(text):
        return template_number_ref(cps, n, prog, after, acc, parse_int(text))
    elif mapContains(prog.names, text):
        return parse_template_at(cps, n, prog, after + 1, listAppend(acc, TplPiece(kind=1, text="", gid=unwrap_or(mapGet(prog.names, text), 0))))
    else:
        return Error(message=f"unknown group name '{text}'")


def template_number_ref(cps, n, prog, after, acc, gid):
    """A numeric `\\g<1>` reference, validated against the group count."""
    if gid > prog.groups:
        return Error(message=f"invalid group reference {gid}")
    else:
        return parse_template_at(cps, n, prog, after + 1, listAppend(acc, TplPiece(kind=1, text="", gid=gid)))


def read_group_name(cps, j, n, term):
    """The template group name, and the terminator's index."""
    return read_group_name_from(cps, j, n, term, "")


def read_group_name_from(cps, j, n, term, acc):
    """Accumulate the template group name until `term` or the end."""
    if j >= n or Unicode.at(cps, j) == term:
        return Program.NameRead(name=acc, next=j)
    else:
        return read_group_name_from(cps, j + 1, n, term, acc + Unicode.utf8_of(Unicode.at(cps, j)))


def is_digits(s):
    """Whether a string is all ASCII digits and non-empty."""
    return s != "" and digits_ok(s, 0)


def digits_ok(s, i):
    """Whether every byte of `s` from `i` on is an ASCII digit."""
    if i >= len(s):
        return True
    else:
        b = unwrap_or(byteAt(s, i), 48)
        return b >= 48 and b <= 57 and digits_ok(s, i + 1)


def parse_int(s):
    """The decimal value of a non-empty digit string."""
    return unwrap_or(parseInt(s), 0)


def escape(text):
    """Backslash every metacharacter, exactly as `re.escape`."""
    return escape_at(Unicode.code_points(text), 0, "")


def escape_at(cps, i, acc):
    """Scan code points, backslashing the metacharacter set."""
    if i >= len(cps):
        return acc
    else:
        c = Unicode.at(cps, i)
        ch = Unicode.utf8_of(c)
        return escape_at(cps, i + 1, acc + ("\\" + ch if is_meta(c) else ch))


def is_meta(c):
    """Whether `re.escape` backslashes code point `c`."""
    return c == 40 or c == 41 or c == 91 or c == 93 or c == 123 or c == 125 or c == 63 or c == 42 or c == 43 or c == 45 or c == 124 or c == 94 or c == 36 or c == 92 or c == 46 or c == 38 or c == 126 or c == 35 or c == 32 or (c >= 9 and c <= 13)


def purge():
    """A no-op: `re` caches compiled patterns, Osprey has no such cache."""
    return 0


def group(m, i):
    """Group i's text (Success), or Error when it did not participate."""
    if i == 0:
        return Success(value=m.text)
    elif i >= 1 and i < len(m.group_texts) and unwrap_or(listGet(m.group_set, i), False):
        return Success(value=unwrap_or(listGet(m.group_texts, i), ""))
    elif i >= 1 and i < len(m.group_texts):
        return Error(message="")
    else:
        return Error(message="no such group")


def groups(m, default):
    """Every group's text, with `default` for groups that did not participate."""
    return toList(map(range(1, len(m.group_texts)), lambda i: group_or(m, i, default)))


def group_or(m, i, default):
    """Group i's text or `default` when it did not participate."""
    if unwrap_or(listGet(m.group_set, i), False):
        return unwrap_or(listGet(m.group_texts, i), "")
    else:
        return default


def groupdict(m, default):
    """A string-keyed map of named groups to their text (or `default`)."""
    return fold(range(1, len(m.names)), {}, lambda acc, i: gd_add(acc, m, i, default))


def gd_add(acc, m, i, default):
    """Add group i to the groupdict when it is named."""
    name = unwrap_or(listGet(m.names, i), "")
    if name == "":
        return acc
    else:
        return mapSet(acc, name, group_or(m, i, default))


def lastindex(m):
    """The index of the last participating group, or 0 when none."""
    return fold(range(1, len(m.group_set)), 0, lambda acc, i: i if unwrap_or(listGet(m.group_set, i), False) else acc)


def span(m):
    """The match's `[start, end]` span as a two-element list."""
    return [m.start, m.end]
