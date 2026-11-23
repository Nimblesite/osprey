# osprey: flavor=python
# osprey: namespace=transformers
# osprey: module=Lists
"""Small shared helpers over `list[int]` used by the logits processors and the
stopping criteria. They live here so `LogitsProcess` and `StoppingCriteria` do
not each carry a private copy. All are total: out-of-range reads fall back to a
sentinel instead of failing."""

from transformers.Math import min_i

__all__ = ["contains_int", "last_n", "map_indexed"]


def map_indexed(xs, read):
    """`xs` rebuilt with its indices: element `i` of the result is `read(i)`.

    # Examples
    ```osprey
    from transformers import Lists
    print(f"[{toString(Lists.map_indexed([4, 4], lambda i: i * 10))}] [{toString(Lists.map_indexed([], lambda i: i))}]")
    ```
    ```output
    [[0, 10]] [[]]
    ```
    """
    return toList(map(range(len(xs)), read))


def contains_int(xs, x):
    """Whether `x` occurs anywhere in `xs`.

    # Examples
    ```osprey
    from transformers import Lists
    print(f"[{Lists.contains_int([1, 4, 7], 4)}] [{Lists.contains_int([1, 4, 7], 3)}] [{Lists.contains_int([], 3)}]")
    ```
    ```output
    [true] [false] [false]
    ```
    """
    return fold(range(len(xs)), False, lambda found, i: found or unwrap_or(listGet(xs, i), x) == x)


def last_n(xs, n):
    """The last `n` elements of `xs` (all of them when n exceeds the length).

    # Examples
    ```osprey
    from transformers import Lists
    print(f"[{toString(Lists.last_n([1, 4, 7], 2))}] [{toString(Lists.last_n([1, 4, 7], 5))}] [{toString(Lists.last_n([], 3))}]")
    ```
    ```output
    [[4, 7]] [[1, 4, 7]] [[]]
    ```
    """
    k = min_i(n, len(xs))
    return toList(map(range(k), lambda i: unwrap_or(listGet(xs, len(xs) - k + i), 0)))

