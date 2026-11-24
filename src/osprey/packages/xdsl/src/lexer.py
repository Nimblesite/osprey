# osprey: flavor=python
# osprey: namespace=xdsl
# osprey: module=Lexer
"""The MLIR lexer: source text to tokens.

A port of `xdsl/utils/mlir_lexer.py` (`MLIRLexer`): the same token kinds, the
same identifier, number and string grammars, and `//` comments. xDSL lexes on
demand from a cursor; this port lexes one line at a time and returns the whole
list, because no MLIR token spans a line. Each token keeps its 1-based line
for error messages.
"""

__all__ = ["Token", "tokens", "at", "cut", "is_digit", "BARE", "AT", "HASH", "PERCENT", "CARET", "BANG", "INT", "FLOAT", "STRING", "PUNCT", "EOF"]

BARE = "bare-ident"
"""`bare-id ::= (letter|[_]) (letter|digit|[_$.])*`."""
AT = "at-ident"
"""``at-ident ::= `@` (bare-id | string-literal)``."""
HASH = "hash-ident"
"""``hash-ident ::= `#` suffix-id``."""
PERCENT = "percent-ident"
"""``percent-ident ::= `%` suffix-id``."""
CARET = "caret-ident"
"""``caret-ident ::= `^` suffix-id``."""
BANG = "exclamation-ident"
"""``exclamation-ident ::= `!` suffix-id``."""
INT = "integer"
"""A decimal or `0x` hexadecimal integer literal."""
FLOAT = "float"
"""A decimal literal with a fraction."""
STRING = "string"
"""A double-quoted string literal, quotes included."""
PUNCT = "punctuation"
"""Punctuation: `-> : , ... = > { ( [ < - + ? / } ) ] * |`."""
EOF = "eof"
"""The end of the input."""


class Token:
    """One token: its kind, its exact source text and its 1-based line."""
    kind: str
    text: str
    line: int


def at(text, i):
    """The byte at `i`, or 0 past the end."""
    return unwrap_or(byteAt(text, i), 0)


def cut(text, start, end):
    """The bytes `[start, end)` of `text`."""
    return unwrap_or(substring(text, start, end), "")


def is_alpha(b):
    """An ASCII letter."""
    return (b >= 65 and b <= 90) or (b >= 97 and b <= 122)


def is_digit(b):
    """An ASCII digit."""
    return b >= 48 and b <= 57


def is_hex(b):
    """An ASCII hexadecimal digit."""
    return is_digit(b) or (b >= 65 and b <= 70) or (b >= 97 and b <= 102)


def is_bare_suffix(b):
    """`letter | digit | [_$.]`: what continues a bare identifier."""
    return is_alpha(b) or is_digit(b) or b == 95 or b == 36 or b == 46


def is_suffix_start(b):
    """`letter | [$._-]`: what starts a named suffix identifier."""
    return is_alpha(b) or b == 36 or b == 46 or b == 95 or b == 45


def run_end(text, i, accepts):
    """The first index from `i` whose byte `accepts` refuses."""
    if i < len(text) and accepts(at(text, i)):
        return run_end(text, wrapAdd(i, 1), accepts)
    else:
        return i


def suffix_end(text, i):
    """The end of `suffix-id ::= digit+ | (letter|[$._-]) (letter|[$._-]|digit)*` at `i`, or `i`."""
    if is_digit(at(text, i)):
        return run_end(text, i, is_digit)
    elif is_suffix_start(at(text, i)):
        return run_end(text, i, lambda b: is_suffix_start(b) or is_digit(b))
    else:
        return i


def string_end(text, i) -> Result[int, str]:
    """The index after the quote closing the string whose body starts at `i`."""
    if i >= len(text):
        return Error(message="End of file reached before closing string literal.")
    elif at(text, i) == 34:
        return Success(value=wrapAdd(i, 1))
    elif at(text, i) == 92:
        return string_end(text, wrapAdd(i, 2))
    else:
        return string_end(text, wrapAdd(i, 1))


def number_end(text, i):
    """The end of the number whose first digit is at `i`: hexadecimal, decimal or float."""
    if at(text, i) == 48 and at(text, wrapAdd(i, 1)) == 120 and is_hex(at(text, wrapAdd(i, 2))):
        return run_end(text, wrapAdd(i, 2), is_hex)
    else:
        return fraction_end(text, run_end(text, i, is_digit))


def fraction_end(text, i):
    """The end of an optional `.digits` and exponent starting at `i`."""
    if at(text, i) == 46:
        return exponent_end(text, run_end(text, wrapAdd(i, 1), is_digit))
    else:
        return i


def exponent_end(text, i):
    """The end of an optional `[eE][+-]?digits` starting at `i`."""
    signed = wrapAdd(i, 2) if at(text, wrapAdd(i, 1)) == 43 or at(text, wrapAdd(i, 1)) == 45 else wrapAdd(i, 1)
    if (at(text, i) == 101 or at(text, i) == 69) and is_digit(at(text, signed)):
        return run_end(text, signed, is_digit)
    else:
        return i


def is_single(b):
    """A one-character punctuation token: `: , ( ) { } [ ] < > = + / * ? |`."""
    return b == 58 or b == 44 or b == 40 or b == 41 or b == 123 or b == 125 or b == 91 or b == 93 or b == 60 or b == 62 or b == 61 or b == 43 or b == 47 or b == 42 or b == 63 or b == 124


def prefixed_kind(b):
    """The token kind a prefix character introduces."""
    if b == 35:
        return HASH
    elif b == 33:
        return BANG
    elif b == 94:
        return CARET
    else:
        return PERCENT


def emit(text, line, start, end, kind, done) -> Result[list[Token], str]:
    """Record the token `[start, end)` and lex what follows it."""
    return lex(text, line, end, listAppend(done, Token(kind=kind, text=cut(text, start, end), line=line)))


def lex(text, line, i, done) -> Result[list[Token], str]:
    """The tokens of `text` from byte `i`, after those already `done`."""
    b = at(text, i)
    if i >= len(text) or (b == 47 and at(text, wrapAdd(i, 1)) == 47):
        return Success(value=done)
    elif b == 32 or b == 9 or b == 13:
        return lex(text, line, wrapAdd(i, 1), done)
    elif is_alpha(b) or b == 95:
        return emit(text, line, i, run_end(text, wrapAdd(i, 1), is_bare_suffix), BARE, done)
    else:
        return lex_symbol(text, line, i, b, done)


def lex_symbol(text, line, i, b, done) -> Result[list[Token], str]:
    """A token that does not start with a letter."""
    if b == 45 and at(text, wrapAdd(i, 1)) == 62:
        return emit(text, line, i, wrapAdd(i, 2), PUNCT, done)
    elif b == 46 and at(text, wrapAdd(i, 1)) == 46 and at(text, wrapAdd(i, 2)) == 46:
        return emit(text, line, i, wrapAdd(i, 3), PUNCT, done)
    elif is_single(b) or b == 45:
        return emit(text, line, i, wrapAdd(i, 1), PUNCT, done)
    elif b == 64:
        return lex_at(text, line, i, done)
    elif b == 35 or b == 33 or b == 94 or b == 37:
        return lex_prefixed(text, line, i, b, done)
    else:
        return lex_literal(text, line, i, b, done)


def lex_at(text, line, i, done) -> Result[list[Token], str]:
    """``at-ident ::= `@` (bare-id | string-literal)``."""
    b = at(text, wrapAdd(i, 1))
    if is_alpha(b) or b == 95:
        return emit(text, line, i, run_end(text, wrapAdd(i, 2), is_bare_suffix), AT, done)
    elif b == 34:
        return andThen(string_end(text, wrapAdd(i, 2)), lambda end: emit(text, line, i, end, AT, done))
    else:
        return Error(message=f"line {line}: @ identifier expected to start with letter, '_', or '\"'.")


def lex_prefixed(text, line, i, b, done) -> Result[list[Token], str]:
    """A `#`, `!`, `^` or `%` identifier."""
    end = suffix_end(text, wrapAdd(i, 1))
    if end == wrapAdd(i, 1):
        return Error(message=f"line {line}: Expected suffix identifier after {cut(text, i, wrapAdd(i, 1))}")
    else:
        return emit(text, line, i, end, prefixed_kind(b), done)


def lex_literal(text, line, i, b, done) -> Result[list[Token], str]:
    """A string or a number, or the character no token starts with."""
    if b == 34:
        return andThen(string_end(text, wrapAdd(i, 1)), lambda end: emit(text, line, i, end, STRING, done))
    elif is_digit(b):
        end = number_end(text, i)
        kind = FLOAT if contains(cut(text, i, end), ".") and at(text, wrapAdd(i, 1)) != 120 else INT
        return emit(text, line, i, end, kind, done)
    else:
        return Error(message=f"line {line}: Unexpected character: {cut(text, i, wrapAdd(i, 1))}")


def lex_lines(lines, number, done) -> Result[list[Token], str]:
    """The tokens of every line from `number` (1-based) on."""
    if number > len(lines):
        return Success(value=listAppend(done, Token(kind=EOF, text="", line=number)))
    else:
        return andThen(lex(unwrap_or(listGet(lines, wrapSub(number, 1)), ""), number, 0, done), lambda more: lex_lines(lines, wrapAdd(number, 1), more))


def tokens(source) -> Result[list[Token], str]:
    """Every token of `source`, ending with one `EOF` token."""
    return lex_lines(unwrap_or(split(source, "\n"), []), 1, [])
