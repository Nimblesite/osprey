# osprey: flavor=python
# osprey: namespace=tokenizers
# osprey: module=GgufVocab
"""The Qwen tokenizer from a GGUF file's own vocabulary tables.

A `gpt2`-model GGUF carries `tokenizer.ggml.tokens` (every token's byte-level
spelling, indexed by id), `tokenizer.ggml.token_type` (llama.cpp's type per id)
and `tokenizer.ggml.merges` (each merge as `left right`, in rank order). This
module turns those three lists into the same `QwenTokenizer` that
`HfJson.load_qwen_tokenizer` builds from `tokenizer.json`, so a served model's
prompt is tokenized from the weights file it runs, with no second download.
Implements [LLM-TOKENIZER]: "read vocabulary/merges from GGUF metadata".

The byte-level BPE model needs the 256 single-byte ids and every merge's id,
which is the id of the merged spelling, so a token the merges name but the
vocabulary lacks is an `Error`, never a skipped merge. Added tokens are the ids
llama.cpp types as `CONTROL` (3, special: dropped by a `skip_special` decode),
`USER_DEFINED` (4, matched whole but kept by every decode, as `<tool_call>` is)
and `UNUSED` (5, the `[PAD…]` placeholders, special). The reference's
`tokenizer.json` marks a few `CONTROL` ids such as `<|fim_prefix|>` as not
special; the GGUF has no record of that distinction, and the README notes it.
"""

from tokenizers.Models import build_bpe
from tokenizers.PreTokenizers import bytelevel_text
from tokenizers.Tokenizer import AddedToken, QwenTokenizer

__all__ = ["qwen_from_vocabulary", "CONTROL", "USER_DEFINED", "UNUSED"]

CONTROL = 3
USER_DEFINED = 4
UNUSED = 5


class Merges:
    """The merges split into halves, with the id of each merged spelling."""
    left: list[str]
    right: list[str]
    ids: list[int]
    bad: int


def qwen_from_vocabulary(tokens, types, merges) -> Result[QwenTokenizer, str]:
    """The tokenizer the three GGUF tables describe, or the first thing missing."""
    ids = index_of(tokens)
    match byte_ids(ids):
        case Success(value=bytes_ids):
            return with_merges(tokens, types, ids, bytes_ids, split_merges(ids, merges))
        case Error(message=message):
            return Error(message=message)


def with_merges(tokens, types, ids, bytes_ids, merges) -> Result[QwenTokenizer, str]:
    if merges.bad >= 0:
        return Error(message="merge " + toString(merges.bad) + " names a spelling the vocabulary lacks")
    else:
        match build_bpe(bytes_ids, merges.left, merges.right, merges.ids):
            case Success(value=model):
                return Success(value=QwenTokenizer(model=model, added=added_tokens(tokens, types)))
            case Error(message=message):
                return Error(message=message)


def index_of(tokens):
    """Every spelling's id."""
    return fold(range(len(tokens)), {}, lambda acc, i: mapSet(acc, unwrap_or(listGet(tokens, i), ""), i))


def id_of(ids, text):
    """The id a spelling has, or -1."""
    return unwrap_or(mapGet(ids, text), -1)


def byte_ids(ids) -> Result[list[int], str]:
    """The id of each single byte's spelling, in byte order."""
    found = toList(map(range(256), lambda b: id_of(ids, bytelevel_text([b]))))
    missing = fold(range(256), -1, lambda hit, b: b if hit < 0 and unwrap_or(listGet(found, b), -1) < 0 else hit)
    if missing >= 0:
        return Error(message="the vocabulary has no token for byte " + toString(missing))
    else:
        return Success(value=found)


def split_merges(ids, merges) -> Merges:
    """Each `left right` entry split at its first space, with the merged id."""
    return fold(range(len(merges)), Merges(left=[], right=[], ids=[], bad=-1), lambda acc, i: merge_step(ids, merges, acc, i))


def merge_step(ids, merges, acc, i) -> Merges:
    if acc.bad >= 0:
        return acc
    else:
        return merge_split(ids, acc, i, unwrap_or(listGet(merges, i), ""))


def merge_split(ids, acc, i, entry) -> Merges:
    cut = unwrap_or(indexOf(entry, " "), -1)
    left = unwrap_or(substring(entry, 0, cut), "")
    right = unwrap_or(substring(entry, cut + 1, len(entry)), "")
    id = id_of(ids, left + right)
    if cut < 0 or id < 0:
        return Merges(left=acc.left, right=acc.right, ids=acc.ids, bad=i)
    else:
        return Merges(left=listAppend(acc.left, left), right=listAppend(acc.right, right), ids=listAppend(acc.ids, id), bad=-1)


def added_tokens(tokens, types):
    """Every control, user-defined and unused id as an added token, in id order."""
    return fold(range(len(types)), [], lambda acc, i: with_added(acc, tokens, i, unwrap_or(listGet(types, i), 0)))


def with_added(acc, tokens, i, kind):
    if kind == CONTROL or kind == USER_DEFINED or kind == UNUSED:
        return listAppend(acc, AddedToken(content=unwrap_or(listGet(tokens, i), ""), id=i, special=kind != USER_DEFINED, lstrip=False, rstrip=False, single_word=False))
    else:
        return acc
