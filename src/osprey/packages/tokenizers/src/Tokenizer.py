# osprey: flavor=python
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
from tiktoken import Bpe
from tokenizers.Decoders import bytelevel_decode
from tokenizers.Models import BpeModel, Token, bpe_tokenize, char_starts, id_to_token, unigram_tokenize, wordlevel_tokenize, wordpiece_tokenize
from tokenizers.Normalizers import Norm, append_norm, bert_normalize, char_at, empty_norm, end_at, nfc_normalize, norm_of, norm_text, start_at
from tokenizers.PreTokenizers import META_CP, bert_pretokenize, bytelevel_text, metaspace_pretokenize, qwen_pretokenize, whitespace_pretokenize
from tokenizers.Processors import Encoding, Offset, bert_template, concat_two, empty_encoding
from tokenizers.Unicode import cps_text, is_whitespace, is_word_char, text_cp_starts, text_cps
from tokenizers.Decoders import byte_fallback_decode_chain, metaspace_decode_chain, wordpiece_decode

__all__ = [
    "byte_char_map", "char_of", "wordpiece_encode", "wordpiece_encode_pair",
    "unigram_encode_pipeline", "wordlevel_encode_pipeline",
    "wordpiece_decode_pipeline", "unigram_decode_pipeline", "wordlevel_decode_pipeline",
    "AddedToken", "QwenTokenizer", "qwen_encode", "qwen_decode",
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


# ---------------------------------------------------------------------------
# The Qwen pipeline: added tokens, NFC, Split, ByteLevel, byte-level BPE
# ---------------------------------------------------------------------------

class AddedToken:
    """An added token from tokenizer.json: its text, id, whether the decoder treats
    it as special, and the three match flags the reference honours."""
    content: str
    id: int
    special: bool
    lstrip: bool
    rstrip: bool
    single_word: bool


class QwenTokenizer:
    """A loaded tokenizer.json: the byte-level BPE model and its added tokens."""
    model: BpeModel
    added: list[AddedToken]


class Segment:
    """A slice of the input: an added token's index, or -1 for text to tokenize,
    starting at character `chars`."""
    added: int
    chars: int


class Segments:
    """The segments found so far, where the pending run of plain text starts, and
    the character index reached."""
    done: list[Segment]
    plain: int
    chars: int


class Hit:
    """The best added-token match at one position: its index and how many
    characters it covers; a width of 0 means nothing matched."""
    index: int
    width: int


def qwen_encode(tok, text) -> Result[Encoding, str]:
    """The whole pipeline: added tokens first, then NFC, Split, ByteLevel and BPE
    over every run of text between them, exactly as the reference composes it."""
    cps = text_cps(text)
    starts = text_cp_starts(text)
    segments = segments_of(tok, cps)
    byte_char = byte_char_map(text)
    st = fold(range(len(segments)), Encodings(enc=empty_encoding(), bad=-1), lambda acc, i: segment_encoding(tok, segments, i, acc, cps, starts, byte_char))
    if st.bad >= 0:
        return Error(message="segment " + toString(st.bad) + " could not be tokenized")
    else:
        return Success(value=st.enc)


def empty_segment():
    return Segment(added=-1, chars=0)


def empty_added():
    return AddedToken(content="", id=-1, special=False, lstrip=False, rstrip=False, single_word=False)


def segments_of(tok, cps):
    """The input split at its added tokens: they are matched in the raw text before
    any normalization, the longest match at the earliest position winning."""
    st = fold(range(len(cps)), Segments(done=[], plain=0, chars=0), lambda acc, i: segment_step(tok, cps, acc, i))
    return listAppend(st.done, Segment(added=-1, chars=st.plain))


def segment_step(tok, cps, acc, i):
    if i < acc.chars:
        return acc
    else:
        return segment_at(tok, cps, acc, i)


def segment_at(tok, cps, acc, i):
    hit = match_added(tok, cps, i)
    if hit.width == 0:
        return Segments(done=acc.done, plain=acc.plain, chars=i + 1)
    else:
        return add_match(tok, cps, acc, hit, i)


def add_match(tok, cps, acc, hit, i):
    """Take a match, letting its strip flags move the boundary it splits at."""
    token = unwrap_or(listGet(tok.added, hit.index), empty_added())
    start = strip_left(token, cps, acc.plain, i)
    stop = strip_right(token, cps, i + hit.width, len(cps))
    return Segments(done=listAppend(flush_plain(acc, start), Segment(added=hit.index, chars=start)), plain=stop, chars=stop)


def flush_plain(acc, start):
    return acc.done if start <= acc.plain else listAppend(acc.done, Segment(added=-1, chars=acc.plain))


def strip_left(token, cps, low, i):
    """How far a left-stripping token reaches back over whitespace, but no further
    than the pending plain text."""
    return i - trailing_space(cps, low, i) if token.lstrip else i


def trailing_space(cps, low, i):
    """The length of the whitespace run that ends just before i, down to low."""
    return fold(range(low, i), 0, lambda acc, k: acc + 1 if is_whitespace(at_cp(cps, k)) else 0)


def strip_right(token, cps, i, stop):
    return i + leading_space(cps, i, stop) if token.rstrip else i


def leading_space(cps, i, stop):
    """The length of the whitespace run that starts at i."""
    return fold(range(i, stop), 0, lambda acc, k: acc + 1 if acc == k - i and is_whitespace(at_cp(cps, k)) else acc)


def match_added(tok, cps, i):
    """The longest added token matching at i, the reference's preference."""
    return fold(range(len(tok.added)), Hit(index=-1, width=0), lambda acc, a: try_added(tok, cps, i, acc, a))


def try_added(tok, cps, i, acc, a):
    width = match_length(unwrap_or(listGet(tok.added, a), empty_added()), cps, i)
    return Hit(index=a, width=width) if width > acc.width else acc


def match_length(token, cps, i):
    """How many characters of the token match at i, or 0 when it does not."""
    chars = text_cps(token.content)
    if len(chars) == 0 or i + len(chars) > len(cps):
        return 0
    else:
        return matched_length(token, cps, chars, i)


def matched_length(token, cps, chars, i):
    width = fold(range(len(chars)), 0, lambda acc, k: acc + 1 if acc == k and at_cp(cps, i + k) == at_cp(chars, k) else acc)
    return width if width == len(chars) and single_word_ok(token, cps, i, width) else 0


def single_word_ok(token, cps, i, width):
    """`single_word` refuses a match touching a word character, which is what the
    reference's lookarounds do."""
    if token.single_word:
        return not is_word_char(at_cp(cps, i - 1)) and not is_word_char(at_cp(cps, i + width))
    else:
        return True


def at_cp(cps, i):
    """Code point i, or -1 outside the list: the scanner's safe read."""
    return unwrap_or(listGet(cps, i), -1) if i >= 0 and i < len(cps) else -1


class Encodings:
    """The encoding built so far and the index of the first segment that failed."""
    enc: Encoding
    bad: int


def segment_encoding(tok, segments, i, acc, cps, starts, byte_char):
    if acc.bad >= 0:
        return acc
    else:
        return segment_at_index(tok, segments, i, acc, cps, starts, byte_char)


def segment_at_index(tok, segments, i, acc, cps, starts, byte_char):
    seg = unwrap_or(listGet(segments, i), empty_segment())
    stop = segment_end(segments, i, len(cps))
    if seg.added >= 0:
        return Encodings(enc=append_added(acc.enc, unwrap_or(listGet(tok.added, seg.added), empty_added()), seg.chars, stop), bad=-1)
    else:
        return text_encoding(tok, seg, stop, acc, cps, starts, byte_char)


def segment_end(segments, i, nchars):
    """A segment ends where the next one starts; the last ends with the input."""
    return unwrap_or(listGet(segments, i + 1), Segment(added=-1, chars=nchars)).chars


def append_added(enc, token, start, stop):
    return Encoding(
        ids=listAppend(enc.ids, token.id),
        tokens=listAppend(enc.tokens, token.content),
        offsets=listAppend(enc.offsets, Offset(start=start, end=stop)),
        type_ids=listAppend(enc.type_ids, 0),
    )


def slice_list(xs, start, stop):
    """xs[start:stop]."""
    return toList(map(range(start, stop), lambda i: unwrap_or(listGet(xs, i), 0)))


def text_encoding(tok, seg, stop, acc, cps, starts, byte_char):
    """NFC, Split, ByteLevel and BPE over one run of plain text."""
    segment = cps_text(slice_list(cps, seg.chars, stop))
    norm = nfc_normalize(norm_shifted(segment, unwrap_or(listGet(starts, seg.chars), 0)))
    return pieces_encoding(tok.model, qwen_pretokenize(norm), acc, byte_char, len(starts))


def norm_shifted(text, base):
    """A Norm over text whose byte ranges are offset by base."""
    n = norm_of(text)
    return fold(range(len(n.chars)), empty_norm(), lambda acc, i: append_norm(acc, char_at(n, i), start_at(n, i) + base, end_at(n, i) + base))


def pieces_encoding(m, pieces, acc, byte_char, nchars) -> Encodings:
    return fold(range(len(pieces)), acc, lambda a, i: piece_encoding(m, unwrap_or(listGet(pieces, i), empty_split()), a, byte_char, nchars))


def piece_encoding(m, piece, acc, byte_char, nchars) -> Encodings:
    if acc.bad >= 0:
        return acc
    else:
        return piece_tokens(m, piece, cps_text(piece.chars), acc, byte_char, nchars)


def piece_tokens(m, piece, text, acc, byte_char, nchars) -> Encodings:
    match bpe_tokenize(m, text):
        case Success(value=tokens):
            return Encodings(enc=fold(range(len(tokens)), acc.enc, lambda e, k: append_piece_token(e, piece, unwrap_or(listGet(tokens, k), Token(id=0, value="", start=0, end=0)), byte_char, nchars)), bad=-1)
        case Error(message=_message):
            return Encodings(enc=acc.enc, bad=0)


def append_piece_token(enc, piece, token, byte_char, nchars) -> Encoding:
    """Append a model token, whose byte span inside the piece is a span of the
    original text that the byte-to-character map turns into char offsets. A token
    that stops inside a character still covers the whole of it, as the reference
    reports."""
    start = char_of(byte_char, nchars, start_at(piece, token.start))
    stop = char_of(byte_char, nchars, start_at(piece, token.end - 1)) + 1
    return Encoding(
        ids=listAppend(enc.ids, token.id),
        tokens=listAppend(enc.tokens, token.value),
        offsets=listAppend(enc.offsets, Offset(start=start, end=stop)),
        type_ids=listAppend(enc.type_ids, 0),
    )


def qwen_decode(tok, ids, skip_special) -> Result[str, str]:
    """The text an id list spells: added tokens are their own content, every other
    id names a token whose bytes are read, and `skip_special` drops the tokens the
    vocabulary marks special."""
    match decode_texts(tok, ids, skip_special):
        case Success(value=texts):
            return bytelevel_decode(texts)
        case Error(message=message):
            return Error(message=message)


class Decoded:
    """The token texts so far and the first id that names no token (-1 while none)."""
    texts: list[str]
    bad: int


def decode_texts(tok, ids, skip_special) -> Result[list[str], str]:
    st = fold(range(len(ids)), Decoded(texts=[], bad=-1), lambda acc, i: decode_step(tok, ids, acc, i, skip_special))
    if st.bad >= 0:
        return Error(message="no token with id " + toString(unwrap_or(listGet(ids, st.bad), -1)))
    else:
        return Success(value=st.texts)


def decode_step(tok, ids, acc, i, skip_special):
    if acc.bad >= 0:
        return acc
    else:
        return decode_id(tok, ids, acc, i, skip_special)


def decode_id(tok, ids, acc, i, skip_special):
    id = unwrap_or(listGet(ids, i), -1)
    added = find_added(tok.added, id)
    if added >= 0:
        return added_text(unwrap_or(listGet(tok.added, added), empty_added()), acc, skip_special)
    else:
        return entry_text(tok.model, id, acc)


def find_added(added, id):
    """The index of the added token with this id, or -1."""
    return fold(range(len(added)), -1, lambda acc, a: a if acc < 0 and unwrap_or(listGet(added, a), empty_added()).id == id else acc)


def added_text(token, acc, skip_special):
    if token.special and skip_special:
        return Decoded(texts=acc.texts, bad=-1)
    else:
        return Decoded(texts=listAppend(acc.texts, token.content), bad=-1)


def entry_text(m, id, acc):
    match Bpe.key_of_rank(m.entries, id, 0, len(m.entries) - 1):
        case Success(value=key):
            return Decoded(texts=listAppend(acc.texts, bytelevel_text(Bpe.bytes_of_key(key))), bad=-1)
        case Error(message=_message):
            return Decoded(texts=acc.texts, bad=0)
