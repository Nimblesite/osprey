# osprey: flavor=python
# osprey: namespace=rouge_score
# osprey: module=Tokenize
"""rouge_score's default tokenizer (`tokenize.tokenize`).

Lowercases, replaces every run of non `[a-z0-9]` with a space, splits on
whitespace, optionally Porter-stems words longer than three characters, and
drops empty or invalid tokens. `use_stemmer` selects the NLTK Porter stemmer.
"""

from sacrebleu.Text import codepoints, is_ascii_lower_alnum, map_codepoints
from rouge_score.Stemmer import stem

__all__ = ["wrapping", "tokenize"]


@handler(Arith)
class wrapping:
    """The arithmetic policy the examples run under: an overflow answers the
    two's-complement wrapped value, a division or remainder by zero answers zero.

    # Examples
    ```osprey
    from rouge_score import Tokenize

    def show(top, zero):
        with Tokenize.wrapping:
            print(f"[{top + 1}] [{3.0 / toFloat(zero)}] [{7 % zero}]")

    show(9223372036854775807, 0)
    ```
    ```output
    [-9223372036854775808] [0.0] [0]
    ```
    """
    def overflow(_op, _l, _r, wrapped):
        return wrapped

    def divideByZero(_op, _l):
        return 0.0

    def remainderByZero(_l):
        return 0


def valid_token(x):
    """Whether `x` is a non-empty run of `[a-z0-9]`."""
    if isEmpty(x):
        return False
    else:
        return foldList(codepoints(x), True, lambda acc, cp: acc and is_ascii_lower_alnum(cp))


def tokenize(text, use_stemmer):
    """`DefaultTokenizer.tokenize`: lowercase, keep only `[a-z0-9]`, split, stem.

    # Examples
    ```osprey
    from rouge_score import Tokenize

    def show():
        with Tokenize.wrapping:
            plain = Tokenize.tokenize("Hello, World! 42", False)
            stemmed = Tokenize.tokenize("running runs", True)
            empty = Tokenize.tokenize("", True)
            print(f"[{plain}] [{stemmed}] [{empty}]")

    show()
    ```
    ```output
    [["hello", "world", "42"]] [["run", "run"]] [[]]
    ```
    """
    lowered = toLowerCase(text)
    spaced = map_codepoints(lowered, lambda cp: 32 if not is_ascii_lower_alnum(cp) else cp)
    raw = words(spaced)
    stemmed = mapList(raw, lambda x: stem(x) if use_stemmer and length(x) > 3 else x)
    return filterList(stemmed, valid_token)
