# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
# osprey: namespace=re
# osprey: module=Parser
"""The pattern parser: source text to a compiled instruction program.

A recursive-descent parser over the pattern's code points, mirroring CPython's
`re._parser`: the same alternation/sequence/quantifier grammar, the same
leftmost-first ordering, and the same error messages and code-point positions.
Inline `(?i)`/`(?m)`/`(?s)`/`(?x)` and scoped `(?i:...)` flags are folded into
the instructions at compile time: IGNORECASE widens literals/classes, DOTALL
widens `.`, MULTILINE widens `^`/`$`, and VERBOSE changes scanning. Fragment
shapes come from `Builder`, escapes from `Escapes`, classes from `Charset`,
inline flags from `Flags`, lookbehind width from `Width`. See the README for
the unsupported extensions (possessive quantifiers, atomic groups,
conditionals, `\\N{...}`).
"""

from re import Builder, Charset, Escapes, Flags, Program, Unicode, Width

__all__ = ["compile_pattern"]


def init_gs():
    """The initial group state (index 0 is the whole match)."""
    return Program.GState(names={}, count=0, group_names=[""])


def is_digit(cp):
    """Whether a code point is an ASCII digit."""
    return cp >= 48 and cp <= 57


def is_ws(cp):
    """Whether a code point is VERBOSE-mode whitespace."""
    return cp == 32 or cp == 9 or cp == 10 or cp == 13 or cp == 11 or cp == 12


def is_ident_byte(b):
    """Whether an ASCII byte can appear in a group name (identifier rules)."""
    return (b >= 65 and b <= 90) or (b >= 97 and b <= 122) or b == 95 or (b >= 48 and b <= 57)


def valid_name(name):
    """Whether a group name is an ASCII identifier (Python's rule; Unicode
    names are a documented divergence)."""
    if name == "":
        return False
    else:
        first = unwrap_or(byteAt(name, 0), 48)
        return is_ident_byte(first) and not (first >= 48 and first <= 57) and name_body_ok(name, 1)


def name_body_ok(name, i):
    """Whether every byte from `i` on is an identifier byte."""
    if i >= len(name):
        return True
    else:
        return is_ident_byte(unwrap_or(byteAt(name, i), 48)) and name_body_ok(name, i + 1)


def add_group(gs, name):
    """A group state with one more capturing group."""
    idx = gs.count + 1
    names = mapSet(gs.names, name, idx) if name != "" else gs.names
    return Program.GState(names=names, count=idx, group_names=listAppend(gs.group_names, name))


def read_number(cps, j, n):
    """[value, next]: the decimal value of the digit run at `j`."""
    return read_number_from(cps, j, n, 0)


def read_number_from(cps, j, n, acc):
    """Accumulate cps[j] while it is an ASCII digit."""
    if j >= n or not is_digit(Program.at(cps, j)):
        return [acc, j]
    else:
        return read_number_from(cps, j + 1, n, acc * 10 + Program.at(cps, j) - 48)


def read_name(cps, j, n, term):
    """The string from j to `term`, and the terminator's index."""
    return read_name_from(cps, j, n, term, "")


def read_name_from(cps, j, n, term, acc):
    """Accumulate the name until `term` or the end."""
    if j >= n or Program.at(cps, j) == term:
        return Program.NameRead(name=acc, next=j)
    else:
        return read_name_from(cps, j + 1, n, term, acc + Unicode.utf8_of(Program.at(cps, j)))


def skip_ws(cps, i, n):
    """Skip VERBOSE whitespace and comments."""
    if i >= n:
        return i
    elif is_ws(Program.at(cps, i)):
        return skip_ws(cps, i + 1, n)
    elif Program.at(cps, i) == 35:
        return skip_ws(cps, skip_comment(cps, i + 1, n), n)
    else:
        return i


def skip_comment(cps, i, n):
    """Skip to the end of a VERBOSE comment."""
    if i >= n or Program.at(cps, i) == 10:
        return i
    else:
        return skip_comment(cps, i + 1, n)


def is_stop(cp):
    """Whether a code point ends the current subpattern."""
    return cp == 41 or cp == 124


def parse_alt(cps, i, n, flags, gs) -> Result[Program.PRes, str]:
    """Parse `seq (| seq)*`."""
    match parse_seq(cps, i, n, flags, gs):
        case Error(message=message):
            return Error(message=message)
        case Success(value=value):
            if value.next < n and Program.at(cps, value.next) == 124:
                return parse_alt_rest(cps, value, n, flags)
            else:
                return Success(value=value)


def parse_alt_rest(cps, first, n, flags):
    """Parse the right side of `|` and join with the left fragment."""
    match parse_alt(cps, first.next + 1, n, flags, first.gs):
        case Error(message=message):
            return Error(message=message)
        case Success(value=value):
            return Success(value=Program.PRes(frag=Builder.alt_frag(first.frag, value.frag), next=value.next, gs=value.gs))


def parse_seq(cps, i, n, flags, gs) -> Result[Program.PRes, str]:
    """Parse a run of quantified atoms."""
    j = skip_ws(cps, i, n) if Program.has(flags, Program.X_BIT) else i
    if j >= n or is_stop(Program.at(cps, j)):
        return Success(value=Program.PRes(frag=[], next=j, gs=gs))
    else:
        return parse_seq_rest(cps, j, n, flags, gs)


def parse_seq_rest(cps, i, n, flags, gs):
    """Parse one atom, then the rest of the sequence."""
    match parse_quantified(cps, i, n, flags, gs):
        case Error(message=message):
            return Error(message=message)
        case Success(value=value):
            return seq_after(cps, value, n, flags)


def seq_after(cps, atom, n, flags):
    """Append the rest of the sequence after one atom."""
    match parse_seq(cps, atom.next, n, flags, atom.gs):
        case Error(message=message):
            return Error(message=message)
        case Success(value=value):
            return Success(value=Program.PRes(frag=listConcat(atom.frag, value.frag), next=value.next, gs=value.gs))


def parse_quantified(cps, i, n, flags, gs):
    """Parse an atom and any quantifier that follows it."""
    match parse_atom(cps, i, n, flags, gs):
        case Error(message=message):
            return Error(message=message)
        case Success(value=value):
            return add_quantifier(cps, value, n, flags)


def parse_braces(cps, j, n):
    """Parse `{m}`, `{m,}` or `{m,n}` at `j`; a [mn, mx, next] triple, or [] when `{` is a literal."""
    if j + 1 < n and Program.at(cps, j + 1) == 125:
        return []
    else:
        return parse_braces_lo(cps, j, n)


def parse_braces_lo(cps, j, n):
    """Parse the digits after `{`."""
    lo = read_number(cps, j + 1, n)
    k = unwrap_or(listGet(lo, 1), j + 1)
    if k >= n:
        return []
    elif Program.at(cps, k) == 125:
        return [unwrap_or(listGet(lo, 0), 0), unwrap_or(listGet(lo, 0), 0), k + 1]
    elif Program.at(cps, k) == 44:
        return parse_braces_hi(cps, j, k, n, unwrap_or(listGet(lo, 0), 0))
    else:
        return []


def parse_braces_hi(cps, j, k, n, mn):
    """Parse the `,hi}` tail of a brace quantifier."""
    hi = read_number(cps, k + 1, n)
    k2 = unwrap_or(listGet(hi, 1), k + 1)
    if k2 < n and Program.at(cps, k2) == 125:
        hiv = unwrap_or(listGet(hi, 0), -1)
        return [mn, hiv if is_digit(Program.at(cps, k2 - 1)) else -1, k2 + 1]
    else:
        return []


def quant_mn(c):
    """The [min, max] of the `*`, `+`, `?` quantifier `c`."""
    if c == 42:
        return [0, -1]
    elif c == 43:
        return [1, -1]
    else:
        return [0, 1]


def add_quantifier(cps, r, n, flags):
    """Attach a quantifier after the atom `r`, if one is present."""
    j = skip_ws(cps, r.next, n) if Program.has(flags, Program.X_BIT) else r.next
    c = Program.at(cps, j)
    if c == 42 or c == 43 or c == 63:
        mn = unwrap_or(listGet(quant_mn(c), 0), 0)
        mx = unwrap_or(listGet(quant_mn(c), 1), -1)
        return lazy_after(cps, r, j + 1, n, mn, mx)
    elif c == 123:
        return brace_quantifier(cps, r, j, n)
    else:
        return Success(value=Program.PRes(frag=r.frag, next=j, gs=r.gs))


def lazy_after(cps, r, after, n, mn, mx):
    """Apply the lazy/possessive suffix after a quantifier, then build it."""
    if after < n and Program.at(cps, after) == 63:
        return lazy_done(cps, r, after + 1, mn, mx, False)
    elif after < n and Program.at(cps, after) == 43:
        return lazy_done(cps, r, after + 1, mn, mx, True)
    else:
        return lazy_done(cps, r, after, mn, mx, True)


def lazy_done(cps, r, after, mn, mx, greedy):
    """Build the quantified fragment with its greediness decided."""
    return Success(value=Program.PRes(frag=Builder.repeat_frag(r.frag, mn, mx, greedy), next=after, gs=r.gs))


def brace_quantifier(cps, r, j, n):
    """Apply a `{m,n}` quantifier, or leave a literal `{`."""
    spec = parse_braces(cps, j, n)
    if len(spec) == 0:
        return Success(value=Program.PRes(frag=r.frag, next=j, gs=r.gs))
    else:
        return brace_spec(cps, r, j, n, spec)


def brace_spec(cps, r, j, n, spec):
    """Validate a parsed brace spec and build the quantified fragment."""
    mn = unwrap_or(listGet(spec, 0), 0)
    mx = unwrap_or(listGet(spec, 1), 0)
    after = unwrap_or(listGet(spec, 2), j)
    if mx >= 0 and mn > mx:
        return Program.fail("min repeat greater than max repeat", j)
    else:
        return lazy_after(cps, r, after, n, mn, mx)


def parse_atom(cps, i, n, flags, gs):
    """Parse one unquantified atom."""
    c = Program.at(cps, i)
    if c == 40:
        return parse_group(cps, i, n, flags, gs)
    elif c == 91:
        return parse_class_atom(cps, i, n, flags, gs)
    elif c == 46:
        return Success(value=Program.PRes(frag=[Program.ins_a("any", 1 if Program.has(flags, Program.S_BIT) else 0)], next=i + 1, gs=gs))
    elif c == 94:
        return Success(value=Program.PRes(frag=[Program.ins_a("bol", 1 if Program.has(flags, Program.M_BIT) else 0)], next=i + 1, gs=gs))
    elif c == 36:
        return Success(value=Program.PRes(frag=[Program.ins_ab("eol", 1 if Program.has(flags, Program.M_BIT) else 0, 0)], next=i + 1, gs=gs))
    elif c == 92:
        return Escapes.parse_escape(cps, i, n, flags, gs)
    elif c == 42 or c == 43 or c == 63:
        return Program.fail("nothing to repeat", i)
    elif c == 123:
        return Success(value=Program.PRes(frag=[Escapes.lit_frag(123, flags)], next=i + 1, gs=gs))
    else:
        return Success(value=Program.PRes(frag=[Escapes.lit_frag(c, flags)], next=i + 1, gs=gs))


def parse_class_atom(cps, i, n, flags, gs):
    """A `[...]` atom via `Charset.parse_class`."""
    match Charset.parse_class(cps, i, n, 1 if Program.has(flags, Program.I_BIT) else 0):
        case Error(message=message):
            return Error(message=message)
        case Success(value=value):
            return Success(value=Program.PRes(frag=[value.ins], next=value.next, gs=gs))


def parse_group(cps, i, n, flags, gs):
    """A `(` group: capturing, or an extended `(?...)` form."""
    if i + 1 < n and Program.at(cps, i + 1) == 63:
        return parse_ext(cps, i, n, flags, gs)
    else:
        return parse_capture(cps, i, i + 1, n, flags, gs, "")


def parse_capture(cps, i, j, n, flags, gs, name):
    """A capturing group with content at `j`, saved into two slots."""
    gs2 = add_group(gs, name)
    match parse_alt(cps, j, n, flags, gs2):
        case Error(message=message):
            return Error(message=message)
        case Success(value=value):
            if value.next >= n or Program.at(cps, value.next) != 41:
                return Program.fail("missing ), unterminated subpattern", i)
            else:
                idx = gs2.count
                frag = listConcat([Program.ins_a("save", 2 * idx)], listConcat(value.frag, [Program.ins_a("save", 2 * idx + 1)]))
                return Success(value=Program.PRes(frag=frag, next=value.next + 1, gs=value.gs))


def parse_ext(cps, i, n, flags, gs):
    """Dispatch a `(?...)` extended group."""
    c = Program.at(cps, i + 2)
    if c == 58:
        return parse_plain(cps, i, i + 3, n, flags, gs)
    elif c == 80:
        return parse_named(cps, i, n, flags, gs)
    elif c == 61:
        return parse_lookahead(cps, i, i + 3, n, flags, gs, 0)
    elif c == 33:
        return parse_lookahead(cps, i, i + 3, n, flags, gs, 1)
    elif c == 60:
        return parse_lookbehind_dispatch(cps, i, n, flags, gs)
    elif c == 35:
        return parse_comment(cps, i, i + 3, n, flags, gs)
    elif c == 45 or Flags.flag_bit(c) != 0:
        return parse_inline(cps, i, n, flags, gs)
    else:
        return Program.fail("unknown extension ?" + Unicode.utf8_of(c), i + 1)


def parse_plain(cps, i, j, n, flags, gs):
    """A non-capturing group (no saves) with content at `j`."""
    match parse_alt(cps, j, n, flags, gs):
        case Error(message=message):
            return Error(message=message)
        case Success(value=value):
            if value.next >= n or Program.at(cps, value.next) != 41:
                return Program.fail("missing ), unterminated subpattern", i)
            else:
                return Success(value=Program.PRes(frag=value.frag, next=value.next + 1, gs=value.gs))


def parse_inline(cps, i, n, flags, gs):
    """An inline flag group: global flags are an error mid-pattern."""
    match Flags.parse_flag_group(cps, i, n):
        case Error(message=message):
            return Error(message=message)
        case Success(value=value):
            if value.kind == "global":
                return Program.fail("global flags not at the start of the expression", i)
            else:
                new_flags = Program.drop_bits(Program.add_bit(flags, value.add), value.dele)
                return parse_plain(cps, i, value.next, n, new_flags, gs)


def parse_named(cps, i, n, flags, gs):
    """A `(?P...)` group: named capture or named backreference."""
    k = i + 3
    c = Program.at(cps, k)
    if c == 60:
        return parse_named_group(cps, i, k + 1, n, flags, gs)
    elif c == 61:
        return parse_named_backref(cps, k + 1, n, gs)
    else:
        return Program.fail("unknown extension ?P" + Unicode.utf8_of(c), i + 1)


def parse_named_group(cps, i, ns, n, flags, gs):
    """A `(?P<name>...)` capturing group."""
    nr = read_name(cps, ns, n, 62)
    name = nr.name
    after = nr.next
    if after >= n:
        return Program.fail("missing >, unterminated name", ns)
    elif not valid_name(name):
        return Program.fail("bad character in group name '" + name + "'", ns)
    elif mapContains(gs.names, name):
        return Program.fail("redefinition of group name '" + name + "' as group " + toString(gs.count + 1) + "; was group " + toString(unwrap_or(mapGet(gs.names, name), 0)), ns)
    else:
        return parse_capture(cps, i, after + 1, n, flags, gs, name)


def parse_named_backref(cps, ns, n, gs):
    """A `(?P=name)` backreference."""
    nr = read_name(cps, ns, n, 41)
    name = nr.name
    after = nr.next
    if after >= n and name == "":
        return Program.fail("missing group name", ns)
    elif after >= n:
        return Program.fail("missing ), unterminated name", ns)
    elif name == "":
        return Program.fail("missing group name", ns)
    elif not mapContains(gs.names, name):
        return Program.fail("unknown group name '" + name + "'", ns)
    else:
        gid = unwrap_or(mapGet(gs.names, name), 0)
        return Success(value=Program.PRes(frag=[Program.ins_a("backref", gid)], next=after + 1, gs=gs))


def parse_lookahead(cps, i, j, n, flags, gs, negate):
    """A `(?=...)` or `(?!...)` zero-width assertion."""
    match parse_alt(cps, j, n, flags, gs):
        case Error(message=message):
            return Error(message=message)
        case Success(value=value):
            if value.next >= n or Program.at(cps, value.next) != 41:
                return Program.fail("missing ), unterminated subpattern", i)
            else:
                frag = listConcat([Program.ins_ab("lookahead", 1, negate)], listConcat(Program.shift(value.frag, 1), [Program.ins("acc")]))
                return Success(value=Program.PRes(frag=frag, next=value.next + 1, gs=value.gs))


def parse_lookbehind_dispatch(cps, i, n, flags, gs):
    """A `(?<=...)` or `(?<!...)` assertion."""
    c = Program.at(cps, i + 3)
    if c != 61 and c != 33:
        return Program.fail("unknown extension ?<" + Unicode.utf8_of(c), i + 1)
    else:
        return parse_lookbehind(cps, i, i + 4, n, flags, gs, 1 if c == 33 else 0)


def parse_lookbehind(cps, i, j, n, flags, gs, negate):
    """A fixed-width lookbehind assertion with content at `j`."""
    match parse_alt(cps, j, n, flags, gs):
        case Error(message=message):
            return Error(message=message)
        case Success(value=value):
            if value.next >= n or Program.at(cps, value.next) != 41:
                return Program.fail("missing ), unterminated subpattern", i)
            else:
                return lookbehind_frag(cps, i, j, value, negate)


def lookbehind_frag(cps, i, j, r, negate):
    """Build the lookbehind fragment once the content width is known."""
    w = Width.sub_width(cps, j, len(cps))
    width = unwrap_or(listGet(w, 0), -1)
    if width < 0:
        return Program.fail("look-behind requires fixed-width pattern", i)
    else:
        op = "lookbehind_not" if negate == 1 else "lookbehind"
        frag = listConcat([Program.ins_ab(op, 1, width)], listConcat(Program.shift(r.frag, 1), [Program.ins("acc")]))
        return Success(value=Program.PRes(frag=frag, next=r.next + 1, gs=r.gs))


def parse_comment(cps, i, j, n, flags, gs):
    """A `(?#...)` comment, which matches nothing."""
    end = skip_to_paren(cps, j, n)
    if end >= n:
        return Program.fail("missing ), unterminated comment", i)
    else:
        return Success(value=Program.PRes(frag=[], next=end + 1, gs=gs))


def skip_to_paren(cps, i, n):
    """The index of the next `)`."""
    if i >= n or Program.at(cps, i) == 41:
        return i
    else:
        return skip_to_paren(cps, i + 1, n)


def compile_pattern(pattern, flags):
    """Compile a pattern string: a Program, or a CPython `re.error` message."""
    cps = Unicode.code_points(pattern)
    n = len(cps)
    start = Flags.scan_global_flags(cps, 0, n, flags)
    return compile_from(cps, n, start)


def compile_from(cps, n, start):
    """Parse the pattern after the leading global flags, and finish it."""
    flags = unwrap_or(listGet(start, 0), 0)
    i = unwrap_or(listGet(start, 1), 0)
    match parse_alt(cps, i, n, flags, init_gs()):
        case Error(message=message):
            return Error(message=message)
        case Success(value=value):
            if value.next < n:
                return Program.fail("unbalanced parenthesis", value.next)
            else:
                code = Program.append_match(value.frag)
                return Success(value=Program.Program(code=code, groups=value.gs.count, names=value.gs.names, group_names=value.gs.group_names))
