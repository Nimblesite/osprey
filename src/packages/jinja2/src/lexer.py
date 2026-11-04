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
    return rstrip_from(s, length(s))


def rstrip_from(s, i):
    """Drop whitespace bytes from `s` before index `i`."""
    if i <= 0:
        return ""
    elif is_ws_byte(byte(s, i - 1)):
        return rstrip_from(s, i - 1)
    else:
        return unwrap_or(substring(s, 0, i), "")


def last_newline(s):
    """The byte index of the last `\n` in `s`, or -1."""
    return last_newline_from(s, length(s) - 1)


def last_newline_from(s, i):
    """Scan `s` backwards from `i` for a `\n`."""
    if i < 0:
        return -1
    elif byte(s, i) == 10:
        return i
    else:
        return last_newline_from(s, i - 1)


def all_ws_from(s, start):
    """Whether `s[start:]` is all ASCII whitespace."""
    return all_ws_scan(s, start, length(s))


def all_ws_scan(s, i, n):
    """Whether bytes `i..n-1` are all whitespace."""
    if i >= n:
        return True
    elif is_ws_byte(byte(s, i)):
        return all_ws_scan(s, i + 1, n)
    else:
        return False


def find_tag(source, pos):
    """The next tag opener at or after `pos`, or a `none` head at the end."""
    return find_tag_from(source, pos, length(source))


def find_tag_from(source, i, n):
    """Scan bytes for `{{`, `{%` or `{#` from index `i`."""
    if i >= n - 1:
        return TagHead(pos=n, kind="none", sign="", content_start=n)
    else:
        c = byte(source, i)
        nxt = byte(source, i + 1)
        if c == 123 and (nxt == 123 or nxt == 37 or nxt == 35):
            sign = opener_sign(source, i, n)
            kind = "variable" if nxt == 123 else "block" if nxt == 37 else "comment"
            return TagHead(pos=i, kind=kind, sign=sign, content_start=i + 2 + (1 if sign != "" else 0))
        else:
            return find_tag_from(source, i + 1, n)


def opener_sign(source, i, n):
    """The `-`/`+` whitespace-control sign after a tag opener, or `""`."""
    if i + 2 < n and (byte(source, i + 2) == 45 or byte(source, i + 2) == 43):
        return "-" if byte(source, i + 2) == 45 else "+"
    else:
        return ""


def skip_string(source, i, quote):
    """Skip the string literal opening with `quote` at `i`; returns the byte after it."""
    return skip_string_from(source, i + 1, quote)


def skip_string_from(source, j, quote):
    """Scan a string body, honouring backslash escapes."""
    if j >= length(source):
        return length(source)
    else:
        c = byte(source, j)
        if c == 92:
            return skip_string_from(source, j + 2, quote)
        elif c == quote:
            return j + 1
        else:
            return skip_string_from(source, j + 1, quote)


def parse_tag(source, content_start, i, closer, balance) -> Result[TagEnd, str]:
    """Find a tag's closer, tracking brace balance and skipping strings."""
    if i >= length(source):
        return Error(message="unexpected end of template, expected 'end of print statement'")
    else:
        c = byte(source, i)
        if c == 34 or c == 39:
            return parse_tag(source, content_start, skip_string(source, i, c), closer, balance)
        elif at(source, i, closer) and balance == 0:
            return Success(value=tag_end(source, content_start, i))
        elif c == 123 or c == 40 or c == 91:
            return parse_tag(source, content_start, i + 1, closer, balance + 1)
        elif c == 125 or c == 41 or c == 93:
            return parse_tag(source, content_start, i + 1, closer, decr(balance))
        else:
            return parse_tag(source, content_start, i + 1, closer, balance)


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
