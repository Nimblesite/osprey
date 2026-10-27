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

__all__ = ["tokenize"]


def valid_token(x):
    """Whether `x` is a non-empty run of `[a-z0-9]`."""
    if isEmpty(x):
        return False
    else:
        return foldList(codepoints(x), True, lambda acc, cp: acc and is_ascii_lower_alnum(cp))


def tokenize(text, use_stemmer):
    """`DefaultTokenizer.tokenize`: lowercase, keep only `[a-z0-9]`, split, stem."""
    lowered = toLowerCase(text)
    spaced = map_codepoints(lowered, lambda cp: 32 if not is_ascii_lower_alnum(cp) else cp)
    raw = words(spaced)
    stemmed = mapList(raw, lambda x: stem(x) if use_stemmer and length(x) > 3 else x)
    return filterList(stemmed, valid_token)
