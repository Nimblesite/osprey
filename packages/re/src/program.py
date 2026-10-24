# osprey: flavor=python
# osprey: namespace=re
# osprey: module=Program
"""The compiled regular expression: a flat instruction list plus group data.

The parser emits `Ins` records into one `Program.code` list. `split`/`jmp`/
`lookahead`/`lookbehind` targets are absolute indices into that list, so a
fragment built for one position is shifted by `shift` when concatenated. The
instruction set is a backtracking VM (Thompson split/jmp, saves, assertions),
documented in the README's recursion-strategy section. Shared parse helpers
(`at`, `fail`, `has`), the flag bits and the group bookkeeping records live
here so every parser module imports them from one place.
"""

__all__ = [
    "Ins", "Program", "GState", "PRes", "ins", "ins_a", "ins_ab", "ins_class",
    "ins_lit", "shift", "append_match", "groups_len", "range_piece", "cat_piece",
    "at", "fail", "has", "concat", "add_bit", "drop_bits", "I_BIT", "M_BIT", "S_BIT", "X_BIT", "NameRead",
]

I_BIT = 2
M_BIT = 8
S_BIT = 16
X_BIT = 64


class Ins:
    """One VM instruction. `op` selects the meaning of `a`, `b` and `ranges`:
    lit (a=code point, b=ignorecase), any (a=dotall), class (a=negate,
    b=ignorecase, ranges=pieces), split/jmp/lookahead/lookbehind (a,b = pc
    targets or width), save (a=slot), bol/eol (a=multiline, eol.b=abs-end),
    wb (a=1 for \\b), backref (a=group), acc/match (none)."""
    op: str
    a: int
    b: int
    ranges: list[list[int]]


class Program:
    """A compiled pattern: its code, the number of capturing groups, the group
    name -> index map, and the name of each index ("" when unnamed)."""
    code: list[Ins]
    groups: int
    names: dict[str, int]
    group_names: list[str]


class GState:
    """Group bookkeeping: name -> index, the count, and name per index."""
    names: dict[str, int]
    count: int
    group_names: list[str]


class PRes:
    """A parsed fragment: its instructions, the next pattern index, groups."""
    frag: list[Ins]
    next: int
    gs: GState


class NameRead:
    """A group name read from a pattern, and the terminator's index."""
    name: str
    next: int


def at(xs, i):
    """Element `i` of an int list, or 0 past the end."""
    return unwrap_or(listGet(xs, i), 0)


def fail(msg, pos):
    """A parse error with CPython's `at position N` suffix."""
    return Error(message=f"{msg} at position {pos}")


def has(flags, bit):
    """Whether a flag bit is set."""
    return flags % (bit * 2) >= bit


def add_bit(flags, bit):
    """`flags | bit` without bitwise operators."""
    return flags if has(flags, bit) else flags + bit


def drop_bits(flags, dele):
    """`flags & ~dele` without bitwise operators."""
    return drop_step(drop_step(drop_step(drop_step(flags, dele, I_BIT), dele, M_BIT), dele, S_BIT), dele, X_BIT)


def drop_step(flags, dele, bit):
    """Drop one flag bit when both `dele` and `flags` have it set."""
    return flags - bit if has(dele, bit) and has(flags, bit) else flags


def ins(op):
    """An instruction with default zero arguments and no ranges."""
    return Ins(op=op, a=0, b=0, ranges=[])


def ins_a(op, a):
    """An instruction with one int argument."""
    return Ins(op=op, a=a, b=0, ranges=[])


def ins_ab(op, a, b):
    """An instruction with two int arguments."""
    return Ins(op=op, a=a, b=b, ranges=[])


def ins_class(negate, icase, ranges):
    """A character-class instruction over class pieces."""
    return Ins(op="class", a=negate, b=icase, ranges=ranges)


def ins_lit(cp, icase):
    """A literal instruction."""
    return Ins(op="lit", a=cp, b=icase, ranges=[])


def is_target(op):
    """Whether the instruction carries a pc address to rebase."""
    return op == "split" or op == "jmp" or op == "lookahead" or op == "lookbehind" or op == "lookbehind_not"


def shift(frag, delta):
    """Rebase a fragment's internal pc targets by `delta`."""
    if delta == 0:
        return frag
    else:
        return mapList(frag, lambda i: shift_ins(i, delta))


def shift_ins(i, delta):
    """One instruction with its pc-target fields shifted by `delta`."""
    if i.op == "split":
        return Ins(op=i.op, a=i.a + delta, b=i.b + delta, ranges=i.ranges)
    elif i.op == "jmp" or i.op == "lookahead" or i.op == "lookbehind" or i.op == "lookbehind_not":
        return Ins(op=i.op, a=i.a + delta, b=i.b, ranges=i.ranges)
    else:
        return i


def concat(left, right):
    """Concatenate fragments, rebasing every target in the right fragment."""
    return listConcat(left, shift(right, len(left)))


def append_match(frag):
    """Append the accept instruction for the whole pattern."""
    return listAppend(frag, ins("match"))


def groups_len(prog):
    """The number of capture slots: one start/end pair per group."""
    return 2 * prog.groups + 2


def range_piece(lo, hi):
    """A character-range class piece `[0, lo, hi]`."""
    return [0, lo, hi]


def cat_piece(code):
    """A category class piece `[code, 0, 0]`: digit/word/space, negated when
    the code is negative."""
    return [code, 0, 0]
