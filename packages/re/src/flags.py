# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
# osprey: namespace=re
# osprey: module=Flags
"""Inline flag groups `(?i)`, `(?im)`, `(?i:...)`, `(?-i:...)`, `(?i-m:...)`.

`scan_global_flags` consumes leading `(?flags)` groups (CPython only allows
those at the very start of the pattern); `parse_flag_group` parses any flag
group the parser meets, so a non-leading global group raises "global flags
not at the start of the expression" with CPython's message and position.
Only the IGNORECASE/MULTILINE/DOTALL/VERBOSE flags are supported; the `a`,
`u` and `L` type flags are reported as "unknown flag" (a documented
divergence).
"""

from re import Program

__all__ = ["parse_flag_group", "scan_global_flags", "flag_bit", "FlagGroup"]


class FlagGroup:
    """A parsed flag group: global or scoped, the add/drop bits, the next index."""
    kind: str
    add: int
    dele: int
    next: int


def is_alpha(cp):
    """Whether a code point is an ASCII letter."""
    return (cp >= 65 and cp <= 90) or (cp >= 97 and cp <= 122)


def flag_bit(ch):
    """The flag bit for `i`, `m`, `s` or `x`, or 0."""
    if ch == 105:
        return Program.I_BIT
    elif ch == 109:
        return Program.M_BIT
    elif ch == 115:
        return Program.S_BIT
    elif ch == 120:
        return Program.X_BIT
    else:
        return 0


def read_flag_run(cps, j, n):
    """[bits, stop]: the flag bits of the run at `j`, and where it stopped."""
    return read_flag_run_from(cps, j, n, 0)


def read_flag_run_from(cps, j, n, acc):
    """Accumulate flag bits from `j`."""
    if j < n and flag_bit(Program.at(cps, j)) != 0:
        return read_flag_run_from(cps, j + 1, n, Program.add_bit(acc, flag_bit(Program.at(cps, j))))
    else:
        return [acc, j]


def parse_flag_group(cps, i, n):
    """Parse the flag group at `i` (where cps[i+1] is `?`)."""
    if Program.at(cps, i + 2) == 45:
        return del_group(cps, i + 3, n, 0)
    else:
        return add_group(cps, i, i + 2, n)


def add_group(cps, i, j, n):
    """Parse `(?flags...)` starting its flag run at `j`."""
    run = read_flag_run(cps, j, n)
    add = unwrap_or(listGet(run, 0), 0)
    k = unwrap_or(listGet(run, 1), j)
    c = Program.at(cps, k)
    if c == 41:
        return Success(value=FlagGroup(kind="global", add=add, dele=0, next=k + 1))
    elif c == 58:
        return Success(value=FlagGroup(kind="scoped", add=add, dele=0, next=k + 1))
    elif c == 45:
        return del_group(cps, k + 1, n, add)
    elif k >= n:
        return Program.fail("missing -, : or )", k)
    elif is_alpha(c):
        return Program.fail("unknown flag", k)
    else:
        return Program.fail("missing -, : or )", k)


def del_group(cps, j, n, add):
    """Parse the `-flags:` tail of a flag group."""
    run = read_flag_run(cps, j, n)
    dele = unwrap_or(listGet(run, 0), 0)
    k = unwrap_or(listGet(run, 1), j)
    c = Program.at(cps, k)
    if c == 58:
        return Success(value=FlagGroup(kind="scoped", add=add, dele=dele, next=k + 1))
    elif k >= n:
        return Program.fail("missing :", k)
    elif is_alpha(c):
        return Program.fail("unknown flag", k)
    else:
        return Program.fail("missing :", k)


def scan_global_flags(cps, i, n, flags):
    """Consume leading `(?flags)` groups; return [flags, next]."""
    if is_global_group(cps, i, n):
        run = read_flag_run(cps, i + 2, n)
        add = unwrap_or(listGet(run, 0), 0)
        k = unwrap_or(listGet(run, 1), i + 2)
        return scan_global_flags(cps, k + 1, n, Program.add_bit(flags, add))
    else:
        return [flags, i]


def is_global_group(cps, i, n):
    """Whether cps[i:] starts with a complete `(?flags)` group."""
    if i + 2 >= n or Program.at(cps, i) != 40 or Program.at(cps, i + 1) != 63:
        return False
    else:
        run = read_flag_run(cps, i + 2, n)
        return Program.at(cps, unwrap_or(listGet(run, 1), i + 2)) == 41 and unwrap_or(listGet(run, 0), 0) != 0
