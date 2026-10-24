# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
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

__all__ = [
    "Token", "build_wordpiece", "wordpiece_tokenize", "build_unigram",
    "unigram_tokenize", "unigram_encode", "build_wordlevel", "wordlevel_tokenize",
    "id_to_token", "char_starts",
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
    return char_starts_go(s, 0, len(s), [])


def char_starts_go(s, i, n, acc):
    if i >= n:
        return acc
    else:
        w = unwrap_or(codePointWidth(unwrap_or(codePointAt(s, i), 63)), 1)
        return char_starts_go(s, i + w, n, listAppend(acc, i))


def id_to_token(tokens, id):
    """The vocab token at an id, or the empty string when out of range."""
    return unwrap_or(listGet(tokens, id), "")


# ---------------------------------------------------------------------------
# WordPiece
# ---------------------------------------------------------------------------

def build_wordpiece(vocab, unk_token, prefix, max_chars):
    """A WordPiece model from an id-ordered token list."""
    token_to_id = fold(range(len(vocab)), {}, lambda m, i: mapSet(m, unwrap_or(listGet(vocab, i), ""), i))
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
            s = unwrap_or(listGet(starts, si), 0)
            e = unwrap_or(listGet(starts, found), len(text))
            sub = unwrap_or(substring(text, s, e), "")
            key = m.continuing_prefix + sub if si > 0 else sub
            tok = Token(id=unk_id(m, key), value=key, start=s, end=e)
            return wordpiece_greedy(m, text, starts, found, listAppend(acc, tok))


def longest_match(m, text, starts, si, end_idx):
    """The largest end char index <= end_idx such that text[si:end] (with the
    continuation prefix when si > 0) is in the vocab, or -1."""
    if end_idx <= si:
        return -1
    else:
        s = unwrap_or(listGet(starts, si), 0)
        e = unwrap_or(listGet(starts, end_idx), len(text))
        sub = unwrap_or(substring(text, s, e), "")
        key = m.continuing_prefix + sub if si > 0 else sub
        if mapContains(m.token_to_id, key):
            return end_idx
        else:
            return longest_match(m, text, starts, si, end_idx - 1)


# ---------------------------------------------------------------------------
# Unigram
# ---------------------------------------------------------------------------

def build_unigram(tokens, scores, unk_id, byte_fallback):
    """A Unigram model from parallel token and score lists."""
    token_to_id = fold(range(len(tokens)), {}, lambda m, i: mapSet(m, unwrap_or(listGet(tokens, i), ""), i))
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
    st = fold(range(len(m.tokens)), Step(best=best, has_single=False), lambda acc, ti: consider_token_at(m, rest, pos, mblen, score_here, acc, ti))
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
    token_to_id = fold(range(len(vocab)), {}, lambda m, i: mapSet(m, unwrap_or(listGet(vocab, i), ""), i))
    return WordLevelModel(tokens=vocab, token_to_id=token_to_id, unk_token=unk_token)


def wordlevel_tokenize(m, text):
    """The whole token, or the unk token when it is not in the vocab."""
    match mapGet(m.token_to_id, text):
        case Success(value=value):
            return [Token(id=value, value=text, start=0, end=len(text))]
        case Error():
            return [Token(id=unk_id(m, m.unk_token), value=m.unk_token, start=0, end=len(text))]
