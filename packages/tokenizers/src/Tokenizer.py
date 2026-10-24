# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
# osprey: namespace=tokenizers
# osprey: module=Tokenizer
"""The high-level Tokenizer: normalize, pre-tokenize, model-tokenize, then
combine byte alignments into the original input's char offsets.

One pipeline per model family, mirroring how the real `tokenizers` composes a
Tokenizer. Offsets are the Python binding's char offsets, exactly what
`Tokenizer.encode` reports: a pre-token's byte span is mapped through the
normalizer's alignments into original bytes, then through a byte->char map of
the original string.
"""
from tokenizers.Normalizers import Norm, bert_normalize, norm_of, norm_text
from tokenizers.PreTokenizers import META_CP, bert_pretokenize, metaspace_pretokenize, whitespace_pretokenize
from tokenizers.Models import Token, char_starts, id_to_token, unigram_tokenize, wordlevel_tokenize, wordpiece_tokenize
from tokenizers.Processors import Encoding, Offset, bert_template, concat_two, empty_encoding
from tokenizers.Decoders import byte_fallback_decode_chain, metaspace_decode_chain, wordpiece_decode

__all__ = [
    "byte_char_map", "char_of", "wordpiece_encode", "wordpiece_encode_pair",
    "unigram_encode_pipeline", "wordlevel_encode_pipeline",
    "wordpiece_decode_pipeline", "unigram_decode_pipeline", "wordlevel_decode_pipeline",
]


def byte_char_map(s):
    """For each byte of s, the index of the char that byte belongs to."""
    return byte_char_go(s, 0, 0, [])


def byte_char_go(s, i, ci, acc):
    if i >= len(s):
        return acc
    else:
        cp = unwrap_or(codePointAt(s, i), 63)
        w = unwrap_or(codePointWidth(cp), 1)
        acc2 = fold(range(w), acc, lambda a, _: listAppend(a, ci))
        return byte_char_go(s, i + w, ci + 1, acc2)


def char_of(byte_char, nchars, b):
    """The char index of a byte offset, or the char count past the end."""
    if b < len(byte_char):
        return unwrap_or(listGet(byte_char, b), 0)
    else:
        return nchars


def offset_chars(byte_char, nchars, s, e):
    return Offset(start=char_of(byte_char, nchars, s), end=char_of(byte_char, nchars, e))


def find_start(starts, n_chars, b):
    return fold(range(n_chars), 0, lambda acc, i: i if unwrap_or(listGet(starts, i), -1) == b else acc)


def token_end_index(starts, n_chars, text, b):
    if b >= len(text):
        return n_chars
    else:
        return find_start(starts, n_chars, b)


def split_encoding(split, tokenize, byte_char, nchars):
    text = norm_text(split)
    starts = char_starts(text)
    toks = tokenize(text)
    return fold(range(len(toks)), empty_encoding(), lambda acc, k: append_token(acc, split, starts, text, unwrap_or(listGet(toks, k), Token(id=0, value="", start=0, end=0)), byte_char, nchars))


def append_token(acc, split, starts, text, tok, byte_char, nchars):
    i = find_start(starts, len(starts), tok.start)
    j = token_end_index(starts, len(starts), text, tok.end)
    s = unwrap_or(listGet(split.starts, i), 0)
    e = unwrap_or(listGet(split.ends, j - 1), 0)
    off = offset_chars(byte_char, nchars, s, e)
    return Encoding(
        ids=listAppend(acc.ids, tok.id),
        tokens=listAppend(acc.tokens, tok.value),
        offsets=listAppend(acc.offsets, off),
        type_ids=listAppend(acc.type_ids, 0),
    )


def encode_splits(splits, tokenize, byte_char, nchars):
    return fold(range(len(splits)), empty_encoding(), lambda acc, i: concat_two(acc, split_encoding(unwrap_or(listGet(splits, i), empty_split()), tokenize, byte_char, nchars)))


def empty_split():
    return Norm(chars=[], starts=[], ends=[])


def wordpiece_single_encoding(m, text):
    norm = bert_normalize(text, True, True, True, True)
    byte_char = byte_char_map(text)
    nchars = len(char_starts(text))
    return encode_splits(bert_pretokenize(norm), lambda t: wordpiece_tokenize(m, t), byte_char, nchars)


def wordpiece_encode(m, text, cls_id, sep_id):
    """BERT pipeline with [CLS]/[SEP] added, type ids all zero for one sequence."""
    return bert_template([wordpiece_single_encoding(m, text)], cls_id, sep_id)


def wordpiece_encode_pair(m, text_a, text_b, cls_id, sep_id):
    """BERT pair pipeline: [CLS] A [SEP] B [SEP] with B at type id 1."""
    return bert_template([wordpiece_single_encoding(m, text_a), wordpiece_single_encoding(m, text_b)], cls_id, sep_id)


def unigram_encode_pipeline(m, text):
    """Metaspace + Unigram (byte fallback per the model) on the raw text."""
    norm = norm_of(text)
    byte_char = byte_char_map(text)
    nchars = len(char_starts(text))
    return encode_splits(metaspace_pretokenize(norm, META_CP, 0, True), lambda t: unigram_tokenize(m, t), byte_char, nchars)


def wordlevel_encode_pipeline(m, text):
    """Whitespace + WordLevel on the raw text."""
    norm = norm_of(text)
    byte_char = byte_char_map(text)
    nchars = len(char_starts(text))
    return encode_splits(whitespace_pretokenize(norm), lambda t: wordlevel_tokenize(m, t), byte_char, nchars)


def decode_chain(tokens, ids, specials):
    """id -> vocab token for every id, skipping special tokens."""
    return fold(range(len(ids)), [], lambda acc, i: append_chain(acc, id_to_token(tokens, unwrap_or(listGet(ids, i), 0)), specials))


def append_chain(acc, tok, specials):
    if contains_str(specials, tok):
        return acc
    else:
        return listAppend(acc, tok)


def contains_str(xs, s):
    return fold(range(len(xs)), False, lambda acc, i: acc or unwrap_or(listGet(xs, i), "") == s)


def wordpiece_decode_pipeline(m, ids):
    """WordPiece decode: ids to tokens, ## joined, cleaned."""
    return wordpiece_decode(decode_chain(m.tokens, ids, []), "##", True)


def unigram_decode_pipeline(m, ids, byte_fallback_mode):
    """Unigram decode: byte fallback first when enabled, then Metaspace."""
    chain = decode_chain(m.tokens, ids, [])
    if byte_fallback_mode:
        return join(metaspace_decode_chain(byte_fallback_decode_chain(chain), META_CP, 0), "")
    else:
        return join(metaspace_decode_chain(chain, META_CP, 0), "")


def wordlevel_decode_pipeline(m, ids):
    """WordLevel decode: tokens joined with a single space."""
    return join(decode_chain(m.tokens, ids, []), " ")
