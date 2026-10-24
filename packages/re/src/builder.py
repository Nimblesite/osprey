# osprey: flavor=python
# osprey: namespace=re
# osprey: module=Builder
"""Quantifier and alternation fragment builders.

Each builder wraps an instruction fragment in the split/jmp shape that the
backtracking matcher in `Engine` executes: greedy quantifiers prefer the body,
lazy ones prefer the exit, and alternation tries the left branch first —
CPython's leftmost-first semantics.
"""

from re import Program

__all__ = ["repeat_frag", "alt_frag"]


def repeat_frag(body, mn, mx, greedy):
    """Wrap `body` in a greedy or lazy quantifier (mx = -1 means unbounded)."""
    if mx == -1 and mn == 0:
        return star_frag(body, greedy)
    elif mx == -1 and mn == 1:
        return plus_frag(body, greedy)
    elif mn == 0 and mx == 1:
        return opt_frag(body, greedy)
    else:
        return count_frag(body, mn, mx, greedy)


def star_frag(body, greedy):
    """`body*`: a split that prefers the body (greedy) or the exit (lazy)."""
    l = len(body) + 2
    head = [Program.ins_ab("split", 1, l)] if greedy else [Program.ins_ab("split", l, 1)]
    return listConcat(head, listConcat(body, [Program.ins_a("jmp", 0)]))


def plus_frag(body, greedy):
    """`body+`: the body once, then `body*`."""
    return listConcat(body, star_frag(body, greedy))


def opt_frag(body, greedy):
    """`body?`: body once or not at all."""
    l = len(body) + 1
    head = [Program.ins_ab("split", 1, l)] if greedy else [Program.ins_ab("split", l, 1)]
    return listConcat(head, body)


def count_frag(body, mn, mx, greedy):
    """`body{mn,mx}`: mn copies, then up to mx-mn optional copies."""
    if mn == 0:
        return opt_range_frag(body, mx, greedy)
    else:
        return listConcat(body, count_frag(body, mn - 1, mx - 1, greedy))


def opt_range_frag(body, mx, greedy):
    """`body{0,mx}`: at most mx copies of the body."""
    if mx == 0:
        return []
    else:
        l = len(body) + 1
        head = [Program.ins_ab("split", 1, l)] if greedy else [Program.ins_ab("split", l, 1)]
        return listConcat(head, listConcat(body, opt_range_frag(body, mx - 1, greedy)))


def alt_frag(l, r):
    """`l|r`: try `l`, backtrack to `r`."""
    return listConcat([Program.ins_ab("split", 1, len(l) + 2)], listConcat(l, listConcat([Program.ins_a("jmp", len(r) + 1)], r)))
