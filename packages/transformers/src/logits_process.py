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

from transformers.Math import softmax, log_softmax, neg_inf
from transformers.Lists import contains_int, last_n

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
    temperature: float
    top_k: int
    filter_value: float
    min_tokens_to_keep: int
    top_p: float
    min_p: float
    mass: float
    epsilon: float
    penalty: float
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


def blank() -> Processor:
    """A processor with every setting at its neutral value: temperature 1, -inf
    filter, one token kept, full nucleus, no penalties, no ids. Each constructor
    updates only the fields its processor reads."""
    return Processor(kind="", temperature=1.0, top_k=1, filter_value=neg_inf(), min_tokens_to_keep=1, top_p=1.0, min_p=0.0, mass=0.9, epsilon=0.1, penalty=1.0, prompt_ignore_length=0, ngram_size=2, min_length=0, eos_ids=[], prompt_length_to_skip=0, min_new_tokens=0, bos_token_id=0, max_length=0, suppress_ids=[], bad_words_ids=[])


def TemperatureLogitsWarper(temperature: float) -> Processor:
    """`TemperatureLogitsWarper(temperature)`: divide every logit by the
    strictly positive temperature."""
    base = blank()
    return update(base, kind="temperature", temperature=temperature)


def TopKLogitsWarper(top_k: int, filter_value: float, min_tokens_to_keep: int) -> Processor:
    """`TopKLogitsWarper(top_k, filter_value, min_tokens_to_keep)`: keep the
    top-k scores, masking the rest with `filter_value`; the kept count is at
    least `min_tokens_to_keep`."""
    base = blank()
    return update(base, kind="top_k", top_k=top_k, filter_value=filter_value, min_tokens_to_keep=min_tokens_to_keep)


def TopPLogitsWarper(top_p: float, filter_value: float, min_tokens_to_keep: int) -> Processor:
    """`TopPLogitsWarper(top_p, filter_value, min_tokens_to_keep)`: keep the
    smallest set of tokens whose ascending cumulative softmax exceeds
    `1 - top_p`, masking the rest with `filter_value`."""
    base = blank()
    return update(base, kind="top_p", filter_value=filter_value, min_tokens_to_keep=min_tokens_to_keep, top_p=top_p)


def MinPLogitsWarper(min_p: float, filter_value: float, min_tokens_to_keep: int) -> Processor:
    """`MinPLogitsWarper(min_p, filter_value, min_tokens_to_keep)`: keep tokens
    whose probability is at least `min_p` times the top probability, masking the
    rest with `filter_value`; the top `min_tokens_to_keep` always survive."""
    base = blank()
    return update(base, kind="min_p", filter_value=filter_value, min_tokens_to_keep=min_tokens_to_keep, min_p=min_p)


def TypicalLogitsWarper(mass: float, filter_value: float, min_tokens_to_keep: int) -> Processor:
    """`TypicalLogitsWarper(mass, filter_value, min_tokens_to_keep)`: keep the
    tokens whose distance from the entropy is below the boundary implied by
    `mass`, masking the rest with `filter_value`."""
    base = blank()
    return update(base, kind="typical", filter_value=filter_value, min_tokens_to_keep=min_tokens_to_keep, mass=mass)


def EpsilonLogitsWarper(epsilon: float, filter_value: float, min_tokens_to_keep: int) -> Processor:
    """`EpsilonLogitsWarper(epsilon, filter_value, min_tokens_to_keep)`: keep
    tokens whose probability is at least `epsilon`, masking the rest with
    `filter_value`; the top `min_tokens_to_keep` always survive."""
    base = blank()
    return update(base, kind="epsilon", filter_value=filter_value, min_tokens_to_keep=min_tokens_to_keep, epsilon=epsilon)


def EtaLogitsWarper(epsilon: float, filter_value: float, min_tokens_to_keep: int) -> Processor:
    """`EtaLogitsWarper(epsilon, filter_value, min_tokens_to_keep)`: keep tokens
    whose probability is at least `min(epsilon, sqrt(epsilon) * exp(-entropy))`,
    masking the rest with `filter_value`."""
    base = blank()
    return update(base, kind="eta", filter_value=filter_value, min_tokens_to_keep=min_tokens_to_keep, epsilon=epsilon)


def RepetitionPenaltyLogitsProcessor(penalty: float, prompt_ignore_length: int) -> Processor:
    """`RepetitionPenaltyLogitsProcessor(penalty, prompt_ignore_length)`: divide
    positive scores and multiply negative scores of each token already present
    (after skipping the first `prompt_ignore_length` history tokens) by
    `penalty`."""
    base = blank()
    return update(base, kind="repetition_penalty", penalty=penalty, prompt_ignore_length=prompt_ignore_length)


def NoRepeatNGramLogitsProcessor(ngram_size: int) -> Processor:
    """`NoRepeatNGramLogitsProcessor(ngram_size)`: mask with -inf every token
    that would complete an n-gram that already appears in the history."""
    base = blank()
    return update(base, kind="no_repeat_ngram", ngram_size=ngram_size)


def MinLengthLogitsProcessor(min_length: int, eos_ids: list[int]) -> Processor:
    """`MinLengthLogitsProcessor(min_length, eos_ids)`: mask every eos id with
    -inf while the sequence is shorter than `min_length`."""
    base = blank()
    return update(base, kind="min_length", min_length=min_length, eos_ids=eos_ids)


def MinNewTokensLengthLogitsProcessor(prompt_length_to_skip: int, min_new_tokens: int, eos_ids: list[int]) -> Processor:
    """`MinNewTokensLengthLogitsProcessor(prompt_length_to_skip, min_new_tokens,
    eos_ids)`: mask every eos id with -inf while fewer than `min_new_tokens`
    tokens follow the skipped prompt."""
    base = blank()
    return update(base, kind="min_new_tokens", eos_ids=eos_ids, prompt_length_to_skip=prompt_length_to_skip, min_new_tokens=min_new_tokens)


def ForcedBOSTokenLogitsProcessor(bos_token_id: int) -> Processor:
    """`ForcedBOSTokenLogitsProcessor(bos_token_id)`: when the history is one
    token long, force `bos_token_id` (everything else -inf)."""
    base = blank()
    return update(base, kind="forced_bos", bos_token_id=bos_token_id)


def ForcedEOSTokenLogitsProcessor(max_length: int, eos_ids: list[int]) -> Processor:
    """`ForcedEOSTokenLogitsProcessor(max_length, eos_ids)`: when the history is
    `max_length - 1` tokens long, force the eos ids (everything else -inf)."""
    base = blank()
    return update(base, kind="forced_eos", eos_ids=eos_ids, max_length=max_length)


def SuppressTokensLogitsProcessor(suppress_ids: list[int]) -> Processor:
    """`SuppressTokensLogitsProcessor(suppress_ids)`: mask every listed token
    with -inf."""
    base = blank()
    return update(base, kind="suppress_tokens", suppress_ids=suppress_ids)


def NoBadWordsLogitsProcessor(bad_words_ids: list[list[int]], eos_ids: list[int]) -> Processor:
    """`NoBadWordsLogitsProcessor(bad_words_ids, eos_token_id)`: mask with -inf the
    last token of any bad word the history is about to complete, and every
    single-token bad word; single-token bad words equal to an eos id are ignored."""
    base = blank()
    return update(base, kind="bad_words", eos_ids=eos_ids, bad_words_ids=bad_words_ids)


def apply_list(processors, input_ids, scores):
    """`LogitsProcessorList.__call__`: fold the processors left to right over the
    scores, so the first processor in the list runs first."""
    return foldList(processors, scores, lambda s, p: apply(p, input_ids, s))


def apply(processor, input_ids, scores):
    """Run one `Processor` over a token history and a logits row."""
    match processor.kind:
        case "temperature":
            return mapList(scores, lambda x: x / processor.temperature)
        case "top_k":
            return top_k_warper(max_i(processor.top_k, processor.min_tokens_to_keep), processor.filter_value, scores)
        case "top_p":
            return top_p_warper(processor.top_p, processor.filter_value, processor.min_tokens_to_keep, scores)
        case "min_p":
            return min_p_warper(processor.min_p, processor.filter_value, processor.min_tokens_to_keep, scores)
        case _:
            return mass_processor(processor, input_ids, scores)


def mass_processor(processor, input_ids, scores):
    """The probability-mass warpers and token suppression; any other kind reads
    the token history."""
    match processor.kind:
        case "typical":
            return typical_warper(processor.mass, processor.filter_value, processor.min_tokens_to_keep, scores)
        case "epsilon":
            return epsilon_warper(processor.epsilon, processor.filter_value, processor.min_tokens_to_keep, scores)
        case "eta":
            return eta_warper(processor.epsilon, processor.filter_value, processor.min_tokens_to_keep, scores)
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

def max_i(a, b):
    return a if a > b else b


def min_i(a, b):
    return a if a < b else b


def min_f(a, b):
    return a if a < b else b


def max_f(a, x):
    return x if x > a else a


def cmp_f(a, b):
    return -1 if a < b else (1 if a > b else 0)


def cmp_i(a, b):
    return -1 if a < b else (1 if a > b else 0)


def score_at(xs, i):
    return unwrap_or(listGet(xs, i), neg_inf())


def prob_at(xs, i):
    return unwrap_or(listGet(xs, i), 0.0)


def cumsum(xs):
    """Prefix sums of a float list."""
    return cumsum_from(xs, 0, 0.0, [])


def cumsum_from(xs, i, running, acc):
    if i == len(xs):
        return acc
    else:
        r = running + unwrap_or(listGet(xs, i), 0.0)
        return cumsum_from(xs, i + 1, r, listAppend(acc, r))


def entropy(logits):
    """`-sum(p * log_softmax(logits))` over finite entries, i.e. torch's
    `Categorical(logits=...).entropy()`; -inf logits (probability 0) contribute
    0 and NaN propagates when every logit is -inf."""
    nls = log_softmax(logits)
    return fold(range(len(nls)), 0.0, lambda acc, i: acc + entropy_term(unwrap_or(listGet(nls, i), neg_inf())))


def entropy_term(x):
    return 0.0 if x == neg_inf() else (0.0 - x) * exp(x)


def sort_indices_by(values):
    """Indices 0..n-1 sorted by ascending value, ties broken by smaller index."""
    idx = toList(map(range(len(values)), lambda i: i))
    return listSortBy(idx, lambda a, b: cmp_entries(values, a, b))


def cmp_entries(values, a, b):
    c = cmp_f(score_at(values, a), score_at(values, b))
    return c if c != 0 else cmp_i(a, b)


def sorted_pos(idx, i):
    """The sorted position of original index `i`."""
    return fold(range(len(idx)), -1, lambda found, j: found if found != -1 else (j if unwrap_or(listGet(idx, j), -1) == i else -1))


def kth_largest(scores, k):
    """The k-th largest score (1-indexed), the threshold torch.topk uses."""
    desc = listSortBy(scores, lambda a, b: cmp_f(b, a))
    return unwrap_or(listGet(desc, k - 1), neg_inf())


def top_k_indices(scores, k):
    """Indices of the k largest scores, ties broken by smaller index."""
    idx = toList(map(range(len(scores)), lambda i: i))
    ordered = listSortBy(idx, lambda a, b: cmp_entries_desc(scores, a, b))
    return toList(map(range(k), lambda i: unwrap_or(listGet(ordered, i), 0)))


def cmp_entries_desc(scores, a, b):
    c = cmp_f(score_at(scores, b), score_at(scores, a))
    return c if c != 0 else cmp_i(a, b)


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
    sorted_scores = mapList(idx, lambda j: score_at(scores, j))
    cum = cumsum(softmax(sorted_scores))
    cutoff = 1.0 - p
    keep_from = len(scores) - m
    return toList(map(range(len(scores)), lambda i: fv if removed_top_p(i, idx, cum, cutoff, keep_from) else score_at(scores, i)))


def removed_top_p(i, idx, cum, cutoff, keep_from):
    j = sorted_pos(idx, i)
    return score_at(cum, j) <= cutoff and j < keep_from


def min_p_warper(p, fv, m, scores):
    probs = softmax(scores)
    scaled = p * fold(range(len(probs)), neg_inf(), lambda a, i: max_f(a, unwrap_or(listGet(probs, i), neg_inf())))
    keep = top_k_indices(scores, min_i(m, len(scores)))
    return toList(map(range(len(scores)), lambda i: fv if prob_at(probs, i) < scaled and not contains_int(keep, i) else score_at(scores, i)))


def typical_warper(mass, fv, m, scores):
    shifted = mapList(log_softmax(scores), lambda x: abs((0.0 - x) - entropy(scores)))
    idx = sort_indices_by(shifted)
    sorted_shifted = mapList(idx, lambda j: score_at(shifted, j))
    sorted_logits = mapList(idx, lambda j: score_at(scores, j))
    cum = cumsum(softmax(sorted_logits))
    last = min_i(count_lt(cum, mass), len(scores) - 1)
    threshold = score_at(sorted_shifted, last)
    return toList(map(range(len(scores)), lambda i: fv if removed_typical(i, idx, shifted, threshold, m) else score_at(scores, i)))


def removed_typical(i, idx, shifted, threshold, m):
    j = sorted_pos(idx, i)
    return j >= m and score_at(shifted, i) > threshold


def epsilon_warper(e, fv, m, scores):
    probs = softmax(scores)
    threshold = kth_largest(scores, min_i(m, len(scores)))
    return toList(map(range(len(scores)), lambda i: fv if prob_at(probs, i) < e and score_at(scores, i) < threshold else score_at(scores, i)))


def eta_warper(e, fv, m, scores):
    probs = softmax(scores)
    eta = min_f(e, sqrt(e) * exp(0.0 - entropy(scores)))
    threshold = kth_largest(scores, min_i(m, len(scores)))
    return toList(map(range(len(scores)), lambda i: fv if prob_at(probs, i) < eta and score_at(scores, i) < threshold else score_at(scores, i)))


# --- processors over the token history ------------------------------------

def repetition_warper(pen, ig, ids, scores):
    tail = toList(map(range(len(ids) - ig), lambda i: unwrap_or(listGet(ids, i + ig), 0)))
    return toList(map(range(len(scores)), lambda i: penalized(score_at(scores, i), pen) if contains_int(tail, i) else score_at(scores, i)))


def penalized(score, penalty):
    return score * penalty if score < 0.0 else score / penalty


def no_repeat_warper(n, ids, scores):
    if len(ids) < n:
        return scores
    else:
        prefix = last_n(ids, n - 1)
        banned = fold(range(len(ids) - n + 1), [], lambda acc, s: listAppend(acc, unwrap_or(listGet(ids, s + n - 1), 0)) if window_prefix_matches(ids, s, n - 1, prefix) else acc)
        return toList(map(range(len(scores)), lambda i: neg_inf() if contains_int(banned, i) else score_at(scores, i)))


def window_prefix_matches(ids, s, n1, prefix):
    return fold(range(n1), True, lambda ok, j: ok and unwrap_or(listGet(ids, s + j), -1) == unwrap_or(listGet(prefix, j), -1))


def min_length_warper(ml, eos, ids, scores):
    if len(ids) < ml:
        return toList(map(range(len(scores)), lambda i: neg_inf() if contains_int(eos, i) else score_at(scores, i)))
    else:
        return scores


def min_new_warper(skip, new, eos, ids, scores):
    if len(ids) - skip < new:
        return toList(map(range(len(scores)), lambda i: neg_inf() if contains_int(eos, i) else score_at(scores, i)))
    else:
        return scores


def forced_bos_warper(bos, ids, scores):
    if len(ids) == 1:
        return toList(map(range(len(scores)), lambda i: 0.0 if i == bos else neg_inf()))
    else:
        return scores


def forced_eos_warper(ml, eos, ids, scores):
    if len(ids) == ml - 1:
        return toList(map(range(len(scores)), lambda i: 0.0 if contains_int(eos, i) else neg_inf()))
    else:
        return scores


def suppress_warper(supp, scores):
    return toList(map(range(len(scores)), lambda i: neg_inf() if contains_int(supp, i) else score_at(scores, i)))


def bad_words_warper(bw, eos, ids, scores):
    filtered = filterList(bw, lambda w: not (len(w) == 1 and contains_int(eos, unwrap_or(listGet(w, 0), -1))))
    return foldList(filtered, scores, lambda acc, w: ban_one(w, ids, acc))


def ban_one(w, ids, scores):
    if len(w) == 1:
        return set_neg_inf(scores, unwrap_or(listGet(w, 0), -1))
    else:
        return set_neg_inf(scores, last_token(w)) if ends_with(ids, all_but_last(w)) else scores


def ends_with(xs, suffix):
    if len(xs) < len(suffix):
        return False
    else:
        return fold(range(len(suffix)), True, lambda ok, j: ok and unwrap_or(listGet(xs, len(xs) - len(suffix) + j), -1) == unwrap_or(listGet(suffix, j), -1))


def all_but_last(xs):
    return toList(map(range(len(xs) - 1), lambda i: unwrap_or(listGet(xs, i), 0)))


def last_token(xs):
    return unwrap_or(listGet(xs, len(xs) - 1), 0)


def set_neg_inf(scores, i):
    return toList(map(range(len(scores)), lambda j: neg_inf() if j == i else score_at(scores, j)))
