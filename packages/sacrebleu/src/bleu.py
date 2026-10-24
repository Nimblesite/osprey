# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
# osprey: namespace=sacrebleu
# osprey: module=Bleu
"""BLEU (Papineni et al., 2002) with sacreBLEU's smoothing and options.

The `BLEU` record holds the metric's options; `score_sentence` and
`score_corpus` reproduce `BLEU.sentence_score` / `BLEU.corpus_score`, and the
module-level `sentence_bleu` / `corpus_bleu` / `raw_corpus_bleu` are the
backward-compatible entry points of `sacrebleu.compat`. Scores are in [0, 100].
A `smooth_value` below zero selects the method's default (0.1 for `floor`, 1
for `add-k`); sacrebleu spells that default `None`. Match counts and totals are
floats because `add-k` makes them fractional.
"""

from sacrebleu.Ngrams import word_ngrams, word_ngrams_from_tokens, counter_total, counter_matched, counter_merge_max
from sacrebleu.Tokenizer import tokenize

__all__ = ["BLEUScore", "BLEU", "compute_bleu", "score_sentence", "score_corpus", "sentence_bleu", "corpus_bleu", "raw_corpus_bleu"]


class BLEUScore:
    """A BLEU score: value, per-order match counts/totals, precisions (percent),
    brevity penalty, and the system/reference lengths."""
    score: float
    counts: list[float]
    totals: list[float]
    precisions: list[float]
    bp: float
    sys_len: int
    ref_len: int


class BLEU:
    """BLEU options: tokenizer, smoothing, n-gram order and casing."""
    lowercase: bool
    force: bool
    tokenize: str
    smooth_method: str
    smooth_value: float
    max_ngram_order: int
    effective_order: bool


class RefInfo:
    """Pre-computed reference n-gram counter and per-reference lengths."""
    ngrams: dict[str, int]
    lens: list[int]


class SegmentStats:
    """One segment's match statistics: lengths plus per-order counts and totals."""
    sys_len: int
    ref_len: int
    correct: list[float]
    totals: list[float]


def default_smooth(smooth_method):
    """The default smoothing value for a method (floor 0.1, add-k 1, else 0)."""
    if smooth_method == "floor":
        return 0.1
    elif smooth_method == "add-k":
        return 1.0
    else:
        return 0.0


def resolve_smooth(smooth_method, smooth_value):
    """The effective smoothing value: `smooth_value`, or the method's default
    when it is negative."""
    return smooth_value if smooth_value >= 0.0 else default_smooth(smooth_method)


def brevity_penalty(sys_len, ref_len):
    """BLEU's brevity penalty."""
    if sys_len < ref_len:
        return exp(1.0 - toFloat(ref_len) / toFloat(sys_len)) if sys_len > 0 else 0.0
    else:
        return 1.0


def all_zero(xs):
    """Whether every element of a numeric list is zero."""
    return foldList(xs, True, lambda acc, x: acc and x == 0.0)


def zeros_float(n):
    """A list of `n` zeros."""
    return toList(map(range(n), lambda _: 0.0))


def pad_zeros(xs, n):
    """`xs` extended with zeros to length `n`."""
    return fold(range(len(xs), n), xs, lambda acc, _: listAppend(acc, 0.0))


def add_k_adjust(xs, k):
    """`xs` with `k` added to every element except the first (add-k smoothing)."""
    return toList(map(range(len(xs)), lambda i: unwrap_or(listGet(xs, i), 0.0) + (k if i >= 1 else 0.0)))


def first_zero(total, max_order, k):
    """The first index whose total is zero, or `max_order` when none is."""
    if k >= max_order:
        return max_order
    elif unwrap_or(listGet(total, k), 0.0) == 0.0:
        return k
    else:
        return first_zero(total, max_order, k + 1)


def effective_order_value(total, effective_order, max_order):
    """The effective n-gram order: the last order with a nonzero total when
    `effective_order` is set, else `max_order`."""
    return first_zero(total, max_order, 0) if effective_order else max_order


def order_precision_value(c, t, smooth_method, sv, mteval):
    """One order's precision (percent), following the smoothing rules."""
    if c == 0.0 and smooth_method == "exp":
        return 100.0 / (mteval * 2.0 * t)
    elif c == 0.0 and smooth_method == "floor":
        return 100.0 * sv / t
    elif c == 0.0:
        return 0.0
    else:
        return 100.0 * c / t


def precision_loop(correct, total, smooth_method, sv, max_order, n, mteval, acc):
    """The per-order precisions, stopping at the first zero total."""
    if n > max_order:
        return acc
    elif unwrap_or(listGet(total, n - 1), 0.0) == 0.0:
        return acc
    else:
        c = unwrap_or(listGet(correct, n - 1), 0.0)
        t = unwrap_or(listGet(total, n - 1), 0.0)
        p = order_precision_value(c, t, smooth_method, sv, mteval)
        doubled = mteval * 2.0 if c == 0.0 and smooth_method == "exp" else mteval
        return precision_loop(correct, total, smooth_method, sv, max_order, n + 1, doubled, listAppend(acc, p))


def my_log(x):
    """sacrebleu's floored log: a large negative constant at zero."""
    return -9999999999.0 if x == 0.0 else ln(x)


def sum_logs(precisions, eff):
    """The sum of `my_log` over the first `eff` precisions."""
    return fold(range(eff), 0.0, lambda acc, k: acc + my_log(unwrap_or(listGet(precisions, k), 0.0)))


def compute_bleu(correct, total, sys_len, ref_len, smooth_method, smooth_value, effective_order, max_ngram_order):
    """BLEU from its sufficient statistics, with smoothing (sacrebleu
    `BLEU.compute_bleu`)."""
    sv = resolve_smooth(smooth_method, smooth_value)
    bp = brevity_penalty(sys_len, ref_len)
    adj_correct = add_k_adjust(correct, sv) if smooth_method == "add-k" else correct
    adj_total = add_k_adjust(total, sv) if smooth_method == "add-k" else total
    if all_zero(correct):
        return BLEUScore(score=0.0, counts=correct, totals=total,
                         precisions=zeros_float(max_ngram_order), bp=bp,
                         sys_len=sys_len, ref_len=ref_len)
    else:
        precisions = pad_zeros(precision_loop(adj_correct, adj_total, smooth_method, sv, max_ngram_order, 1, 1.0, []), max_ngram_order)
        eff = effective_order_value(adj_total, effective_order, max_ngram_order)
        score = bp * exp(sum_logs(precisions, eff) / toFloat(eff))
        return BLEUScore(score=score, counts=adj_correct, totals=adj_total,
                         precisions=precisions, bp=bp, sys_len=sys_len, ref_len=ref_len)


def closest_ref_len(hyp_len, ref_lens):
    """The reference length closest to `hyp_len`; ties prefer the shorter."""
    return foldList(ref_lens, -1, lambda best, rl: rl if best == -1 or abs(hyp_len - rl) < abs(hyp_len - best) or (abs(hyp_len - rl) == abs(hyp_len - best) and rl < best) else best)


def preprocess(bleu, sent):
    """Lowercase (optionally), strip trailing whitespace, then tokenize."""
    lowered = toLowerCase(sent) if bleu.lowercase else sent
    return tokenize(trimEnd(lowered), bleu.tokenize)


def ref_info(refs, max_order):
    """The merged reference counter and reference lengths for one segment."""
    seed = RefInfo(ngrams= {}, lens=[])
    return foldList(refs, seed, lambda acc, ref: RefInfo(
        ngrams=counter_merge_max(acc.ngrams, word_ngrams(ref, 1, max_order)),
        lens=listAppend(acc.lens, len(words(ref)))))


def matched_order(tokens, ref_counter, order):
    """Match count for order `order`: sum of min(hyp, ref) over its n-grams."""
    return toFloat(counter_matched(word_ngrams_from_tokens(tokens, order), ref_counter))


def total_order(tokens, order):
    """Total n-gram count for order `order`."""
    return toFloat(len(tokens) - order + 1) if len(tokens) >= order else 0.0


def segment_stats(bleu, hyp, info):
    """Match statistics of one tokenized hypothesis against precomputed refs."""
    tokens = words(hyp)
    correct = toList(map(range(bleu.max_ngram_order), lambda n: matched_order(tokens, info.ngrams, n + 1)))
    totals = toList(map(range(bleu.max_ngram_order), lambda n: total_order(tokens, n + 1)))
    return SegmentStats(sys_len=len(tokens), ref_len=closest_ref_len(len(tokens), info.lens),
                        correct=correct, totals=totals)


def add_float_lists(xs, ys):
    """Elementwise sum of two float lists."""
    return toList(map(range(len(xs)), lambda i: unwrap_or(listGet(xs, i), 0.0) + unwrap_or(listGet(ys, i), 0.0)))


def sum_stats(a, b):
    """The elementwise sum of two segment statistics."""
    return SegmentStats(sys_len=a.sys_len + b.sys_len, ref_len=a.ref_len + b.ref_len,
                        correct=add_float_lists(a.correct, b.correct), totals=add_float_lists(a.totals, b.totals))


def score_sentence(bleu, hypothesis, references):
    """BLEU for one hypothesis against one or more references."""
    info = ref_info(mapList(references, lambda r: preprocess(bleu, r)), bleu.max_ngram_order)
    stats = segment_stats(bleu, preprocess(bleu, hypothesis), info)
    return compute_bleu(stats.correct, stats.totals, stats.sys_len, stats.ref_len,
                        bleu.smooth_method, bleu.smooth_value, bleu.effective_order, bleu.max_ngram_order)


def transpose_row(docs, i):
    """Reference segment `i` of every document."""
    return toList(map(range(len(docs)), lambda d: unwrap_or(listGet(unwrap_or(listGet(docs, d), []), i), "")))


def transpose(docs):
    """Documents (each a list of segments) as per-segment reference lists."""
    n = foldList(docs, 0, lambda m, doc: len(doc) if len(doc) > m else m)
    return toList(map(range(n), lambda i: transpose_row(docs, i)))


def score_corpus(bleu, hypotheses, references):
    """BLEU over a corpus; `references` is a list of reference documents."""
    seg_refs = transpose(references)
    seed = SegmentStats(sys_len=0, ref_len=0, correct=zeros_float(bleu.max_ngram_order), totals=zeros_float(bleu.max_ngram_order))
    stats = fold(range(len(hypotheses)), seed, lambda acc, i: sum_stats(acc, segment_stats(
        bleu, preprocess(bleu, unwrap_or(listGet(hypotheses, i), "")),
        ref_info(mapList(unwrap_or(listGet(seg_refs, i), []), lambda r: preprocess(bleu, r)), bleu.max_ngram_order))))
    return compute_bleu(stats.correct, stats.totals, stats.sys_len, stats.ref_len,
                        bleu.smooth_method, bleu.smooth_value, bleu.effective_order, bleu.max_ngram_order)


def sentence_bleu(hypothesis, references, smooth_method, smooth_value, lowercase, tokenize, use_effective_order):
    """`sacrebleu.sentence_bleu`: sentence-level BLEU (compat entry point)."""
    metric = BLEU(lowercase=lowercase, force=False, tokenize=tokenize, smooth_method=smooth_method,
                  smooth_value=smooth_value, max_ngram_order=4, effective_order=use_effective_order)
    return score_sentence(metric, hypothesis, references)


def corpus_bleu(hypotheses, references, smooth_method, smooth_value, force, lowercase, tokenize, use_effective_order):
    """`sacrebleu.corpus_bleu`: corpus-level BLEU (compat entry point)."""
    metric = BLEU(lowercase=lowercase, force=force, tokenize=tokenize, smooth_method=smooth_method,
                  smooth_value=smooth_value, max_ngram_order=4, effective_order=use_effective_order)
    return score_corpus(metric, hypotheses, references)


def raw_corpus_bleu(hypotheses, references, smooth_value):
    """`sacrebleu.raw_corpus_bleu`: floor smoothing, no tokenization."""
    return corpus_bleu(hypotheses, references, "floor", smooth_value, True, False, "none", True)
