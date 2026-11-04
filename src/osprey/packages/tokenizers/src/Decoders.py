# osprey: flavor=python
# osprey: namespace=tokenizers
# osprey: module=Decoders
"""The decoders: WordPiece (## join plus cleanup), Metaspace (▁ -> space) and
ByteFallback (<0xXX> runs back to UTF-8). Each exposes a `*_decode_chain` that
returns the list of strings a Decoder::decode_chain produces; the Tokenizer
joins that list with the empty separator, matching Decoder::decode.
"""

from tokenizers.Unicode import cps_text

__all__ = [
    "wordpiece_decode", "metaspace_decode_chain", "byte_fallback_decode_chain",
    "utf8_decode",
]


def cps_of(s):
    """The codepoints of a string."""
    return cps_go(s, 0, len(s), [])


def cps_go(s, i, n, acc):
    if i >= n:
        return acc
    else:
        cp = unwrap_or(codePointAt(s, i), 63)
        w = unwrap_or(codePointWidth(cp), 1)
        return cps_go(s, i + w, n, listAppend(acc, cp))


def repl(s, needle, with_):
    return unwrap_or(replace(s, needle, with_), s)


def wordpiece_cleanup(s):
    """The WordPiece decoder's cleanup rewrites."""
    a = repl(s, " .", ".")
    b = repl(a, " ?", "?")
    c = repl(b, " !", "!")
    d = repl(c, " ,", ",")
    e = repl(d, " ' ", "'")
    f = repl(e, " n't", "n't")
    g = repl(f, " 'm", "'m")
    h = repl(g, " do not", " don't")
    i = repl(h, " 's", "'s")
    j = repl(i, " 've", "'ve")
    return repl(j, " 're", "'re")


def wordpiece_decode_chain(tokens, prefix, cleanup):
    """Strip the continuation prefix or prepend a space per token (the first is
    only cleaned), then optionally run cleanup."""
    return fold(range(len(tokens)), [], lambda acc, i: listAppend(acc, wordpiece_one(unwrap_or(listGet(tokens, i), ""), i, prefix, cleanup)))


def wordpiece_one(token, i, prefix, cleanup):
    base = token if i == 0 else (drop(token, len(prefix)) if startsWith(token, prefix) else " " + token)
    return wordpiece_cleanup(base) if cleanup else base


def wordpiece_decode(tokens, prefix, cleanup):
    """The decoded string: decode_chain joined with the empty separator."""
    return join(wordpiece_decode_chain(tokens, prefix, cleanup), "")


def metaspace_decode_chain(tokens, replacement, scheme):
    """Replace the meta char with a space, dropping the leading one unless the
    scheme is never, token by token."""
    return toList(map(range(len(tokens)), lambda i: meta_token(unwrap_or(listGet(tokens, i), ""), i, replacement, scheme)))


def meta_token(token, i, replacement, scheme):
    cps = cps_of(token)
    outs = fold(range(len(cps)), [], lambda acc, k: meta_char(acc, unwrap_or(listGet(cps, k), 32), i, replacement, scheme))
    return join(toList(map(range(len(outs)), lambda k: unwrap_or(fromCodePoint(unwrap_or(listGet(outs, k), 63)), "?"))), "")


def meta_char(acc, cp, i, replacement, scheme):
    if cp == replacement:
        if i == 0 and scheme != 2:
            return acc
        else:
            return listAppend(acc, 32)
    else:
        return listAppend(acc, cp)


def byte_fallback_decode_chain(tokens):
    """Collect runs of <0xXX> tokens into byte strings, decoded as UTF-8 or as
    one replacement character per byte when invalid."""
    return byte_fb_scan(tokens, 0, [], [])


def byte_fb_scan(tokens, i, buf, acc):
    if i >= len(tokens):
        return flush_bytes(buf, acc)
    else:
        t = unwrap_or(listGet(tokens, i), "")
        b = parse_byte_token(t)
        if b >= 0:
            return byte_fb_scan(tokens, i + 1, listAppend(buf, b), acc)
        else:
            return byte_fb_scan(tokens, i + 1, [], listAppend(flush_bytes(buf, acc), t))


def parse_byte_token(t):
    """The byte a <0xXX> token stands for, or -1 when it is not one."""
    if len(t) == 6 and startsWith(t, "<0x") and endsWith(t, ">"):
        hi = hex_digit(unwrap_or(byteAt(t, 3), 0))
        lo = hex_digit(unwrap_or(byteAt(t, 4), 0))
        return hi * 16 + lo if hi >= 0 and lo >= 0 else -1
    else:
        return -1


def hex_digit(b):
    if b >= 48 and b <= 57:
        return b - 48
    elif b >= 65 and b <= 70:
        return b - 65 + 10
    else:
        return -1


def flush_bytes(buf, acc):
    if len(buf) == 0:
        return acc
    else:
        match utf8_decode(buf, 0, ""):
            case Success(value=value):
                return listAppend(acc, value)
            case Error():
                return listConcat(acc, toList(map(range(len(buf)), lambda _: "\ufffd")))


def seq_len(b):
    """How many bytes the sequence starting with lead byte b takes, or -1.

    0xC0 and 0xC1 only ever start an overlong form and 0xF5..0xFF exceed U+10FFFF,
    so they are rejected here; the second-byte bounds that exclude the remaining
    overlongs and the surrogates live in `second_ok`.
    """
    if b < 128:
        return 1
    elif b <= 191:
        return -1
    elif b <= 223:
        return 2 if b >= 194 else -1
    elif b <= 239:
        return 3
    elif b <= 244:
        return 4
    else:
        return -1


def lead_bits(b, n):
    return b if n == 1 else b - 192 if n == 2 else b - 224 if n == 3 else b - 240


def cont_ok(bytes, i, n):
    return fold(range(1, n), True, lambda acc, k: acc and in_cont(unwrap_or(listGet(bytes, i + k), 0)))


def in_cont(b):
    return b >= 128 and b <= 191


def second_ok(lead, second):
    """RFC 3629's bounds on the byte after the lead: they exclude the overlong
    forms of E0, F0 and above and the surrogate range of ED."""
    if lead == 224:
        return second >= 160 and second <= 191
    elif lead == 237:
        return second >= 128 and second <= 159
    elif lead == 240:
        return second >= 144 and second <= 191
    elif lead == 244:
        return second >= 128 and second <= 143
    else:
        return True


class Walk:
    """A left-to-right UTF-8 read: where the next sequence starts, the code points
    so far and the index of the first bad sequence (-1 while there is none)."""
    next: int
    cps: list[int]
    bad: int


def utf8_decode(bytes, i, acc) -> Result[str, str]:
    """bytes[i:] decoded as UTF-8 and appended to acc, Error on a malformed sequence.

    A fold, not a recursion: one frame per code point overflows the stack on the
    long text the byte-level decoder feeds it (safetensors' port had the same bug).
    """
    walk = fold(range(i, len(bytes)), Walk(next=i, cps=[], bad=-1), lambda st, k: utf8_step(bytes, st, k))
    if walk.bad >= 0:
        return Error(message="bad utf8 at byte " + toString(walk.bad))
    else:
        return Success(value=acc + cps_text(walk.cps))


def utf8_step(bytes, st, k):
    """Read the sequence at byte k when one starts there; skip continuation bytes."""
    if st.bad >= 0 or k < st.next:
        return st
    else:
        return utf8_sequence(bytes, st, k)


def utf8_sequence(bytes, st, k):
    lead = unwrap_or(listGet(bytes, k), 0)
    width = seq_len(lead)
    if width < 1 or k + width > len(bytes) or not cont_ok(bytes, k, width) or not second_ok(lead, unwrap_or(listGet(bytes, k + 1), 0)):
        return Walk(next=st.next, cps=st.cps, bad=k)
    else:
        return Walk(next=k + width, cps=listAppend(st.cps, code_point(bytes, k, lead, width)), bad=-1)


def code_point(bytes, i, lead, n):
    """The code point a checked n-byte sequence from byte i spells."""
    return fold(range(1, n), lead_bits(lead, n), lambda code, k: code * 64 + unwrap_or(listGet(bytes, i + k), 128) - 128)
