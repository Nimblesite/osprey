# osprey: flavor=python
# osprey: namespace=qwen
# osprey: module=Qwen3_5Vocab
"""The tokenizer a GGUF carries: built from its own `tokenizer.ggml.*` tables.

`tokenizer` reads `tokens`, `token_type` and `merges` through `@osprey/gguf` and hands
them to `@osprey/tokenizers`' `GgufVocab`, so prompt ids come from the file being
served and nothing else; `eos_of` is the end-of-turn id the file declares, and `ids_of`
encodes one text. Implements [LLM-TOKENIZER].
"""

from gguf import Chain, Reader
from tokenizers import GgufVocab
from tokenizers.Tokenizer import qwen_encode

__all__ = ["Tables", "tables", "tokenizer", "eos_of", "ids_of"]


class Tables:
    """The three vocabulary tables a GGUF carries."""
    tokens: list[str]
    types: list[int]
    merges: list[str]


def strings(file, key):
    return Chain.then(Reader.field(file, key), lambda field: Reader.field_strings(field))


def ints(file, key):
    return Chain.then(Reader.field(file, key), lambda field: Reader.field_ints(field))


def tables(file):
    """`tokenizer.ggml.tokens`, `token_type` and `merges`, or the first one missing."""
    return Chain.then(strings(file, "tokenizer.ggml.tokens"), lambda tokens: Chain.then(ints(file, "tokenizer.ggml.token_type"), lambda types: Chain.then(strings(file, "tokenizer.ggml.merges"), lambda merges: Success(value=Tables(tokens=tokens, types=types, merges=merges)))))


def tokenizer(file):
    """The file's tokenizer, from its own tables."""
    return Chain.then(tables(file), lambda t: GgufVocab.qwen_from_vocabulary(t.tokens, t.types, t.merges))


def eos_of(file):
    """`tokenizer.ggml.eos_token_id`: the end-of-turn token the file declares."""
    return Chain.then(Reader.field(file, "tokenizer.ggml.eos_token_id"), lambda field: Reader.field_int(field))


def ids_of(tok, text):
    """The token ids of `text`."""
    return Chain.then(qwen_encode(tok, text), lambda encoding: Success(value=encoding.ids))
