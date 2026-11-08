# osprey: flavor=python
# osprey: namespace=tokenizers
# osprey: module=Models
"""The tokenization models: WordPiece (greedy longest-match-first), Unigram
(Viterbi over log-probability scores with byte fallback) and WordLevel.

Each model maps a pre-token string to a list of `Token` (id, value, byte
offsets within the pre-token). The high-level Tokenizer composes those with the
pre-tokenizer offsets. Unigram follows tokenizers' optimized encoder: a forward
dynamic program with strict `>` tie-breaking in increasing start position, then
a backward pass that fuses consecutive unknown pieces.
"""

from tiktoken import Bpe
from tokenizers.PreTokenizers import bytelevel_raw, bytelevel_text
from tokenizers.Unicode import text_cp_starts

__all__ = [
    "Token", "build_wordpiece", "wordpiece_tokenize", "build_unigram",
    "unigram_tokenize", "unigram_encode", "build_wordlevel", "wordlevel_tokenize",
    "id_to_token", "char_starts", "BpeModel", "build_bpe", "bpe_tokenize", "token_index",
]

K_UNK_PENALTY = 10.0
HEX = "0123456789ABCDEF"


class Token:
    """One model token: its vocab id, spelling, and byte span in the pre-token."""
    id: int
    value: str
    start: int
    end: int


class WordPieceModel:
    """A WordPiece vocabulary: token list, token->id map and the unk/prefix."""
    tokens: list[str]
    token_to_id: dict[str, int]
    unk_token: str
    continuing_prefix: str
    max_chars: int


class UnigramModel:
    """A Unigram vocabulary: tokens with log-probability scores."""
    tokens: list[str]
    scores: list[float]
    token_to_id: dict[str, int]
    unk_id: int
    byte_fallback: bool
    fuse_unk: bool
    min_score: float


class WordLevelModel:
    """A WordLevel vocabulary: whole tokens and their unk token."""
    tokens: list[str]
    token_to_id: dict[str, int]
    unk_token: str


class Node:
    """A Viterbi cell: best-path score ending here and the link backward."""
    id: int
    score: float
    starts_at: int


class Step:
    best: list[Node]
    has_single: bool


def char_starts(s):
    """The byte offset where each codepoint of s starts."""
    return text_cp_starts(s)


def id_to_token(tokens, id):
    """The vocab token at an id, or the empty string when out of range."""
    return unwrap_or(listGet(tokens, id), "")


# ---------------------------------------------------------------------------
# WordPiece
# ---------------------------------------------------------------------------

def token_index(tokens):
    """Every spelling's id: the token->id map an id-ordered token list defines."""
    return fold(range(len(tokens)), {}, lambda m, i: mapSet(m, unwrap_or(listGet(tokens, i), ""), i))


def build_wordpiece(vocab, unk_token, prefix, max_chars):
    """A WordPiece model from an id-ordered token list."""
    token_to_id = token_index(vocab)
    return WordPieceModel(tokens=vocab, token_to_id=token_to_id, unk_token=unk_token, continuing_prefix=prefix, max_chars=max_chars)


def unk_id(m, token):
    return unwrap_or(mapGet(m.token_to_id, token), 0)


def wordpiece_tokenize(m, text):
    """Greedy longest-match-first with `##` continuations; the whole word is
    [UNK] when any part fails or it is longer than max_chars."""
    starts = char_starts(text)
    if len(starts) > m.max_chars:
        return [Token(id=unk_id(m, m.unk_token), value=m.unk_token, start=0, end=len(text))]
    else:
        return wordpiece_greedy(m, text, starts, 0, [])


def wordpiece_greedy(m, text, starts, si, acc):
    if si >= len(starts):
        return acc
    else:
        found = longest_match(m, text, starts, si, len(starts))
        if found < 0:
            return [Token(id=unk_id(m, m.unk_token), value=m.unk_token, start=0, end=len(text))]
        else:
            return wordpiece_greedy(m, text, starts, found, listAppend(acc, part_token(m, text, part_span(text, starts, si, found), si)))


def part_span(text, starts, si, end_idx):
    """The byte span [start, end) of the chars [si, end_idx)."""
    return [unwrap_or(listGet(starts, si), 0), unwrap_or(listGet(starts, end_idx), len(text))]


def part_key(m, text, span, si):
    """The vocab key of one word part: its text, prefixed when it continues a word."""
    sub = unwrap_or(substring(text, unwrap_or(listGet(span, 0), 0), unwrap_or(listGet(span, 1), len(text))), "")
    return m.continuing_prefix + sub if si > 0 else sub


def part_token(m, text, span, si):
    """The token of one word part: its key and its byte span."""
    key = part_key(m, text, span, si)
    return Token(id=unk_id(m, key), value=key, start=unwrap_or(listGet(span, 0), 0), end=unwrap_or(listGet(span, 1), len(text)))


def longest_match(m, text, starts, si, end_idx):
    """The largest end char index <= end_idx such that text[si:end] (with the
    continuation prefix when si > 0) is in the vocab, or -1."""
    if end_idx <= si:
        return -1
    else:
        if mapContains(m.token_to_id, part_key(m, text, part_span(text, starts, si, end_idx), si)):
            return end_idx
        else:
            return longest_match(m, text, starts, si, end_idx - 1)


# ---------------------------------------------------------------------------
# Unigram
# ---------------------------------------------------------------------------

def build_unigram(tokens, scores, unk_id, byte_fallback):
    """A Unigram model from parallel token and score lists."""
    token_to_id = token_index(tokens)
    min_score = fold(range(len(scores)), exp(1000.0), lambda acc, i: low(acc, unwrap_or(listGet(scores, i), 0.0)))
    return UnigramModel(tokens=tokens, scores=scores, token_to_id=token_to_id, unk_id=unk_id, byte_fallback=byte_fallback, fuse_unk=True, min_score=min_score)


def low(a, b):
    return b if b < a else a


def unigram_tokenize(m, text):
    """Viterbi-segment then tokenize one pre-token string."""
    strs = unigram_encode(m, text)
    return tokenize_strings(m, strs, 0, 0, [])


def tokenize_strings(m, strs, i, offset, acc):
    if i >= len(strs):
        return acc
    else:
        s = unwrap_or(listGet(strs, i), "")
        return tokenize_strings(m, strs, i + 1, offset + len(s), listConcat(acc, tokens_for(m, s, offset)))


def tokens_for(m, s, offset):
    match mapGet(m.token_to_id, s):
        case Success(value=value):
            return [Token(id=value, value=s, start=offset, end=offset + len(s))]
        case Error():
            if m.byte_fallback:
                return byte_tokens(m, s, offset)
            else:
                return [Token(id=m.unk_id, value=s, start=offset, end=offset + len(s))]


def hex2(b):
    return unwrap_or(substring(HEX, b // 16, b // 16 + 1), "0") + unwrap_or(substring(HEX, b % 16, b % 16 + 1), "0")


def byte_token_text(b):
    return "<0x" + hex2(b) + ">"


def byte_tokens(m, s, offset):
    token_to_id = m.token_to_id
    unk = m.unk_id
    return byte_tokens_go(token_to_id, unk, s, offset, 0, [])


def byte_tokens_go(token_to_id, unk, s, offset, k, acc):
    """Each byte of an out-of-vocab piece becomes a <0xXX> token; if any byte
    is missing from the vocab the whole piece stays the unk token."""
    if k >= len(s):
        return acc
    else:
        b = unwrap_or(byteAt(s, k), 0)
        bt = byte_token_text(b)
        id = unwrap_or(mapGet(token_to_id, bt), -1)
        if id < 0:
            return [Token(id=unk, value=s, start=offset, end=offset + len(s))]
        else:
            return byte_tokens_go(token_to_id, unk, s, offset, k + 1, listAppend(acc, Token(id=id, value=bt, start=offset, end=offset + len(s))))


def unigram_encode(m, text):
    """The Viterbi segmentation, as a list of piece strings (unknowns fused)."""
    if len(text) == 0:
        return []
    else:
        best = unigram_dp(m, text)
        return unigram_backtrack(m, text, best, len(text), [], [])


def unigram_dp(m, text):
    starts = char_starts(text)
    size = len(text)
    init = toList(map(range(size + 1), lambda _: Node(id=0, score=0.0, starts_at=-1)))
    return fold(range(len(starts)), init, lambda best, i: step_position(m, text, best, unwrap_or(listGet(starts, i), 0)))


def step_position(m, text, best, pos):
    mblen = char_width_at(text, pos)
    score_here = unwrap_or(listGet(best, pos), Node(id=0, score=0.0, starts_at=-1)).score
    rest = unwrap_or(substring(text, pos, len(text)), "")
    st = consider_token_at(m, rest, pos, mblen, score_here, Step(best=best, has_single=False), 0)
    if st.has_single:
        return st.best
    else:
        return best_set(st.best, pos + mblen, Node(id=m.unk_id, score=m.min_score - K_UNK_PENALTY + score_here, starts_at=pos))


def char_width_at(text, pos):
    return unwrap_or(codePointWidth(unwrap_or(codePointAt(text, pos), 63)), 1)


def consider_token_at(m, rest, pos, mblen, score_here, st, ti):
    if ti >= len(m.tokens):
        return st
    else:
        token = unwrap_or(listGet(m.tokens, ti), "")
        if startsWith(rest, token):
            key_pos = pos + len(token)
            candidate = unwrap_or(listGet(m.scores, ti), 0.0) + score_here
            target = unwrap_or(listGet(st.best, key_pos), Node(id=0, score=0.0, starts_at=-1))
            better = target.starts_at < 0 or candidate > target.score
            best2 = best_set(st.best, key_pos, Node(id=ti, score=candidate, starts_at=pos)) if better else st.best
            return consider_token_at(m, rest, pos, mblen, score_here, Step(best=best2, has_single=st.has_single or len(token) == mblen), ti + 1)
        else:
            return consider_token_at(m, rest, pos, mblen, score_here, st, ti + 1)


def best_set(best, pos, node):
    """best with position pos replaced by node. A fold, not a map lambda:
    map closures capturing a record leak under ARC (compiler bug, report)."""
    return fold(range(len(best)), [], lambda acc, i: listAppend(acc, node if i == pos else unwrap_or(listGet(best, i), Node(id=0, score=0.0, starts_at=-1))))


def flush_fused(fused, results):
    return results if len(fused) == 0 else listConcat([join(fused, "")], results)


def unigram_backtrack(m, text, best, ends_at, results, fused):
    if ends_at <= 0:
        return flush_fused(fused, results)
    else:
        node = unwrap_or(listGet(best, ends_at), Node(id=0, score=0.0, starts_at=-1))
        starts = node.starts_at
        chunk = unwrap_or(substring(text, starts, ends_at), "")
        if m.fuse_unk and node.id == m.unk_id:
            return unigram_backtrack(m, text, best, starts, results, listConcat([chunk], fused))
        else:
            return unigram_backtrack(m, text, best, starts, listConcat([chunk], flush_fused(fused, results)), [])


# ---------------------------------------------------------------------------
# WordLevel
# ---------------------------------------------------------------------------

def build_wordlevel(vocab, unk_token):
    """A WordLevel model from an id-ordered token list."""
    token_to_id = token_index(vocab)
    return WordLevelModel(tokens=vocab, token_to_id=token_to_id, unk_token=unk_token)


def wordlevel_tokenize(m, text):
    """The whole token, or the unk token when it is not in the vocab."""
    match mapGet(m.token_to_id, text):
        case Success(value=value):
            return [Token(id=value, value=text, start=0, end=len(text))]
        case Error():
            return [Token(id=unk_id(m, m.unk_token), value=m.unk_token, start=0, end=len(text))]


# ---------------------------------------------------------------------------
# Byte-level BPE, reusing @osprey/tiktoken's merge core
# ---------------------------------------------------------------------------

class BpeModel:
    """A byte-level BPE vocabulary: every mergeable token's canonical byte key
    mapped to its id, and the same entries sorted by id for decoding.

    The id doubles as the merge priority, exactly as tiktoken's encoder reads it,
    so a vocabulary whose merge order disagrees with its id order cannot be used
    and is refused rather than tokenized differently.
    """
    ranks: dict[str, int]
    entries: list[Bpe.Ranked]


class Built:
    """A vocabulary taking shape: the ranks so far, the last merge id accepted and
    the index of the first merge that could not be added (-1 while none)."""
    ranks: dict[str, int]
    last: int
    bad: int


def build_bpe(byte_ids, merge_left, merge_right, merge_ids) -> Result[BpeModel, str]:
    """The model from supplied data: one id per byte in byte order, then one merge
    per entry in rank order, each spelled as its byte-level text with its id."""
    if len(byte_ids) != 256:
        return Error(message="a byte-level vocabulary needs 256 byte ids, got " + toString(len(byte_ids)))
    elif len(merge_left) != len(merge_right) or len(merge_left) != len(merge_ids):
        return Error(message="the merge lists disagree in length")
    else:
        return checked_bpe(fold(range(256), {}, lambda acc, b: mapSet(acc, Bpe.key([b]), at(byte_ids, b))), merge_left, merge_right, merge_ids)


def checked_bpe(base, merge_left, merge_right, merge_ids) -> Result[BpeModel, str]:
    built = fold(range(len(merge_left)), Built(ranks=base, last=-1, bad=-1), lambda acc, i: add_merge(acc, merge_left, merge_right, merge_ids, i))
    if built.bad >= 0:
        return Error(message="merge " + toString(built.bad) + " is unusable: its halves must be byte-level text and its id must exceed every earlier merge's")
    else:
        return Success(value=BpeModel(ranks=built.ranks, entries=Bpe.entries_by_rank(built.ranks)))


def add_merge(state, left, right, ids, i):
    """Add one merge when its halves decode to bytes and its id is the highest yet."""
    if state.bad >= 0:
        return state
    else:
        return merge_step(state, i, unwrap_or(listGet(left, i), ""), unwrap_or(listGet(right, i), ""), at(ids, i))


def merge_step(state, i, left, right, id):
    match bytelevel_raw(left + right):
        case Success(value=octets):
            if id <= state.last:
                return Built(ranks=state.ranks, last=state.last, bad=i)
            else:
                return Built(ranks=mapSet(state.ranks, Bpe.key(octets), id), last=id, bad=-1)
        case Error(message=_message):
            return Built(ranks=state.ranks, last=state.last, bad=i)


def bpe_tokenize(m, text) -> Result[list[Token], str]:
    """Byte-pair encode one byte-level piece: tiktoken's merge loop over the piece's
    bytes, then each id read back as its token's text and byte span."""
    return continued(bytelevel_raw(text), lambda octets: continued(Bpe.bpe_encode(m.ranks, octets), lambda ids: tokens_by_id(m, ids)))


class Spans:
    """The tokens read so far, the byte offset reached and the index of the first
    id that has no token (-1 while every id does)."""
    tokens: list[Token]
    offset: int
    bad: int


def continued(result, step):
    """The next step's answer on success, or the reason it already failed."""
    match result:
        case Success(value=value):
            return step(value)
        case Error(message=message):
            return Error(message=message)


def tokens_by_id(m, ids) -> Result[list[Token], str]:
    """Each id as the token whose bytes it names, with its byte span in the piece."""
    st = fold(range(len(ids)), Spans(tokens=[], offset=0, bad=-1), lambda acc, i: span_step(m, ids, acc, i))
    if st.bad >= 0:
        return Error(message="no token with id " + toString(at(ids, st.bad)))
    else:
        return Success(value=st.tokens)


def span_step(m, ids, acc, i):
    if acc.bad >= 0:
        return acc
    else:
        return span_at(m, ids, acc, i)


def span_at(m, ids, acc, i):
    match Bpe.key_of_rank(m.entries, at(ids, i), 0, len(m.entries) - 1):
        case Success(value=key):
            octets = Bpe.bytes_of_key(key)
            return Spans(tokens=listAppend(acc.tokens, Token(id=at(ids, i), value=bytelevel_text(octets), start=acc.offset, end=acc.offset + len(octets))), offset=acc.offset + len(octets), bad=-1)
        case Error(message=_message):
            return Spans(tokens=acc.tokens, offset=acc.offset, bad=i)


def at(xs, i):
    """Element i of a list, or 0 past the end: the model's safe read."""
    return unwrap_or(listGet(xs, i), 0)
