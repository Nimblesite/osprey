# osprey: flavor=python
# osprey: namespace=sacrebleu
# osprey: module=Text
"""Codepoint primitives shared by the sacrebleu and rouge_score tokenizers.

Osprey strings are UTF-8 byte sequences and `codePointAt` decodes one codepoint
at a byte index. These helpers iterate a string's codepoints and rebuild a
string from them, which is what every tokenizer here does. They live in
sacrebleu (not duplicated) because @osprey/rouge_score depends on
@osprey/sacrebleu.
"""

__all__ = ["codepoints", "from_codepoints", "map_codepoints", "is_ascii_digit", "is_ascii_lower_alnum", "collapse_whitespace", "map_indexed", "segment_references"]


def map_indexed(xs, read):
    """`xs` rebuilt with its indices: element `i` of the result is `read(i)`.

    # Examples
    ```osprey
    from sacrebleu import Text
    indexed = Text.map_indexed([5, 6], lambda i: i)
    print(f"[{toString(indexed)}] [{toString(Text.map_indexed([], lambda i: i))}]")
    ```
    ```output
    [[0, 1]] [[]]
    ```
    """
    return toList(map(range(len(xs)), read))


def codepoints_from(s, i, acc):
    """The codepoints of `s` from byte index `i`, appended to `acc`."""
    if i >= length(s):
        return acc
    else:
        cp = unwrap_or(codePointAt(s, i), 63)
        return codepoints_from(s, i + unwrap_or(codePointWidth(cp), 1), listAppend(acc, cp))


def codepoints(s):
    """The Unicode codepoint value of each character of `s`, in order.

    # Examples
    ```osprey
    from sacrebleu import Text

    def show():
        accented = Text.codepoints("aé")
        print(f"[{toString(accented)}] [{toString(Text.codepoints(""))}]")

    show()
    ```
    ```output
    [[97, 233]] [[]]
    ```
    """
    return codepoints_from(s, 0, [])


def from_codepoints(cps):
    """The string whose codepoints are `cps` (the inverse of `codepoints`).

    # Examples
    ```osprey
    from sacrebleu import Text
    rebuilt = Text.from_codepoints([97, 233])
    blank = Text.from_codepoints([])
    print(f"[{rebuilt}] [{blank}]")
    ```
    ```output
    [aé] []
    ```
    """
    return foldList(cps, "", lambda acc, cp: acc + unwrap_or(fromCodePoint(cp), ""))


def map_codepoints(s, f):
    """`s` with every codepoint replaced by the codepoint `f` returns.

    # Examples
    ```osprey
    from sacrebleu import Text

    def show():
        mapped = Text.map_codepoints("a1!", lambda cp: 120 if cp == 97 else cp)
        print(f"[{mapped}] [{Text.map_codepoints("", lambda cp: cp)}]")

    show()
    ```
    ```output
    [x1!] []
    ```
    """
    return from_codepoints(mapList(codepoints(s), f))


def is_ascii_digit(cp):
    """Whether codepoint `cp` is an ASCII digit `0`-`9`.

    # Examples
    ```osprey
    from sacrebleu import Text
    zero = Text.is_ascii_digit(48)
    nine = Text.is_ascii_digit(57)
    print(f"[{zero}] [{nine}] [{Text.is_ascii_digit(47)}] [{Text.is_ascii_digit(58)}]")
    ```
    ```output
    [true] [true] [false] [false]
    ```
    """
    return cp >= 48 and cp <= 57


def is_ascii_lower_alnum(cp):
    """Whether codepoint `cp` is an ASCII lowercase letter or digit.

    # Examples
    ```osprey
    from sacrebleu import Text
    zed = Text.is_ascii_lower_alnum(122)
    digit = Text.is_ascii_lower_alnum(57)
    print(f"[{zed}] [{digit}] [{Text.is_ascii_lower_alnum(91)}] [{Text.is_ascii_lower_alnum(65)}]")
    ```
    ```output
    [true] [true] [false] [false]
    ```
    """
    return (cp >= 97 and cp <= 122) or (cp >= 48 and cp <= 57)


def collapse_whitespace(s):
    """`s` with runs of ASCII whitespace folded to one space, edges trimmed.

    # Examples
    ```osprey
    from sacrebleu import Text
    folded = Text.collapse_whitespace("  a  b ")
    blank = Text.collapse_whitespace("")
    print(f"[{folded}] [{blank}]")
    ```
    ```output
    [a b] []
    ```
    """
    return join(words(s), " ")


def segment_references(documents):
    """Transpose reference documents to per-segment lists, filling missing segments with empty text.

    # Examples
    ```osprey
    from sacrebleu import Text
    aligned = Text.segment_references([["a", "b"], ["A", "B"]])
    ragged = Text.segment_references([["a"], [], ["A", "B"]])
    print(f"[{toString(aligned)}] [{toString(ragged)}]")
    ```
    ```output
    [[["a", "A"], ["b", "B"]]] [[["a", "", "A"], ["", "", "B"]]]
    ```
    """
    segments = foldList(documents, 0, lambda count, doc: len(doc) if len(doc) > count else count)
    return toList(map(range(segments), lambda i: mapList(documents, lambda doc: unwrap_or(listGet(doc, i), ""))))
