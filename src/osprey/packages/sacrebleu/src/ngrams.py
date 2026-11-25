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
    """The count of `key` in `counter`, or 0 when absent.

    # Examples
    ```osprey
    from sacrebleu import Ngrams
    counter = {"a": 2, "b": 1}
    present = Ngrams.counter_get(counter, "a")
    absent = Ngrams.counter_get(counter, "z")
    blank = Ngrams.counter_get({}, "a")
    print(f"[{present}] [{absent}] [{blank}]")
    ```
    ```output
    [2] [0] [0]
    ```
    """
    return unwrap_or(mapGet(counter, key), 0)


def counter_increment(counter, key):
    """`counter` with `key` counted once more.

    # Examples
    ```osprey
    from sacrebleu import Ngrams

    def show():
        grown = Ngrams.counter_increment({"x": 1}, "x")
        seeded = Ngrams.counter_increment({}, "new")
        print(f"[{Ngrams.counter_get(grown, "x")}] [{Ngrams.counter_get(seeded, "new")}]")

    wrappingArithmetic(show)
    ```
    ```output
    [2] [1]
    ```
    """
    return mapSet(counter, key, counter_get(counter, key) + 1)


def ngram_key(tokens, i, order):
    """The joined order-`order` word n-gram starting at token `i`."""
    return join(toList(map(range(order), lambda k: unwrap_or(listGet(tokens, i + k), ""))), " ")


def char_key(cps, i, order):
    """The order-`order` character n-gram starting at codepoint `i`."""
    return from_codepoints(toList(map(range(order), lambda k: unwrap_or(listGet(cps, i + k), 32))))


def word_ngrams_from_tokens(tokens, order):
    """Counter of order-`order` word n-grams of `tokens`, joined by spaces.

    # Examples
    ```osprey
    from sacrebleu import Ngrams

    def show():
        counts = Ngrams.word_ngrams_from_tokens(["the", "cat", "the"], 2)
        pair = Ngrams.counter_get(counts, "the cat")
        backward = Ngrams.counter_get(counts, "cat the")
        blank = Ngrams.counter_total(Ngrams.word_ngrams_from_tokens([], 2))
        print(f"[{pair}] [{backward}] [{blank}]")

    wrappingArithmetic(show)
    ```
    ```output
    [1] [1] [0]
    ```
    """
    return fold(range(len(tokens) - order + 1), {},
                lambda acc, i: counter_increment(acc, ngram_key(tokens, i, order)))


def word_ngrams(line, min_order, max_order):
    """Counter of all word n-grams of orders `min_order`..`max_order` of `line`.
    Orders longer than the line contribute nothing.

    # Examples
    ```osprey
    from sacrebleu import Ngrams

    def show():
        counts = Ngrams.word_ngrams("the cat the", 1, 2)
        single = Ngrams.counter_get(counts, "the")
        bigram = Ngrams.counter_get(counts, "cat the")
        over = Ngrams.counter_total(Ngrams.word_ngrams("cat", 2, 3))
        print(f"[{single}] [{bigram}] [{over}]")

    wrappingArithmetic(show)
    ```
    ```output
    [2] [1] [0]
    ```
    """
    tokens = words(line)
    return fold(range(min_order, max_order + 1), {},
                lambda acc, n: fold(range(len(tokens) - n + 1), acc,
                                    lambda a2, i: counter_increment(a2, ngram_key(tokens, i, n))))


def char_ngrams(line, order, include_whitespace):
    """Counter of order-`order` character n-grams; whitespace kept only when
    `include_whitespace` is true.

    # Examples
    ```osprey
    from sacrebleu import Ngrams

    def show():
        plain = Ngrams.char_ngrams("abc", 2, False)
        spaced = Ngrams.char_ngrams("a b", 2, True)
        stripped = Ngrams.char_ngrams("a b", 2, False)
        ab = Ngrams.counter_get(plain, "ab")
        a_space = Ngrams.counter_get(spaced, "a ")
        joined = Ngrams.counter_get(stripped, "ab")
        print(f"[{ab}] [{a_space}] [{joined}]")

    wrappingArithmetic(show)
    ```
    ```output
    [1] [1] [1]
    ```
    """
    stripped = line if include_whitespace else join(words(line), "")
    cps = codepoints(stripped)
    return fold(range(len(cps) - order + 1), {},
                lambda acc, i: counter_increment(acc, char_key(cps, i, order)))


def counter_total(counter):
    """The sum of every count in `counter`.

    # Examples
    ```osprey
    from sacrebleu import Ngrams

    def show():
        total = Ngrams.counter_total({"a": 2, "b": 3})
        blank = Ngrams.counter_total({})
        print(f"[{total}] [{blank}]")

    wrappingArithmetic(show)
    ```
    ```output
    [5] [0]
    ```
    """
    return foldList(mapValues(counter), 0, lambda acc, v: acc + v)


def counter_matched(hyp, ref):
    """Sum of `min(hyp[n], ref[n])` over every n-gram in `hyp`.

    # Examples
    ```osprey
    from sacrebleu import Ngrams

    def show():
        hyp = Ngrams.word_ngrams("cat dog", 1, 1)
        ref = Ngrams.word_ngrams("cat fish", 1, 1)
        matched = Ngrams.counter_matched(hyp, ref)
        none = Ngrams.counter_matched(Ngrams.word_ngrams("x", 1, 1), Ngrams.word_ngrams("y", 1, 1))
        print(f"[{matched}] [{none}]")

    wrappingArithmetic(show)
    ```
    ```output
    [1] [0]
    ```
    """
    return foldList(mapKeys(hyp), 0, lambda acc, key: acc + min(counter_get(hyp, key), counter_get(ref, key)))


def counter_merge_max(a, b):
    """The per-key maximum of two counters (multiple-reference merge).

    # Examples
    ```osprey
    from sacrebleu import Ngrams
    merged = Ngrams.counter_merge_max({"a": 2}, {"a": 3, "b": 1})
    kept = Ngrams.counter_get(merged, "a")
    added = Ngrams.counter_get(merged, "b")
    print(f"[{kept}] [{added}]")
    ```
    ```output
    [3] [1]
    ```
    """
    return foldList(mapKeys(b), a, lambda acc, key: mapSet(acc, key, max(counter_get(acc, key), counter_get(b, key))))
