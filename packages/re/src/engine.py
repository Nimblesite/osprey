# osprey: flavor=python
# osprey: namespace=re
# osprey: module=Engine
"""The backtracking matcher over a compiled program.

`run` executes the instruction program against a code-point list with CPython's
leftmost-first, greedy/lazy semantics. The matcher is a backtracking VM: `step`
advances the program counter and `backtrack` resumes the most recent `split`
alternative, both as tail calls so the native stack stays flat; the alternatives
live in an explicit heap list (`frames`), which is why long inputs cannot blow
the native stack (the README's recursion strategy). `endpos` constrains the
accept position: -1 for match/search, the string length for fullmatch, and the
lookbehind position for its fixed-width sub-run. Captures are saved as code
point spans in `saves` (slot 2*i is group i's start, 2*i+1 its end, -1 means
the group did not participate).
"""

from re import Program, Unicode

__all__ = ["run", "Hit"]


class Hit:
    """A successful match: the end position and the capture slots."""
    pos: int
    saves: list[int]


class Frame:
    """A backtracking alternative: resume at `pc`/`pos` with these saves."""
    pc: int
    pos: int
    saves: list[int]


def run(prog, cps, n, start, endpos):
    """Run the main program from `start`, requiring end at `endpos` (-1 = any)."""
    saves = toList(map(range(2 * prog.groups + 2), lambda _: -1))
    return step(prog, cps, n, "match", endpos, 0, start, saves, [])


def code_at(code, pc):
    """Instruction `pc`, or a harmless `match` past the end."""
    return unwrap_or(listGet(code, pc), Program.ins("match"))


def step(prog, cps, n, acc_op, endpos, pc, pos, saves, frames):
    """Advance the VM; a tail call, so the native stack stays flat."""
    ins = code_at(prog.code, pc)
    op = ins.op
    if op == acc_op:
        if endpos >= 0 and pos != endpos:
            return backtrack(prog, cps, n, acc_op, endpos, frames)
        else:
            return Success(value=Hit(pos=pos, saves=saves))
    elif op == "lit":
        if pos < n and lit_hit(ins, Unicode.at(cps, pos)):
            return step(prog, cps, n, acc_op, endpos, pc + 1, pos + 1, saves, frames)
        else:
            return backtrack(prog, cps, n, acc_op, endpos, frames)
    elif op == "any":
        if pos < n and any_hit(ins, Unicode.at(cps, pos)):
            return step(prog, cps, n, acc_op, endpos, pc + 1, pos + 1, saves, frames)
        else:
            return backtrack(prog, cps, n, acc_op, endpos, frames)
    elif op == "class":
        if pos < n and class_hit(ins, Unicode.at(cps, pos)):
            return step(prog, cps, n, acc_op, endpos, pc + 1, pos + 1, saves, frames)
        else:
            return backtrack(prog, cps, n, acc_op, endpos, frames)
    elif op == "split":
        return step(prog, cps, n, acc_op, endpos, ins.a, pos, saves, listAppend(frames, Frame(pc=ins.b, pos=pos, saves=saves)))
    elif op == "jmp":
        return step(prog, cps, n, acc_op, endpos, ins.a, pos, saves, frames)
    elif op == "save":
        return step(prog, cps, n, acc_op, endpos, pc + 1, pos, set_slot(saves, ins.a, pos), frames)
    elif op == "bol":
        if bol_hit(ins, cps, pos):
            return step(prog, cps, n, acc_op, endpos, pc + 1, pos, saves, frames)
        else:
            return backtrack(prog, cps, n, acc_op, endpos, frames)
    elif op == "eol":
        if eol_hit(ins, cps, n, pos):
            return step(prog, cps, n, acc_op, endpos, pc + 1, pos, saves, frames)
        else:
            return backtrack(prog, cps, n, acc_op, endpos, frames)
    elif op == "a_start":
        if pos == 0:
            return step(prog, cps, n, acc_op, endpos, pc + 1, pos, saves, frames)
        else:
            return backtrack(prog, cps, n, acc_op, endpos, frames)
    elif op == "wb":
        if wb_hit(ins, cps, n, pos):
            return step(prog, cps, n, acc_op, endpos, pc + 1, pos, saves, frames)
        else:
            return backtrack(prog, cps, n, acc_op, endpos, frames)
    elif op == "backref":
        return backref_step(prog, cps, n, acc_op, endpos, pc, pos, saves, frames, ins.a)
    elif op == "lookahead":
        return lookahead_step(prog, cps, n, acc_op, endpos, pc, pos, saves, frames, ins)
    elif op == "lookbehind" or op == "lookbehind_not":
        return lookbehind_step(prog, cps, n, acc_op, endpos, pc, pos, saves, frames, ins)
    else:
        return backtrack(prog, cps, n, acc_op, endpos, frames)


def backtrack(prog, cps, n, acc_op, endpos, frames):
    """Resume the most recent alternative, or fail when none remain."""
    if len(frames) == 0:
        return Error(message="")
    else:
        f = unwrap_or(listGet(frames, len(frames) - 1), Frame(pc=0, pos=0, saves=[]))
        return step(prog, cps, n, acc_op, endpos, f.pc, f.pos, f.saves, drop_last(frames))


def drop_last(xs):
    """The list without its last element."""
    return toList(map(range(len(xs) - 1), lambda i: unwrap_or(listGet(xs, i), Frame(pc=0, pos=0, saves=[]))))


def set_slot(saves, k, v):
    """`saves` with slot `k` replaced by `v`."""
    return toList(map(range(len(saves)), lambda i: v if i == k else unwrap_or(listGet(saves, i), 0)))


def lit_hit(ins, cp):
    """Whether `cp` matches a literal (with IGNORECASE folding)."""
    if ins.b == 1:
        return cp == ins.a or Unicode.fold_lower(cp) == ins.a or Unicode.fold_upper(cp) == ins.a
    else:
        return cp == ins.a


def any_hit(ins, cp):
    """Whether `cp` matches `.` (newline only when DOTALL)."""
    return ins.a == 1 or cp != 10


def class_hit(ins, cp):
    """Whether `cp` matches a class (negate and IGNORECASE applied)."""
    hit = class_match(ins.ranges, cp, ins.b)
    return not hit if ins.a == 1 else hit


def class_match(ranges, cp, icase):
    """Whether any class piece matches `cp` (or its case folds)."""
    if icase == 1:
        return piece_hit(ranges, cp, 0) or piece_hit(ranges, Unicode.fold_lower(cp), 0) or piece_hit(ranges, Unicode.fold_upper(cp), 0)
    else:
        return piece_hit(ranges, cp, 0)


def piece_hit(ranges, cp, i):
    """Whether class piece `i` matches `cp`."""
    if i >= len(ranges):
        return False
    else:
        p = unwrap_or(listGet(ranges, i), [0, 0, 0])
        hit = piece_match(unwrap_or(listGet(p, 0), 0), unwrap_or(listGet(p, 1), 0), unwrap_or(listGet(p, 2), 0), cp)
        return hit or piece_hit(ranges, cp, i + 1)


def piece_match(code, lo, hi, cp):
    """One class piece: a range, or a digit/word/space category."""
    if code == 0:
        return cp >= lo and cp <= hi
    elif code == 1:
        return Unicode.is_digit(cp)
    elif code == -1:
        return not Unicode.is_digit(cp)
    elif code == 2:
        return Unicode.is_word(cp)
    elif code == -2:
        return not Unicode.is_word(cp)
    elif code == 3:
        return Unicode.is_space(cp)
    else:
        return not Unicode.is_space(cp)


def bol_hit(ins, cps, pos):
    """Whether `^` matches at `pos`."""
    return pos == 0 or (ins.a == 1 and Unicode.at(cps, pos - 1) == 10)


def eol_hit(ins, cps, n, pos):
    """Whether `$` or `\\Z` matches at `pos`."""
    if pos == n:
        return True
    elif Unicode.at(cps, pos) == 10 and ins.b == 0:
        return ins.a == 1 or pos == n - 1
    else:
        return False


def wb_hit(ins, cps, n, pos):
    """Whether `\\b` (a=1) or `\\B` (a=0) matches at `pos`."""
    before = pos > 0 and Unicode.is_word(Unicode.at(cps, pos - 1))
    after = pos < n and Unicode.is_word(Unicode.at(cps, pos))
    boundary = before != after
    return boundary if ins.a == 1 else not boundary


def backref_step(prog, cps, n, acc_op, endpos, pc, pos, saves, frames, gid):
    """A `\\1`-style backreference."""
    start = Unicode.at(saves, 2 * gid)
    end = Unicode.at(saves, 2 * gid + 1)
    if start < 0:
        return step(prog, cps, n, acc_op, endpos, pc + 1, pos, saves, frames)
    elif compare_span(cps, n, pos, start, end - start):
        return step(prog, cps, n, acc_op, endpos, pc + 1, pos + end - start, saves, frames)
    else:
        return backtrack(prog, cps, n, acc_op, endpos, frames)


def compare_span(cps, n, pos, start, left):
    """Whether cps[pos:pos+left] equals cps[start:start+left]."""
    if left == 0:
        return True
    elif pos >= n or start >= n:
        return False
    elif Unicode.at(cps, pos) == Unicode.at(cps, start):
        return compare_span(cps, n, pos + 1, start + 1, left - 1)
    else:
        return False


def lookahead_step(prog, cps, n, acc_op, endpos, pc, pos, saves, frames, ins):
    """A `(?=...)` / `(?!...)` assertion (captures propagate)."""
    sub = step(prog, cps, n, "acc", -1, ins.a, pos, saves, [])
    if ins.b == 1:
        match sub:
            case Success():
                return backtrack(prog, cps, n, acc_op, endpos, frames)
            case Error():
                return step(prog, cps, n, acc_op, endpos, pc + 1, pos, saves, frames)
    else:
        match sub:
            case Success(value=value):
                return step(prog, cps, n, acc_op, endpos, pc + 1, pos, value.saves, frames)
            case Error():
                return backtrack(prog, cps, n, acc_op, endpos, frames)


def lookbehind_step(prog, cps, n, acc_op, endpos, pc, pos, saves, frames, ins):
    """A `(?<=...)` / `(?<!...)` fixed-width assertion (captures dropped)."""
    negate = 1 if ins.op == "lookbehind_not" else 0
    if pos < ins.b:
        if negate == 1:
            return step(prog, cps, n, acc_op, endpos, pc + 1, pos, saves, frames)
        else:
            return backtrack(prog, cps, n, acc_op, endpos, frames)
    else:
        return lookbehind_run(prog, cps, n, acc_op, endpos, pc, pos, saves, frames, ins, negate)


def lookbehind_run(prog, cps, n, acc_op, endpos, pc, pos, saves, frames, ins, negate):
    """Run the lookbehind sub-pattern against the fixed-width prefix."""
    sub = step(prog, cps, n, "acc", pos, ins.a, pos - ins.b, saves, [])
    if negate == 1:
        match sub:
            case Success():
                return backtrack(prog, cps, n, acc_op, endpos, frames)
            case Error():
                return step(prog, cps, n, acc_op, endpos, pc + 1, pos, saves, frames)
    else:
        match sub:
            case Success():
                return step(prog, cps, n, acc_op, endpos, pc + 1, pos, saves, frames)
            case Error():
                return backtrack(prog, cps, n, acc_op, endpos, frames)
