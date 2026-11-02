# osprey: flavor=python
# osprey: namespace=chromecast
# osprey: module=Hex
"""Bytes as lowercase hex text.

Osprey strings are NUL-terminated, so the wire bytes of the CAST v2 protocol
travel between Osprey and the runtime's TLS session as hex: two characters
per byte, no separator ([FFI-TLS-CLIENT]). This module is the only place
that knows how a byte, a text or a code point is spelled in that form.
"""

__all__ = ["byte_hex", "of_text", "byte_at", "byte_count", "to_text", "code_points"]

DIGITS = "0123456789abcdef"


def digit(value):
    """The hex digit for a value in 0..15."""
    return unwrap_or(substring(DIGITS, value, value + 1), "0")


def byte_hex(b):
    """Two hex digits for a byte value in 0..255."""
    return digit(shiftRightLogical(bitAnd(b, 255), 4)) + digit(bitAnd(b, 15))


def nibble(c):
    """The value of one hex digit given as its byte, for `0-9`, `a-f` or `A-F`."""
    if c >= 97:
        return c - 87
    elif c >= 65:
        return c - 55
    else:
        return c - 48


def text_byte(s, i):
    """Byte `i` of a string, 0 past its end."""
    return unwrap_or(byteAt(s, i), 0)


def of_text(s):
    """The UTF-8 bytes of a string as hex."""
    return fold(range(0, length(s)), "", lambda acc, i: acc + byte_hex(text_byte(s, i)))


def byte_at(hex, i):
    """Byte `i` of a hex text, 0 past its end."""
    return nibble(text_byte(hex, 2 * i)) * 16 + nibble(text_byte(hex, 2 * i + 1))


def byte_count(hex):
    """How many bytes a hex text spells."""
    return intDiv(length(hex), 2)


def width(b):
    """How many bytes the UTF-8 sequence starting with `b` occupies."""
    if b >= 240:
        return 4
    elif b >= 224:
        return 3
    elif b >= 192:
        return 2
    else:
        return 1


def continuation(at, i, w, acc):
    """Folds the `w - 1` continuation bytes after position `i` into `acc`."""
    return fold(range(1, w), acc, lambda cp, k: bitOr(shiftLeft(cp, 6), bitAnd(at(i + k), 63)))


def code_point_at(at, i):
    """The code point of the UTF-8 sequence at byte `i`, read through `at`."""
    b = at(i)
    w = width(b)
    if w == 1:
        return b
    elif w == 2:
        return continuation(at, i, w, bitAnd(b, 31))
    elif w == 3:
        return continuation(at, i, w, bitAnd(b, 15))
    else:
        return continuation(at, i, w, bitAnd(b, 7))


def points_from(at, i, n, acc):
    """Every code point from byte `i` to `n`, appended to `acc`."""
    if i >= n:
        return acc
    else:
        cp = code_point_at(at, i)
        return points_from(at, i + width(at(i)), n, listAppend(acc, cp))


def code_points(s):
    """The code points of a string, in order."""
    return points_from(lambda i: text_byte(s, i), 0, length(s), [])


def to_text(hex):
    """The string whose UTF-8 bytes a hex text spells."""
    points = points_from(lambda i: byte_at(hex, i), 0, byte_count(hex), [])
    return foldList(points, "", lambda acc, cp: acc + unwrap_or(fromCodePoint(cp), ""))
