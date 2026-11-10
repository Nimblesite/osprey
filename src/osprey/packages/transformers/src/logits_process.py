# osprey: flavor=python
# osprey: namespace=transformers
# osprey: module=LogitsProcess
"""Logits processors and warpers with Hugging Face semantics.

Each exported constructor returns a `Processor` record holding one
configuration; `apply` runs it over a token history (`list[int]`) and a logits
row (`list[float]`), and `apply_list` composes a `list[Processor]` left to right
exactly as `LogitsProcessorList.__call__` does. The warpers implement
transformers 5.20.2's `transformers/generation/logits_process.py` semantics on
one batch row: the same comparisons, the same ascending softmax/cumsum order,
and `-inf` as the filter value. `filter_value` and `min_tokens_to_keep` have no
Osprey default arguments, so every constructor takes them explicitly (see
README).

The torch CPU sort used by top-p and typical decoding is only stable for short
tensors, so this port sorts ascending with ties broken by the smaller index;
that is exactly what torch.sort does for the vocab sizes this package is tested
at, and it is the documented tie order for larger vocabs.
"""

from transformers.Math import log_softmax, max_f, max_i, min_f, min_i, neg_inf, softmax
from transformers.Lists import contains_int, last_n, map_indexed

__all__ = [
    "Processor",
    "TemperatureLogitsWarper",
    "TopKLogitsWarper",
    "TopPLogitsWarper",
    "MinPLogitsWarper",
    "TypicalLogitsWarper",
    "EpsilonLogitsWarper",
    "EtaLogitsWarper",
    "RepetitionPenaltyLogitsProcessor",
    "NoRepeatNGramLogitsProcessor",
    "MinLengthLogitsProcessor",
    "MinNewTokensLengthLogitsProcessor",
    "ForcedBOSTokenLogitsProcessor",
    "ForcedEOSTokenLogitsProcessor",
    "SuppressTokensLogitsProcessor",
    "NoBadWordsLogitsProcessor",
    "apply",
    "apply_list",
]


class Processor:
    """One processor's configuration; `kind` selects the behaviour `apply`
    dispatches on, and each constructor leaves the unrelated fields blank."""
    kind: str
    temperature: float32
    top_k: int
    filter_value: float32
    min_tokens_to_keep: int
    setting: float32
    penalty: float32
    prompt_ignore_length: int
    ngram_size: int
    min_length: int
    eos_ids: list[int]
    prompt_length_to_skip: int
    min_new_tokens: int
    bos_token_id: int
    max_length: int
    suppress_ids: list[int]
    bad_words_ids: list[list[int]]


def blank():
    """A processor with every setting at its neutral value: temperature 1, -inf
    filter, one token kept, full nucleus, no penalties, no ids. Each constructor
    updates only the fields its processor reads."""
    return Processor(kind="", temperature=1.0, top_k=1, filter_value=neg_inf(), min_tokens_to_keep=1, setting=0.0, penalty=1.0, prompt_ignore_length=0, ngram_size=2, min_length=0, eos_ids=[], prompt_length_to_skip=0, min_new_tokens=0, bos_token_id=0, max_length=0, suppress_ids=[], bad_words_ids=[])


def warper(kind, setting, filter_value, min_tokens_to_keep):
    """A mass or threshold warper: the setting its `kind` reads, plus the filter
    value and the kept-token floor every warper shares."""
    return update(blank(), kind=kind, setting=setting, filter_value=filter_value, min_tokens_to_keep=min_tokens_to_keep)


def TemperatureLogitsWarper(temperature):
    """`TemperatureLogitsWarper(temperature)`: divide every logit by the
    strictly positive temperature."""
    return update(blank(), kind="temperature", temperature=temperature)


def TopKLogitsWarper(top_k, filter_value, min_tokens_to_keep):
    """`TopKLogitsWarper(top_k, filter_value, min_tokens_to_keep)`: keep the
    top-k scores, masking the rest with `filter_value`; the kept count is at
    least `min_tokens_to_keep`."""
    return update(blank(), kind="top_k", top_k=top_k, filter_value=filter_value, min_tokens_to_keep=min_tokens_to_keep)


def TopPLogitsWarper(top_p, filter_value, min_tokens_to_keep):
    """`TopPLogitsWarper(top_p, filter_value, min_tokens_to_keep)`: keep the
    smallest set of tokens whose ascending cumulative softmax exceeds
    `1 - top_p`, masking the rest with `filter_value`."""
    return warper("top_p", top_p, filter_value, min_tokens_to_keep)


def MinPLogitsWarper(min_p, filter_value, min_tokens_to_keep):
    """`MinPLogitsWarper(min_p, filter_value, min_tokens_to_keep)`: keep tokens
    whose probability is at least `min_p` times the top probability, masking the
    rest with `filter_value`; the top `min_tokens_to_keep` always survive."""
    return warper("min_p", min_p, filter_value, min_tokens_to_keep)


def TypicalLogitsWarper(mass, filter_value, min_tokens_to_keep):
    """`TypicalLogitsWarper(mass, filter_value, min_tokens_to_keep)`: keep the
    tokens whose distance from the entropy is below the boundary implied by
    `mass`, masking the rest with `filter_value`."""
    return warper("typical", mass, filter_value, min_tokens_to_keep)


def EpsilonLogitsWarper(epsilon, filter_value, min_tokens_to_keep):
    """`EpsilonLogitsWarper(epsilon, filter_value, min_tokens_to_keep)`: keep
    tokens whose probability is at least `epsilon`, masking the rest with
    `filter_value`; the top `min_tokens_to_keep` always survive."""
    return warper("epsilon", epsilon, filter_value, min_tokens_to_keep)


def EtaLogitsWarper(epsilon, filter_value, min_tokens_to_keep):
    """`EtaLogitsWarper(epsilon, filter_value, min_tokens_to_keep)`: keep tokens
    whose probability is at least `min(epsilon, sqrt(epsilon) * exp(-entropy))`,
    masking the rest with `filter_value`."""
    return warper("eta", epsilon, filter_value, min_tokens_to_keep)


def RepetitionPenaltyLogitsProcessor(penalty, prompt_ignore_length):
    """`RepetitionPenaltyLogitsProcessor(penalty, prompt_ignore_length)`: divide
    positive scores and multiply negative scores of each token already present
    (after skipping the first `prompt_ignore_length` history tokens) by
    `penalty`."""
    return update(blank(), kind="repetition_penalty", penalty=penalty, prompt_ignore_length=prompt_ignore_length)


def NoRepeatNGramLogitsProcessor(ngram_size):
    """`NoRepeatNGramLogitsProcessor(ngram_size)`: mask with -inf every token
    that would complete an n-gram that already appears in the history."""
    return update(blank(), kind="no_repeat_ngram", ngram_size=ngram_size)


def MinLengthLogitsProcessor(min_length, eos_ids):
    """`MinLengthLogitsProcessor(min_length, eos_ids)`: mask every eos id with
    -inf while the sequence is shorter than `min_length`."""
    return update(blank(), kind="min_length", min_length=min_length, eos_ids=eos_ids)


def MinNewTokensLengthLogitsProcessor(prompt_length_to_skip, min_new_tokens, eos_ids):
    """`MinNewTokensLengthLogitsProcessor(prompt_length_to_skip, min_new_tokens,
    eos_ids)`: mask every eos id with -inf while fewer than `min_new_tokens`
    tokens follow the skipped prompt."""
    return update(blank(), kind="min_new_tokens", eos_ids=eos_ids, prompt_length_to_skip=prompt_length_to_skip, min_new_tokens=min_new_tokens)


def ForcedBOSTokenLogitsProcessor(bos_token_id):
    """`ForcedBOSTokenLogitsProcessor(bos_token_id)`: when the history is one
    token long, force `bos_token_id` (everything else -inf)."""
    return update(blank(), kind="forced_bos", bos_token_id=bos_token_id)


def ForcedEOSTokenLogitsProcessor(max_length, eos_ids):
    """`ForcedEOSTokenLogitsProcessor(max_length, eos_ids)`: when the history is
    `max_length - 1` tokens long, force the eos ids (everything else -inf)."""
    return update(blank(), kind="forced_eos", eos_ids=eos_ids, max_length=max_length)


def SuppressTokensLogitsProcessor(suppress_ids):
    """`SuppressTokensLogitsProcessor(suppress_ids)`: mask every listed token
    with -inf."""
    return update(blank(), kind="suppress_tokens", suppress_ids=suppress_ids)


def NoBadWordsLogitsProcessor(bad_words_ids, eos_ids):
    """`NoBadWordsLogitsProcessor(bad_words_ids, eos_token_id)`: mask with -inf the
    last token of any bad word the history is about to complete, and every
    single-token bad word; single-token bad words equal to an eos id are ignored."""
    return update(blank(), kind="bad_words", eos_ids=eos_ids, bad_words_ids=bad_words_ids)


def apply_list(processors, input_ids, scores: list[float32]):
    """`LogitsProcessorList.__call__`: fold the processors left to right over the
    scores, so the first processor in the list runs first."""
    return foldList(processors, scores, lambda s, p: apply(p, input_ids, s))


def apply(processor: Processor, input_ids, scores: list[float32]):
    """Run one `Processor` over a token history and a logits row."""
    match processor.kind:
        case "temperature":
            return mapList(scores, lambda x: x / processor.temperature)
        case "top_k":
            return top_k_warper(max_i(processor.top_k, processor.min_tokens_to_keep), processor.filter_value, scores)
        case "top_p":
            return tuned(top_p_warper, processor, scores)
        case "min_p":
            return tuned(min_p_warper, processor, scores)
        case _:
            return mass_processor(processor, input_ids, scores)


def tuned(warper, processor, scores):
    """`warper` at the processor's setting, filter value and kept floor."""
    return warper(processor.setting, processor.filter_value, processor.min_tokens_to_keep, scores)


def mass_processor(processor, input_ids, scores):
    """The probability-mass warpers and token suppression; any other kind reads
    the token history."""
    match processor.kind:
        case "typical":
            return tuned(typical_warper, processor, scores)
        case "epsilon":
            return tuned(below_cutoff, processor, scores)
        case "eta":
            return tuned(eta_warper, processor, scores)
        case "suppress_tokens":
            return suppress_warper(processor.suppress_ids, scores)
        case _:
            return history_processor(processor, input_ids, scores)


def history_processor(processor, input_ids, scores):
    """The token-history processors: penalty, n-gram, length, forcing and bad
    words."""
    match processor.kind:
        case "repetition_penalty":
            return repetition_warper(processor.penalty, processor.prompt_ignore_length, input_ids, scores)
        case "no_repeat_ngram":
            return no_repeat_warper(processor.ngram_size, input_ids, scores)
        case "min_length":
            return min_length_warper(processor.min_length, processor.eos_ids, input_ids, scores)
        case "min_new_tokens":
            return min_new_warper(processor.prompt_length_to_skip, processor.min_new_tokens, processor.eos_ids, input_ids, scores)
        case "forced_bos":
            return forced_bos_warper(processor.bos_token_id, input_ids, scores)
        case "forced_eos":
            return forced_eos_warper(processor.max_length, processor.eos_ids, input_ids, scores)
        case "bad_words":
            return bad_words_warper(processor.bad_words_ids, processor.eos_ids, input_ids, scores)
        case _:
            return scores


# --- shared numerics ------------------------------------------------------

def cmp_f(a, b):
    return -1 if a < b else (1 if a > b else 0)


def cmp_i(a, b):
    return -1 if a < b else (1 if a > b else 0)


def get_or(xs, i, fallback):
    """Element `i` of `xs`, or `fallback` when `i` is out of range."""
    return unwrap_or(listGet(xs, i), fallback)


def score_at(xs, i):
    return get_or(xs, i, neg_inf())


def prob_at(xs, i):
    return get_or(xs, i, toFloat32(0.0))


def cumsum(xs):
    """Prefix sums of a float list."""
    return cumsum_from(xs, 0, toFloat32(0.0), [])


def cumsum_from(xs, i, running, acc):
    if i == len(xs):
        return acc
    else:
        r = running + get_or(xs, i, toFloat32(0.0))
        return cumsum_from(xs, i + 1, r, listAppend(acc, r))


def entropy(logits):
    """`-sum(p * log_softmax(logits))` over finite entries, i.e. torch's
    `Categorical(logits=...).entropy()`; -inf logits (probability 0) contribute
    0 and NaN propagates when every logit is -inf."""
    nls = log_softmax(logits)
    return fold(range(len(nls)), toFloat32(0.0), lambda acc, i: acc + entropy_term(get_or(nls, i, neg_inf())))


def entropy_term(x):
    return 0.0 if x == neg_inf() else (0.0 - x) * exp(x)


def sort_indices_by(values):
    """Indices 0..n-1 sorted by ascending value, ties broken by smaller index."""
    idx = map_indexed(values, lambda i: i)
    return listSortBy(idx, lambda a, b: cmp_entries(values, a, b, False))


def in_index_order(idx, xs):
    """`xs` gathered in the order of the index list `idx`."""
    return mapList(idx, lambda j: score_at(xs, j))


def cum_probs(idx, logits):
    """Cumulative softmax of `logits` gathered in `idx` order."""
    return cumsum(softmax(in_index_order(idx, logits)))


def cmp_entries(values, a, b, desc):
    """Order indices by `values`, descending when `desc`, ties by index."""
    va = score_at(values, a)
    vb = score_at(values, b)
    return ranked(a, b, cmp_f(vb, va) if desc else cmp_f(va, vb))


def ranked(a, b, by_value):
    """`by_value`, falling back to ascending index on a tie."""
    return by_value if by_value != 0 else cmp_i(a, b)


def sorted_pos(idx, i):
    """The sorted position of original index `i`."""
    return fold(range(len(idx)), -1, lambda found, j: found if found != -1 else (j if get_or(idx, j, -1) == i else -1))


def kth_largest(scores, k):
    """The k-th largest score (1-indexed), the threshold torch.topk uses."""
    desc = listSortBy(scores, lambda a, b: cmp_f(b, a))
    return get_or(desc, k - 1, neg_inf())


def top_k_indices(scores, k):
    """Indices of the k largest scores, ties broken by smaller index."""
    idx = map_indexed(scores, lambda i: i)
    ordered = listSortBy(idx, lambda a, b: cmp_entries(scores, a, b, True))
    return toList(map(range(k), lambda i: get_or(ordered, i, 0)))


def count_lt(xs, limit):
    """How many elements of `xs` are strictly less than `limit`."""
    return fold(range(len(xs)), 0, lambda n, i: n + 1 if score_at(xs, i) < limit else n)


# --- warpers --------------------------------------------------------------

def top_k_warper(k, fv, scores):
    kk = min_i(k, len(scores))
    threshold = kth_largest(scores, kk)
    return mapList(scores, lambda x: x if x >= threshold else fv)


def top_p_warper(p, fv, m, scores):
    idx = sort_indices_by(scores)
    cum = cum_probs(idx, scores)
    # torch computes `1 - top_p` as a Python float64 scalar and casts it to the
    # tensor's dtype; match that rather than subtracting in binary32, so a
    # boundary lands on the same side torch chose.
    cutoff = toFloat32(1.0 - toFloat(p))
    keep_from = len(scores) - m
    return map_indexed(scores, lambda i: fv if removed_top_p(i, idx, cum, cutoff, keep_from) else score_at(scores, i))


def removed_top_p(i, idx, cum, cutoff, keep_from):
    j = sorted_pos(idx, i)
    return score_at(cum, j) <= cutoff and j < keep_from


def min_p_warper(p, fv, m, scores):
    probs = softmax(scores)
    scaled = p * fold(range(len(probs)), neg_inf(), lambda a, i: max_f(a, get_or(probs, i, neg_inf())))
    keep = top_k_indices(scores, min_i(m, len(scores)))
    return map_indexed(scores, lambda i: fv if prob_at(probs, i) < scaled and not contains_int(keep, i) else score_at(scores, i))


def typical_warper(mass, fv, m, scores):
    shifted = mapList(log_softmax(scores), lambda x: abs((0.0 - x) - entropy(scores)))
    idx = sort_indices_by(shifted)
    sorted_shifted = in_index_order(idx, shifted)
    cum = cum_probs(idx, scores)
    last = min_i(count_lt(cum, mass), len(scores) - 1)
    threshold = score_at(sorted_shifted, last)
    return map_indexed(scores, lambda i: fv if removed_typical(i, idx, shifted, threshold, m) else score_at(scores, i))


def removed_typical(i, idx, shifted, threshold, m):
    j = sorted_pos(idx, i)
    return j >= m and score_at(shifted, i) > threshold


def eta_warper(e, fv, m, scores):
    """The eta threshold warper: `below_cutoff` at eta's entropy-scaled limit."""
    return below_cutoff(min_f(e, sqrt(e) * exp(0.0 - entropy(scores))), fv, m, scores)


def below_cutoff(limit, fv, m, scores):
    """Filter every token whose probability is under `limit`, keeping the `m` best."""
    probs = softmax(scores)
    threshold = kth_largest(scores, min_i(m, len(scores)))
    return map_indexed(scores, lambda i: fv if prob_at(probs, i) < limit and score_at(scores, i) < threshold else score_at(scores, i))


# --- processors over the token history ------------------------------------

def repetition_warper(pen, ig, ids, scores):
    tail = toList(map(range(len(ids) - ig), lambda i: get_or(ids, i + ig, 0)))
    return map_indexed(scores, lambda i: penalized(score_at(scores, i), pen) if contains_int(tail, i) else score_at(scores, i))


def penalized(score, penalty):
    return score * penalty if score < toFloat32(0.0) else score / penalty


def masked(scores, ids):
    """`scores` with every listed token id set to -inf."""
    return map_indexed(scores, lambda i: neg_inf() if contains_int(ids, i) else score_at(scores, i))


def forced(scores, ids):
    """A row that allows only the listed token ids: 0 for them, -inf elsewhere."""
    return map_indexed(scores, lambda i: 0.0 if contains_int(ids, i) else neg_inf())


def no_repeat_warper(n, ids, scores):
    if len(ids) < n:
        return scores
    else:
        prefix = last_n(ids, n - 1)
        banned = fold(range(len(ids) - n + 1), [], lambda acc, s: listAppend(acc, get_or(ids, s + n - 1, 0)) if spans_match(ids, s, prefix) else acc)
        return masked(scores, banned)


def spans_match(xs, offset, ys):
    """Whether the `len(ys)` elements of `xs` from `offset` equal `ys` pairwise."""
    return fold(range(len(ys)), True, lambda ok, j: ok and get_or(xs, offset + j, -1) == get_or(ys, j, -1))


def min_length_warper(ml, eos, ids, scores):
    return masked(scores, eos) if len(ids) < ml else scores


def min_new_warper(skip, new, eos, ids, scores):
    return masked(scores, eos) if len(ids) - skip < new else scores


def forced_bos_warper(bos, ids, scores):
    return forced(scores, [bos]) if len(ids) == 1 else scores


def forced_eos_warper(ml, eos, ids, scores):
    return forced(scores, eos) if len(ids) == ml - 1 else scores


def suppress_warper(supp, scores):
    return masked(scores, supp)


def bad_words_warper(bw, eos, ids, scores):
    filtered = filterList(bw, lambda w: not (len(w) == 1 and contains_int(eos, get_or(w, 0, -1))))
    return foldList(filtered, scores, lambda acc, w: ban_one(w, ids, acc))


def ban_one(w, ids, scores):
    if len(w) == 1:
        return set_neg_inf(scores, get_or(w, 0, -1))
    else:
        return set_neg_inf(scores, last_token(w)) if ends_with(ids, all_but_last(w)) else scores


def ends_with(xs, suffix):
    """Whether `xs` ends with `suffix`, compared token by token."""
    return len(xs) >= len(suffix) and spans_match(xs, len(xs) - len(suffix), suffix)


def all_but_last(xs):
    return toList(map(range(len(xs) - 1), lambda i: get_or(xs, i, 0)))


def last_token(xs):
    return get_or(xs, len(xs) - 1, 0)


def set_neg_inf(scores, i):
    return map_indexed(scores, lambda j: neg_inf() if j == i else score_at(scores, j))

