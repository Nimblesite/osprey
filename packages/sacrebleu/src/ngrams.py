# osprey: flavor=python
# osprey: namespace=sacrebleu
# osprey: module=Ngrams
"""Word and character n-gram counters.

A counter is a `Map[string, int]` keyed by the n-gram text: word n-grams join
their tokens with a single space (tokens never contain whitespace, so the join
is unambiguous), and character n-grams are substrings of the (possibly
whitespace-free) line. This is the `Counter` of `sacrebleu.metrics.helpers`.
"""

from sacrebleu.Text import codepoints, from_codepoints

__all__ = ["word_ngrams", "word_ngrams_from_tokens", "char_ngrams", "counter_total", "counter_matched", "counter_merge_max", "counter_get", "counter_increment"]


def counter_get(counter, key):
    """The count of `key` in `counter`, or 0 when absent."""
    return unwrap_or(mapGet(counter, key), 0)


def counter_increment(counter, key):
    """`counter` with `key` counted once more."""
    return mapSet(counter, key, counter_get(counter, key) + 1)


def min_i(a, b):
    """The smaller of two ints."""
    return a if a < b else b


def max_i(a, b):
    """The larger of two ints."""
    return a if a > b else b


def ngram_key(tokens, i, order):
    """The joined order-`order` word n-gram starting at token `i`."""
    return join(toList(map(range(order), lambda k: unwrap_or(listGet(tokens, i + k), ""))), " ")


def char_key(cps, i, order):
    """The order-`order` character n-gram starting at codepoint `i`."""
    return from_codepoints(toList(map(range(order), lambda k: unwrap_or(listGet(cps, i + k), 32))))


def word_ngrams_from_tokens(tokens, order):
    """Counter of order-`order` word n-grams of `tokens`, joined by spaces."""
    return fold(range(len(tokens) - order + 1), {},
                lambda acc, i: counter_increment(acc, ngram_key(tokens, i, order)))


def word_ngrams(line, min_order, max_order):
    """Counter of all word n-grams of orders `min_order`..`max_order` of `line`."""
    tokens = words(line)
    return fold(range(min_order, max_order + 1), {},
                lambda acc, n: fold(range(len(tokens) - n + 1), acc,
                                    lambda a2, i: counter_increment(a2, ngram_key(tokens, i, n))))


def char_ngrams(line, order, include_whitespace):
    """Counter of order-`order` character n-grams; whitespace kept only when
    `include_whitespace` is true."""
    stripped = line if include_whitespace else join(words(line), "")
    cps = codepoints(stripped)
    return fold(range(len(cps) - order + 1), {},
                lambda acc, i: counter_increment(acc, char_key(cps, i, order)))


def counter_total(counter):
    """The sum of every count in `counter`."""
    return foldList(mapValues(counter), 0, lambda acc, v: acc + v)


def counter_matched(hyp, ref):
    """Sum of `min(hyp[n], ref[n])` over every n-gram in `hyp`."""
    return foldList(mapKeys(hyp), 0, lambda acc, key: acc + min_i(counter_get(hyp, key), counter_get(ref, key)))


def counter_merge_max(a, b):
    """The per-key maximum of two counters (multiple-reference merge)."""
    return foldList(mapKeys(b), a, lambda acc, key: mapSet(acc, key, max_i(counter_get(acc, key), counter_get(b, key))))
