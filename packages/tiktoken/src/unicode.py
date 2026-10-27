# osprey: flavor=python
# osprey: namespace=tiktoken
# osprey: module=Unicode
"""Code point classification, iteration, ordering and UTF-8.

The pre-tokenizers classify code points as letter, number, whitespace or
punctuation exactly as the `regex` module (and the Rust core) do: \p{L} and
\p{N} are the general categories, \\s is the White_Space property. The range
tables in `tiktoken.Tables` were generated from that module; outside their
covered blocks a code point classifies as punctuation, which the README
documents. UTF-8 decoding follows CPython's `errors` semantics: `replace`
writes one U+FFFD per maximal subpart of an ill-formed sequence, `ignore`
drops it, `strict` fails. U+0000 cannot appear in an Osprey string, so
decoding it returns Error rather than corrupt the output.
"""

from tiktoken import Tables

__all__ = [
    "at", "at_str", "at_list", "max_int", "min_int", "code_points", "text_of", "cp_len",
    "is_letter", "is_number", "is_space", "is_other", "class_of",
    "utf8_of_cp", "utf8_range", "utf8_text", "decode_utf8", "cmp_text",
    "ints_text",
]


def at(xs, i):
    """Element i of an int list, or 0 past the end: the scanners' safe read."""
    return unwrap_or(listGet(xs, i), 0)


def at_str(xs, i):
    """Element i of a string list, or "" past the end."""
    return unwrap_or(listGet(xs, i), "")


def at_list(xs, i):
    """Element i of a list whose elements are byte lists, or [] past the end."""
    return unwrap_or(listGet(xs, i), [])


def max_int(a, b):
    """The larger of two ints."""
    return a if a > b else b


def min_int(a, b):
    """The smaller of two ints."""
    return a if a < b else b


def code_points(s):
    """The code points of a string, in order."""
    return walk_cps(s, 0, [])


def walk_cps(s, i, acc):
    if i >= len(s):
        return acc
    else:
        cp = unwrap_or(codePointAt(s, i), 63)
        return walk_cps(s, i + unwrap_or(codePointWidth(cp), 1), listAppend(acc, cp))


def cp_len(s):
    """How many code points a string has."""
    return len(code_points(s))


def text_of(cps):
    """The string a code point list spells."""
    return fold(range(len(cps)), "", lambda acc, i: acc + unwrap_or(fromCodePoint(at(cps, i)), "?"))


def in_ranges(ranges, cp, lo, hi):
    """Whether cp lies in one of the sorted inclusive [lo, hi] ranges, by binary search."""
    if lo > hi:
        return False
    else:
        mid = (lo + hi) // 2
        r = unwrap_or(listGet(ranges, mid), [0, 0])
        if cp < at(r, 0):
            return in_ranges(ranges, cp, lo, mid - 1)
        elif cp > at(r, 1):
            return in_ranges(ranges, cp, mid + 1, hi)
        else:
            return True


def is_letter(cp):
    """\\p{L}: a Unicode letter (general category L*)."""
    return in_ranges(Tables.LETTER_RANGES, cp, 0, len(Tables.LETTER_RANGES) - 1)


def is_number(cp):
    """\\p{N}: a Unicode number (general category N*)."""
    return in_ranges(Tables.NUMBER_RANGES, cp, 0, len(Tables.NUMBER_RANGES) - 1)


def is_space(cp):
    """\\s: Unicode White_Space."""
    return in_ranges(Tables.SPACE_RANGES, cp, 0, len(Tables.SPACE_RANGES) - 1)


def is_other(cp):
    """The pre-tokenizers' fourth class: anything but whitespace, letter or number."""
    return not (is_letter(cp) or is_number(cp) or is_space(cp))


def class_of(cp):
    """0 letter, 1 number, 2 space, 3 other."""
    if is_letter(cp):
        return 0
    elif is_number(cp):
        return 1
    elif is_space(cp):
        return 2
    else:
        return 3


def utf8_of_cp(cp, acc):
    """The UTF-8 bytes of one code point, appended to acc."""
    if cp < 128:
        return listAppend(acc, cp)
    elif cp < 2048:
        return listAppend(listAppend(acc, 192 + cp // 64), 128 + cp % 64)
    elif cp < 65536:
        return listAppend(listAppend(listAppend(acc, 224 + cp // 4096), 128 + cp % 4096 // 64), 128 + cp % 64)
    else:
        rem = cp % 262144
        return listAppend(
            listAppend(
                listAppend(listAppend(acc, 240 + cp // 262144), 128 + rem // 4096),
                128 + rem % 4096 // 64,
            ),
            128 + rem % 64,
        )


def utf8_range(cps, start, stop):
    """The UTF-8 bytes of the code points cps[start:stop]."""
    return fold(range(start, stop), [], lambda acc, i: utf8_of_cp(at(cps, i), acc))


def utf8_text(s):
    """The UTF-8 bytes of a string."""
    return utf8_range(code_points(s), 0, cp_len(s))


def ints_text(values):
    """`[1, 2, 3]`: an int list as Python prints it."""
    return "[" + join(toList(map(range(len(values)), lambda i: toString(at(values, i)))), ", ") + "]"


class Chunk:
    """One decode step's output text and the byte it resumes at."""
    text: str
    next: int


def decode_utf8(octets, errors):
    """Decode UTF-8 bytes like CPython: `replace` writes U+FFFD per maximal
    subpart, `ignore` drops them, `strict` errors; unknown handlers error."""
    if errors != "replace" and errors != "ignore" and errors != "strict":
        return Error(message=f"unknown error handler name '{errors}'")
    else:
        return decode_chunks(octets, 0, "", errors)


def decode_chunks(octets, i, acc, errors) -> Result[str, str]:
    if i >= len(octets):
        return Success(value=acc)
    else:
        match decode_one(octets, i, errors):
            case Success(value=value):
                return decode_chunks(octets, value.next, acc + value.text, errors)
            case Error(message=message):
                return Error(message=message)


def decode_one(octets, i, errors):
    """Decode the sequence starting at byte i: one code point, or the maximal
    subpart of an ill-formed one."""
    b = at(octets, i)
    if b < 128:
        return ascii_chunk(b, i)
    elif b >= 194 and b <= 223:
        return decode_two(octets, i, errors)
    elif b >= 224 and b <= 239:
        return decode_three(octets, i, errors)
    elif b >= 240 and b <= 244:
        return decode_four(octets, i, errors)
    else:
        return malformed(octets, i, i + 1, errors)


def ascii_chunk(b, i):
    """A one-byte code point; U+0000 is unrepresentable and errors."""
    if b == 0:
        return Error(message="cannot decode U+0000: an Osprey string cannot hold NUL")
    else:
        return Success(value=Chunk(text=unwrap_or(fromCodePoint(b), "?"), next=i + 1))


def decode_two(octets, i, errors):
    if continuation_at(octets, i + 1):
        cp = (at(octets, i) - 192) * 64 + at(octets, i + 1) - 128
        return Success(value=Chunk(text=unwrap_or(fromCodePoint(cp), "?"), next=i + 2))
    else:
        return malformed(octets, i, i + 1, errors)


def decode_three(octets, i, errors):
    b = at(octets, i)
    if not bounded_at(octets, i + 1, second_lo(b), second_hi(b)):
        return malformed(octets, i, i + 1, errors)
    elif not continuation_at(octets, i + 2):
        return malformed(octets, i, i + 2, errors)
    else:
        cp = ((b - 224) * 64 + at(octets, i + 1) - 128) * 64 + at(octets, i + 2) - 128
        return Success(value=Chunk(text=unwrap_or(fromCodePoint(cp), "?"), next=i + 3))


def decode_four(octets, i, errors):
    b = at(octets, i)
    if not bounded_at(octets, i + 1, second_lo(b), second_hi(b)):
        return malformed(octets, i, i + 1, errors)
    elif not continuation_at(octets, i + 2):
        return malformed(octets, i, i + 2, errors)
    elif not continuation_at(octets, i + 3):
        return malformed(octets, i, i + 3, errors)
    else:
        cp = (((b - 240) * 64 + at(octets, i + 1) - 128) * 64 + at(octets, i + 2) - 128) * 64 + at(octets, i + 3) - 128
        return Success(value=Chunk(text=unwrap_or(fromCodePoint(cp), "?"), next=i + 4))


def continuation_at(octets, i):
    """Whether byte i exists and is a continuation byte (0x80..0xBF)."""
    return i < len(octets) and at(octets, i) >= 128 and at(octets, i) <= 191


def bounded_at(octets, i, lo, hi):
    """Whether byte i exists and lies in lo..hi (a lead byte's valid second byte)."""
    return i < len(octets) and at(octets, i) >= lo and at(octets, i) <= hi


def second_lo(b):
    """The lowest valid second byte for a multi-byte lead."""
    if b == 224:
        return 160
    elif b == 237:
        return 128
    elif b == 240:
        return 144
    else:
        return 128


def second_hi(b):
    """The highest valid second byte for a multi-byte lead."""
    if b == 224:
        return 191
    elif b == 237:
        return 159
    elif b == 240:
        return 191
    elif b == 244:
        return 143
    else:
        return 191


def malformed(octets, start, at_byte, errors):
    """The maximal subpart octets[start:at_byte] is ill-formed: replace it with
    U+FFFD, drop it, or fail, then resume at the offending byte."""
    end = min_int(at_byte, len(octets))
    if errors == "replace":
        return Success(value=Chunk(text="�", next=end))
    elif errors == "ignore":
        return Success(value=Chunk(text="", next=end))
    else:
        return Error(message=f"'utf-8' codec can't decode byte sequence at {start}")


def cmp_text(a, b):
    """-1, 0 or 1: lexicographic order, by UTF-8 bytes (which is also code
    point order). The order the Rust core sorts special tokens in."""
    return cmp_from(a, b, 0)


def cmp_from(a, b, i):
    if i >= len(a) or i >= len(b):
        if i >= len(a) and i >= len(b):
            return 0
        else:
            return -1 if i >= len(a) else 1
    else:
        ca = unwrap_or(byteAt(a, i), 0)
        cb = unwrap_or(byteAt(b, i), 0)
        if ca < cb:
            return -1
        elif ca > cb:
            return 1
        else:
            return cmp_from(a, b, i + 1)
