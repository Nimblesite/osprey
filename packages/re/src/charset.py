# osprey: flavor=python
# osprey: namespace=re
# osprey: module=Charset
"""Character-class parsing and escape decoding, shared by the parser.

`parse_class` reads `[...]` into a class instruction whose `ranges` field is a
list of pieces: `[0, lo, hi]` for a code-point range, or `[code, 0, 0]` for a
category (1 digit, 2 word, 3 space, negated when negative). `escape_cp`
decodes the single-code-point escapes (`\\a \\f \\n \\r \\t \\v \\\\`,
`\\x41`, `\\u0041`, `\\U0001F600`, `\\0` octal, and non-letter escapes to
their literal). Error messages and positions match CPython.
"""

from re import Program, Tables, Unicode

__all__ = ["parse_class", "escape_cp", "parse_hex", "single_cp"]


class ClassItem:
    """One parsed class item: its pieces, the next index, and its source."""
    pieces: list[list[int]]
    next: int
    src: str


class ClassAtom:
    """A parsed class atom: its instruction and the next index."""
    ins: Program.Ins
    next: int


def at(xs, i):
    """Element `i` of an int list, or 0 past the end."""
    return unwrap_or(listGet(xs, i), 0)


def fail(msg, pos):
    """A parse error with CPython's `at position N` suffix."""
    return Error(message=f"{msg} at position {pos}")


def is_hex(cp):
    """Whether a code point is an ASCII hex digit."""
    return (cp >= 48 and cp <= 57) or (cp >= 97 and cp <= 102) or (cp >= 65 and cp <= 70)


def hex_val(cp):
    """The value of an ASCII hex digit, or 16 when not a digit."""
    if cp >= 48 and cp <= 57:
        return cp - 48
    elif cp >= 97 and cp <= 102:
        return cp - 87
    elif cp >= 65 and cp <= 70:
        return cp - 55
    else:
        return 16


def is_octal(cp):
    """Whether a code point is an ASCII octal digit."""
    return cp >= 48 and cp <= 55


def is_letter(cp):
    """Whether a code point is an ASCII letter."""
    return (cp >= 65 and cp <= 90) or (cp >= 97 and cp <= 122)


def parse_hex(cps, i, n, digits):
    """[value, read, next]: read up to `digits` hex digits from `i`."""
    return read_hex(cps, i, n, digits, 0, 0)


def read_hex(cps, i, n, left, read, acc):
    """Accumulate up to `left` hex digits from `i`."""
    if left == 0 or i >= n or not is_hex(at(cps, i)):
        return [acc, read, i]
    else:
        return read_hex(cps, i + 1, n, left - 1, read + 1, acc * 16 + hex_val(at(cps, i)))


def hex_digits_text(cps, i, count):
    """The text of `count` code points from `i`."""
    if count == 0:
        return ""
    else:
        return Unicode.utf8_of(at(cps, i)) + hex_digits_text(cps, i + 1, count - 1)


def escape_cp(cps, i, n):
    """Decode one backslash escape at `i` to a single code point."""
    if i + 1 >= n:
        return fail("bad escape (end of pattern)", i)
    else:
        return escape_body(cps, i, n)


def escape_body(cps, i, n):
    """Decode the escape after the backslash."""
    c = at(cps, i + 1)
    if c == 97:
        return Success(value=[7, i + 2])
    elif c == 102:
        return Success(value=[12, i + 2])
    elif c == 110:
        return Success(value=[10, i + 2])
    elif c == 114:
        return Success(value=[13, i + 2])
    elif c == 116:
        return Success(value=[9, i + 2])
    elif c == 118:
        return Success(value=[11, i + 2])
    elif c == 92:
        return Success(value=[92, i + 2])
    elif c == 120:
        return hex_escape(cps, i, n, 2)
    elif c == 117:
        return hex_escape(cps, i, n, 4)
    elif c == 85:
        return u_escape(cps, i, n)
    elif c == 48:
        return octal_escape(cps, i, n)
    elif is_letter(c):
        return fail("bad escape " + "\\" + Unicode.utf8_of(c), i)
    else:
        return Success(value=[c, i + 2])


def hex_escape(cps, i, n, digits):
    """Decode `\\x41`, `\\u0041` (exactly `digits` hex digits)."""
    got = parse_hex(cps, i + 2, n, digits)
    read = unwrap_or(listGet(got, 1), 0)
    if read < digits:
        return fail("incomplete escape " + "\\" + Unicode.utf8_of(at(cps, i + 1)) + hex_digits_text(cps, i + 2, read), i)
    else:
        return Success(value=[unwrap_or(listGet(got, 0), 0), unwrap_or(listGet(got, 2), n)])


def u_escape(cps, i, n):
    """Decode `\\U0001F600` (exactly eight hex digits, a valid scalar)."""
    got = parse_hex(cps, i + 2, n, 8)
    read = unwrap_or(listGet(got, 1), 0)
    if read < 8:
        return fail("incomplete escape " + "\\" + Unicode.utf8_of(at(cps, i + 1)) + hex_digits_text(cps, i + 2, read), i)
    else:
        return u_escape_cp(cps, i, n, got)


def u_escape_cp(cps, i, n, got):
    """Validate the decoded `\\U` scalar and return it."""
    cp = unwrap_or(listGet(got, 0), 0)
    if cp > 1114111 or (cp >= 55296 and cp <= 57343):
        return fail("bad escape " + "\\U" + hex_digits_text(cps, i + 2, 8), i)
    else:
        return Success(value=[cp, unwrap_or(listGet(got, 2), n)])


def octal_escape(cps, i, n):
    """Decode `\\0` followed by up to two octal digits."""
    return octal_from(cps, i + 2, n, 2, 0)


def octal_from(cps, i, n, left, acc):
    """Accumulate `left` octal digits from `i`."""
    if left == 0 or i >= n or not is_octal(at(cps, i)):
        return Success(value=[acc, i])
    else:
        return octal_from(cps, i + 1, n, left - 1, acc * 8 + at(cps, i) - 48)


def single_cp(pieces):
    """The code point when `pieces` is one single-codepoint range, else -1."""
    if len(pieces) == 1:
        p = unwrap_or(listGet(pieces, 0), [0, 0, 0])
        if unwrap_or(listGet(p, 0), 0) == 0 and unwrap_or(listGet(p, 1), -1) == unwrap_or(listGet(p, 2), -2):
            return unwrap_or(listGet(p, 1), -1)
        else:
            return -1
    else:
        return -1


def cat_of(c):
    """The category piece code for a category escape letter, or 0."""
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
    elif c == 83:
        return -3
    else:
        return 0


def class_item(cps, j, n):
    """One class item: its pieces, next index and source text, or an Error."""
    c = at(cps, j)
    if c == 92:
        return class_escape(cps, j, n)
    else:
        return Success(value=ClassItem(pieces=[Program.range_piece(c, c)], next=j + 1, src=Unicode.utf8_of(c)))


def class_escape(cps, i, n):
    """One backslash escape inside a character class."""
    if i + 1 >= n:
        return fail("bad escape (end of pattern)", i)
    else:
        return class_escape_body(cps, i, n)


def class_escape_body(cps, i, n):
    """Decode a class escape after its backslash."""
    c = at(cps, i + 1)
    if c == 98:
        return Success(value=ClassItem(pieces=[Program.range_piece(8, 8)], next=i + 2, src="\\b"))
    else:
        return class_escape_cat(cps, i, n, c)


def class_escape_cat(cps, i, n, c):
    """A category escape, or fall through to a literal escape."""
    cat = cat_of(c)
    if cat != 0:
        return Success(value=ClassItem(pieces=[Program.cat_piece(cat)], next=i + 2, src="\\" + Unicode.utf8_of(c)))
    else:
        return class_escape_lit(cps, i, n, c)


def class_escape_lit(cps, i, n, c):
    """A literal escape inside a character class."""
    match escape_cp(cps, i, n):
        case Error(message=message):
            return Error(message=message)
        case Success(value=value):
            cp = unwrap_or(listGet(value, 0), 0)
            return Success(value=ClassItem(pieces=[Program.range_piece(cp, cp)], next=unwrap_or(listGet(value, 1), n), src="\\" + Unicode.utf8_of(c)))


def parse_class(cps, i, n, icase):
    """Parse `[...]` at `i`; returns [class_instruction, next] or an Error."""
    if i + 1 < n and at(cps, i + 1) == 94:
        return parse_class_items(cps, i, i + 2, n, icase, 1, [])
    else:
        return parse_class_items(cps, i, i + 1, n, icase, 0, [])


def parse_class_items(cps, i, j, n, icase, negate, pieces):
    """Accumulate class pieces until the closing `]`."""
    if j >= n:
        return fail("unterminated character set", i)
    elif at(cps, j) == 93 and len(pieces) > 0:
        return Success(value=ClassAtom(ins=Program.ins_class(negate, icase, pieces), next=j + 1))
    else:
        return parse_class_item(cps, i, j, n, icase, negate, pieces)


def parse_class_item(cps, i, j, n, icase, negate, pieces):
    """Parse one class item, then any range, then continue."""
    match class_item(cps, j, n):
        case Error(message=message):
            return Error(message=message)
        case Success(value=value):
            if value.next < n and at(cps, value.next) == 45:
                return parse_range(cps, i, value.next, n, icase, negate, pieces, value.pieces, value.src)
            else:
                return parse_class_items(cps, i, value.next, n, icase, negate, listConcat(pieces, value.pieces))


def parse_range(cps, i, j, n, icase, negate, pieces, lo_pieces, lo_src):
    """Parse a `lo-hi` range (or a trailing literal `-`) after a class item."""
    if j + 1 < n and at(cps, j + 1) == 93:
        return parse_class_items(cps, i, j + 1, n, icase, negate, listConcat(pieces, listConcat(lo_pieces, [Program.range_piece(45, 45)])))
    else:
        return parse_range_hi(cps, i, j, n, icase, negate, pieces, lo_pieces, lo_src)


def parse_range_hi(cps, i, j, n, icase, negate, pieces, lo_pieces, lo_src):
    """Parse the high endpoint of a class range and emit it."""
    match class_item(cps, j + 1, n):
        case Error(message=message):
            return Error(message=message)
        case Success(value=value):
            lo = single_cp(lo_pieces)
            hi = single_cp(value.pieces)
            if lo < 0 or hi < 0 or hi < lo:
                return fail("bad character range " + lo_src + "-" + value.src, j)
            else:
                return parse_class_items(cps, i, value.next, n, icase, negate, listAppend(pieces, Program.range_piece(lo, hi)))
