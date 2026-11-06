# osprey: flavor=python
# osprey: namespace=jinja2
# osprey: module=Lexer
"""The template lexer: turns template text into data/variable/block chunks with
Jinja's exact whitespace rules.

Newlines are normalized to `\n` and one trailing newline is dropped
(keep_trailing_newline=False). `{{-`/`{{` and `-}}`/`}}` control whitespace on
either side of a tag; `{%` and `{#` additionally honour trim_blocks (strip one
newline after the tag) and lstrip_blocks (strip a line's leading whitespace
before the tag). String literals, dict braces and brackets are skipped when
finding a tag's end, so `{{ "}}" }}` lexes as one variable.
"""


__all__ = ["Chunk", "TagHead", "TagEnd", "Consumed", "scan"]


class Chunk:
    """One lexed piece: literal text, a `{{ ... }}` body, or a `{% ... %}` body."""
    kind: str
    text: str


class TagHead:
    """A tag opener found in the text: its kind, sign and where its content starts."""
    pos: int
    kind: str
    sign: str
    content_start: int


class TagEnd:
    """A tag's parsed body, the byte after its closer, and its closing sign."""
    content: str
    end: int
    rsign: str


class Consumed:
    """The whitespace a closer consumed: the new position and whether it ended in a newline."""
    pos: int
    ended_nl: bool


class Find:
    """A tag search: the byte to read next and the opener's offset (-1 until found)."""
    pos: int
    at: int


class Skip:
    """A quoted-string skip: the byte to read next and whether the closing quote was passed."""
    pos: int
    done: bool


class TagScan:
    """A tag scan: the byte to read next, the bracket balance, the closer's offset and
    whether the tag has ended."""
    pos: int
    balance: int
    end: int
    done: bool


def byte(source, i):
    """The byte at `i`, or 0 past the end."""
    return unwrap_or(byteAt(source, i), 0)


def at(source, i, text):
    """Whether `source` has `text` at byte offset `i`."""
    return unwrap_or(substring(source, i, i + length(text)), "") == text


def norm(s):
    """Normalize newlines and drop one trailing newline, as Jinja's lexer does."""
    s1 = unwrap_or(replace(s, "\r\n", "\n"), s)
    s2 = unwrap_or(replace(s1, "\r", "\n"), s1)
    if s2 != "" and endsWith(s2, "\n"):
        return unwrap_or(substring(s2, 0, length(s2) - 1), s2)
    else:
        return s2


def is_ws_byte(b):
    """Whether byte `b` is ASCII whitespace."""
    return b == 32 or b == 9 or b == 10 or b == 13 or b == 12 or b == 11


def rstrip(s):
    """`s` without trailing ASCII whitespace."""
    return trimEnd(s)


def last_newline(s):
    """The byte index of the last `\n` in `s`, or -1."""
    return byte_offset(length(s), last_part(s))


def last_part(s):
    """The text after `s`'s last `\n` (all of `s` when it has none)."""
    match split(s, "\n"):
        case Success(value=parts):
            return unwrap_or(listGet(parts, len(parts) - 1), s)
        case Error(_message):
            return s


def byte_offset(n, tail):
    """The index of the `\n` that precedes a `tail` of byte length `n`."""
    return n - length(tail) - 1


def all_ws_from(s, start):
    """Whether `s[start:]` is all ASCII whitespace."""
    return trimStart(drop(s, start)) == ""


def find_tag(source, pos):
    """The next tag opener at or after `pos`, or a `none` head at the end."""
    st = fold(range(length(source) - pos), Find(pos=pos, at=-1), lambda state, _k: find_step(source, state))
    return head_of(source, st.at)


def find_step(source, st):
    """Record the opener at the scan's byte, or advance one byte."""
    if st.at >= 0 or st.pos >= length(source) - 1:
        return st
    else:
        return Find(pos=st.pos + 1, at=st.pos if opens_tag(source, st.pos) else -1)


def opens_tag(source, i):
    """Whether byte `i` starts `{{`, `{%` or `{#`."""
    nxt = byte(source, i + 1)
    return byte(source, i) == 123 and (nxt == 123 or nxt == 37 or nxt == 35)


def head_of(source, at):
    """The opener head at `at`, or the `none` head at the end of the text."""
    if at < 0:
        return TagHead(pos=length(source), kind="none", sign="", content_start=length(source))
    else:
        nxt = byte(source, at + 1)
        sign = opener_sign(source, at, length(source))
        kind = "variable" if nxt == 123 else "block" if nxt == 37 else "comment"
        return TagHead(pos=at, kind=kind, sign=sign, content_start=at + 2 + (1 if sign != "" else 0))


def opener_sign(source, i, n):
    """The `-`/`+` whitespace-control sign after a tag opener, or `""`."""
    if i + 2 < n and (byte(source, i + 2) == 45 or byte(source, i + 2) == 43):
        return "-" if byte(source, i + 2) == 45 else "+"
    else:
        return ""


def skip_string(source, i, quote):
    """Skip the string literal opening with `quote` at `i`; returns the byte after it.
    One fold over the body's bytes, so a long literal costs no stack."""
    first = i + 1
    st = fold(range(length(source) - first), Skip(pos=first, done=False), lambda state, _k: skip_step(source, state, quote))
    return length(source) if st.pos > length(source) else st.pos


def skip_step(source, st, quote):
    """Read one byte of a quoted string, honouring backslash escapes."""
    if st.done:
        return st
    else:
        c = byte(source, st.pos)
        if c == 92:
            return Skip(pos=st.pos + 2, done=False)
        elif c == quote:
            return Skip(pos=st.pos + 1, done=True)
        else:
            return Skip(pos=st.pos + 1, done=False)


def parse_tag(source, content_start, i, closer, balance) -> Result[TagEnd, str]:
    """Find a tag's closer, tracking brace balance and skipping strings. One fold
    over the bytes, so a long literal inside a tag costs no stack."""
    st = fold(range(length(source) - i), TagScan(pos=i, balance=balance, end=-1, done=False), lambda state, _k: tag_step(source, state, closer))
    if st.done:
        return Success(value=tag_end(source, content_start, st.end))
    else:
        return Error(message="unexpected end of template, expected 'end of print statement'")


def tag_step(source, st, closer):
    """Read one byte of a tag: skip a string, count a bracket, or stop at the closer."""
    if st.done:
        return st
    else:
        return tag_byte(source, st, closer)


def tag_byte(source, st, closer):
    """Apply one byte's meaning to the tag scan."""
    c = byte(source, st.pos)
    if c == 34 or c == 39:
        return TagScan(pos=skip_string(source, st.pos, c), balance=st.balance, end=-1, done=False)
    elif at(source, st.pos, closer) and st.balance == 0:
        return TagScan(pos=st.pos, balance=st.balance, end=st.pos, done=True)
    elif c == 123 or c == 40 or c == 91:
        return TagScan(pos=st.pos + 1, balance=st.balance + 1, end=-1, done=False)
    elif c == 125 or c == 41 or c == 93:
        return TagScan(pos=st.pos + 1, balance=decr(st.balance), end=-1, done=False)
    else:
        return TagScan(pos=st.pos + 1, balance=st.balance, end=-1, done=False)


def decr(balance):
    """Decrement a brace balance, clamped at zero."""
    if balance <= 0:
        return 0
    else:
        return balance - 1


def tag_end(source, content_start, i):
    """The closer at `i`: its content and closing sign (`-`/`+` trim the content)."""
    if i > content_start and byte(source, i - 1) == 45:
        return TagEnd(content=unwrap_or(substring(source, content_start, i - 1), ""), end=i + 2, rsign="-")
    elif i > content_start and byte(source, i - 1) == 43:
        return TagEnd(content=unwrap_or(substring(source, content_start, i - 1), ""), end=i + 2, rsign="+")
    else:
        return TagEnd(content=unwrap_or(substring(source, content_start, i), ""), end=i + 2, rsign="")


def lstrip_text(text, head, lstrip_blocks):
    """Apply the opener's whitespace control to the data before a tag."""
    if head.sign == "-":
        return rstrip(text)
    elif head.sign != "+" and lstrip_blocks and head.kind != "variable":
        lpos = last_newline(text) + 1
        if (lpos > 0 or text != "") and all_ws_from(text, lpos):
            return unwrap_or(substring(text, 0, lpos), text)
        else:
            return text
    else:
        return text


def consume_end(source, i, rsign, kind, trim_blocks):
    """Consume the whitespace a closer controls; reports the new position and
    whether the consumed run ended in a newline."""
    if rsign == "-":
        j = skip_ws(source, i)
        return Consumed(pos=j, ended_nl=(j > i and byte(source, j - 1) == 10))
    elif trim_blocks and kind != "variable" and byte(source, i) == 10:
        return Consumed(pos=i + 1, ended_nl=True)
    else:
        return Consumed(pos=i, ended_nl=False)


def skip_ws(source, i):
    """Skip ASCII whitespace bytes from `i`."""
    if i >= length(source):
        return i
    elif is_ws_byte(byte(source, i)):
        return skip_ws(source, i + 1)
    else:
        return i


def scan(template, trim_blocks, lstrip_blocks) -> Result[list[Chunk], str]:
    """Lex `template` into chunks."""
    return scan_from(norm(template), trim_blocks, lstrip_blocks, 0, [], True)


def scan_from(src, tb, lb, pos, chunks, at_line_start) -> Result[list[Chunk], str]:
    """The lexing loop: emit the data before the next tag, then the tag itself."""
    n = length(src)
    if pos >= n:
        return Success(value=chunks)
    else:
        head = find_tag(src, pos)
        if head.kind == "none":
            return Success(value=listAppend(chunks, Chunk(kind="data", text=unwrap_or(substring(src, pos, n), ""))))
        else:
            text = lstrip_text(unwrap_or(substring(src, pos, head.pos), ""), head, lb)
            chunks2 = listAppend(chunks, Chunk(kind="data", text=text)) if text != "" else chunks
            closer = "}}" if head.kind == "variable" else "%}" if head.kind == "block" else "#}"
            return scan_tag(src, tb, lb, head, closer, chunks2)


def scan_tag(src, tb, lb, head, closer, chunks2) -> Result[list[Chunk], str]:
    """Parse one tag and continue the lexing loop."""
    match parse_tag(src, head.content_start, head.content_start, closer, 0):
        case Success(value=value):
            c = consume_end(src, value.end, value.rsign, head.kind, tb)
            return scan_from(src, tb, lb, c.pos, append_tag(chunks2, head.kind, value.content), c.ended_nl)
        case Error(message=message):
            return Error(message=message)


def append_tag(chunks, kind, content):
    """Append a variable/block chunk (comments produce nothing)."""
    if kind == "variable":
        return listAppend(chunks, Chunk(kind="variable", text=trim(content)))
    elif kind == "block":
        return listAppend(chunks, Chunk(kind="block", text=trim(content)))
    else:
        return chunks
