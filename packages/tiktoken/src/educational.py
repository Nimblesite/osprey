# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
# osprey: namespace=tiktoken
# osprey: module=Educational
"""`tiktoken._educational`: SimpleBytePairEncoding, bpe_encode and bpe_train.

The educational codec is an Encoding with no special tokens, so the port
spells it as a type alias over `Core.Encoding` and its methods delegate to
Core's; `bpe_encode` is the same merge loop as the core's. `bpe_train` is
tiktoken's exact algorithm: the 256 single-byte ranks first, then repeatedly
the most common adjacent pair becomes the next token (the first pair seen
wins a count tie, matching `collections.Counter` iteration and `max`), and
every occurrence of that pair merges in the training words. A merge whose
bytes already exist overwrites the old rank, as the Python assignment does.
"""

from tiktoken import Bpe, Core, Pattern, Unicode

__all__ = [
    "SimpleBytePairEncoding", "simple", "bpe_encode", "bpe_train", "train",
    "encode", "decode", "decode_bytes", "decode_tokens_bytes",
]

# SimpleBytePairEncoding(pat_str=..., mergeable_ranks=...) is an Encoding
# without special tokens.
type SimpleBytePairEncoding = Core.Encoding


def simple(pattern, mergeable_ranks):
    """The SimpleBytePairEncoding constructor."""
    return Core.build("simple", mergeable_ranks, {}, pattern)


def bpe_encode(mergeable_ranks, piece) -> Result[list[int], str]:
    """The module-level educational encoder: the core merge loop on raw bytes."""
    return Bpe.bpe_encode(mergeable_ranks, piece)


def encode(enc, text) -> Result[list[int], str]:
    """SimpleBytePairEncoding.encode: split by the pattern, encode each piece."""
    return Core.encode_ordinary(enc, text)


def decode_bytes(enc, tokens):
    """SimpleBytePairEncoding.decode_bytes."""
    return Core.decode_bytes(enc, tokens)


def decode(enc, tokens):
    """SimpleBytePairEncoding.decode: errors='replace', like the original."""
    return Core.decode(enc, tokens)


def decode_tokens_bytes(enc, tokens):
    """SimpleBytePairEncoding.decode_tokens_bytes."""
    return Core.decode_tokens_bytes(enc, tokens)


def train(data, vocab_size, pattern):
    """SimpleBytePairEncoding.train: bpe_train wrapped in the codec."""
    match bpe_train(data, vocab_size, pattern):
        case Success(value=value):
            return Success(value=simple(pattern, value))
        case Error(message=message):
            return Error(message=message)


def bpe_train(data, vocab_size, pattern):
    """Train a byte-pair vocabulary on data, up to vocab_size tokens."""
    if vocab_size < 256:
        return Error(message="vocab_size must be at least 256, so we can encode all bytes")
    else:
        return train_loop(Bpe.base_ranks(), training_words(pattern, data), vocab_size)


def training_words(pattern, data):
    """The data as per-piece lists of single-byte token keys."""
    pieces = Pattern.split(pattern, data)
    return mapList(pieces, lambda p: mapList(p, Bpe.key_char))


def train_loop(ranks, words, vocab_size) -> Result[dict[str, int], str]:
    """One merge round, until the vocabulary is full."""
    if len(ranks) >= vocab_size:
        return Success(value=ranks)
    else:
        match pick_pair(words):
            case Success(value=value):
                return train_loop(
                    mapSet(ranks, value.first + value.second, len(ranks)),
                    merged_words(words, value),
                    vocab_size,
                )
            case Error(message=message):
                return Error(message=message)


class Counts:
    """The pair counts and the order pairs were first seen in."""
    counts: dict[str, int]
    order: list[str]


def pick_pair(words):
    """The most common adjacent pair over all words; the first seen wins a
    tie, like Counter iteration order under max."""
    state = count_pairs(words)
    if len(state.order) == 0:
        return Error(message="no pairs left to merge before the vocabulary filled")
    else:
        best = foldList(state.order, "", lambda acc, pk: better_key(state.counts, acc, pk))
        return pair_of_key(best)


def better_key(counts, acc, pk):
    """The key with the higher count; strictly greater replaces, so the first
    seen wins ties and acc == '' means nothing seen yet."""
    if acc == "":
        return pk
    elif unwrap_or(mapGet(counts, pk), 0) > unwrap_or(mapGet(counts, acc), 0):
        return pk
    else:
        return acc


def pair_of_key(pk):
    """The pair behind a pair key, split at the separator."""
    parts = unwrap_or(split(pk, Bpe.SEP), [])
    return Success(value=Bpe.Pair(first=Unicode.at_str(parts, 0), second=Unicode.at_str(parts, 1)))


def count_pairs(words):
    """Count every adjacent pair, keeping first-seen order."""
    return foldList(words, Counts(counts={}, order=[]), count_word)


def count_word(state, word):
    return fold(range(0, len(word) - 1), state, lambda st, i: count_pair_at(st, word, i))


def count_pair_at(state, word, i):
    pk = Bpe.pair_key(Unicode.at_str(word, i), Unicode.at_str(word, i + 1))
    return Counts(
        counts=mapSet(state.counts, pk, unwrap_or(mapGet(state.counts, pk), 0) + 1),
        order=state.order if mapContains(state.counts, pk) else listAppend(state.order, pk),
    )


def merged_words(words, pair):
    """Merge the chosen pair in every word, left to right, non-overlapping."""
    return mapList(words, lambda w: Bpe.merge_all(w, pair.first, pair.second, 0, []))
