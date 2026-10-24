# osprey: flavor=python
# osprey: namespace=re
# osprey: module=Escapes
"""Pattern-level escape handling: the fragment an escape compiles to.

`\\d \\D \\w \\W \\s \\S` become category classes, `\\A \\Z \\b \\B` become
assertions, `\\1`-`\\9` become backreferences (invalid when the group is not
open yet), and everything else decodes to a literal via `Charset.escape_cp`.
"""

from re import Charset, Program, Unicode

__all__ = ["parse_escape", "lit_frag", "cat_frag"]


def is_digit(cp):
    """Whether a code point is an ASCII digit."""
    return cp >= 48 and cp <= 57


def cat_frag(code):
    """A pattern-level category escape as a class instruction."""
    return Program.ins_class(0, 0, [Program.cat_piece(code)])


def lit_frag(cp, flags):
    """A literal: exact, or a one-codepoint class under IGNORECASE."""
    if Program.has(flags, Program.I_BIT):
        return Program.ins_class(0, 1, [Program.range_piece(cp, cp)])
    else:
        return Program.ins_lit(cp, 0)


def parse_escape(cps, i, n, flags, gs):
    """The fragment for the escape at `i`, plus the next index."""
    if i + 1 >= n:
        return Program.fail("bad escape (end of pattern)", i)
    else:
        return escape_dispatch(cps, i, n, flags, gs)


def escape_dispatch(cps, i, n, flags, gs):
    """Dispatch on the escaped character."""
    c = Program.at(cps, i + 1)
    if c == 100 or c == 68 or c == 119 or c == 87 or c == 115 or c == 83:
        return Success(value=Program.PRes(frag=[cat_frag(cat_code(c))], next=i + 2, gs=gs))
    elif c == 65:
        return Success(value=Program.PRes(frag=[Program.ins("a_start")], next=i + 2, gs=gs))
    elif c == 90:
        return Success(value=Program.PRes(frag=[Program.ins_ab("eol", 0, 1)], next=i + 2, gs=gs))
    elif c == 98:
        return Success(value=Program.PRes(frag=[Program.ins_a("wb", 1)], next=i + 2, gs=gs))
    elif c == 66:
        return Success(value=Program.PRes(frag=[Program.ins_a("wb", 0)], next=i + 2, gs=gs))
    elif is_digit(c) and c != 48:
        return backref(cps, i, n, gs, c - 48)
    else:
        return literal_escape(cps, i, n, flags, gs)


def cat_code(c):
    """The category code for `\\d \\D \\w \\W \\s \\S`."""
    if c == 100:
        return 1
    elif c == 68:
        return -1
    elif c == 119:
        return 2
    elif c == 87:
        return -2
    elif c == 115:
        return 3
    else:
        return -3


def backref(cps, i, n, gs, gid):
    """A `\\1`-`\\9` backreference, when the group exists."""
    if gid <= gs.count:
        return Success(value=Program.PRes(frag=[Program.ins_a("backref", gid)], next=i + 2, gs=gs))
    else:
        return Program.fail(f"invalid group reference {gid}", i + 1)


def literal_escape(cps, i, n, flags, gs):
    """An escape that decodes to one literal code point."""
    match Charset.escape_cp(cps, i, n):
        case Error(message=message):
            return Error(message=message)
        case Success(value=value):
            cp = unwrap_or(listGet(value, 0), 0)
            nxt = unwrap_or(listGet(value, 1), n)
            return Success(value=Program.PRes(frag=[lit_frag(cp, flags)], next=nxt, gs=gs))
