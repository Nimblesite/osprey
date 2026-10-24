# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
# osprey: namespace=sacrebleu
# osprey: module=Chrf
"""chrF (Popović 2015) and chrF++ (Popović 2017) metrics.

`CHRF` holds the options; `score_sentence` / `score_corpus` reproduce
`CHRF.sentence_score` / `CHRF.corpus_score`; `sentence_chrf` / `corpus_chrf`
are the backward-compatible entry points. Scores are in [0, 100]. The
`whitespace` flag keeps whitespace in character n-grams (the compat entry
points spell its inverse as `remove_whitespace`).
"""

from sacrebleu.Ngrams import char_ngrams, word_ngrams_from_tokens, counter_total, counter_matched

__all__ = ["CHRFScore", "CHRF", "score_sentence", "score_corpus", "sentence_chrf", "corpus_chrf"]


class CHRFScore:
    """A chrF score: the value and the configuration that produced it."""
    score: float
    char_order: int
    word_order: int
    beta: int


class CHRF:
    """chrF options: n-gram orders, beta, casing, whitespace and smoothing."""
    char_order: int
    word_order: int
    beta: int
    lowercase: bool
    whitespace: bool
    eps_smoothing: bool


class ChrFAcc:
    """Accumulated f-score terms over the n-gram orders."""
    score: float
    avg_prec: float
    avg_rec: float
    effective_order: int


def is_punct_byte(b):
    """Whether byte `b` is one of chrF's ASCII punctuation set."""
    return (b >= 33 and b <= 47) or (b >= 58 and b <= 64) or (b >= 91 and b <= 96) or (b >= 123 and b <= 126)


def split_word(acc, w):
    """Append `w` to `acc`, separating a leading or trailing punctuation mark."""
    if length(w) == 1:
        return listAppend(acc, w)
    else:
        last = unwrap_or(byteAt(w, length(w) - 1), 32)
        first = unwrap_or(byteAt(w, 0), 32)
        if is_punct_byte(last):
            return listConcat(acc, [take(w, length(w) - 1), unwrap_or(fromCodePoint(last), "")])
        elif is_punct_byte(first):
            return listConcat(acc, [unwrap_or(fromCodePoint(first), ""), drop(w, 1)])
        else:
            return listAppend(acc, w)


def remove_punctuation(sent):
    """chrF's word tokenization: punctuation separated from word edges."""
    return foldList(words(sent), [], lambda acc, w: split_word(acc, w))


def char_counters(sent, co, ws, n, acc):
    """The char n-gram counters for orders `n`..`co`, appended to `acc`."""
    if n > co:
        return acc
    else:
        return char_counters(sent, co, ws, n + 1, listAppend(acc, char_ngrams(sent, n, ws)))


def word_counters(rwords, wo, n, acc):
    """The word n-gram counters for orders `n`..`wo`, appended to `acc`."""
    if n > wo:
        return acc
    else:
        return word_counters(rwords, wo, n + 1, listAppend(acc, word_ngrams_from_tokens(rwords, n)))


def ngrams_list(sent, chrf):
    """The char n-gram counters (orders 1..char_order) followed by the word
    n-gram counters (orders 1..word_order) of a sentence."""
    co = chrf.char_order
    wo = chrf.word_order
    ws = chrf.whitespace
    chars = char_counters(sent, co, ws, 1, [])
    if wo > 0:
        rwords = remove_punctuation(sent)
        return listConcat(chars, word_counters(rwords, wo, 1, []))
    else:
        return chars


def match_stats(hyp_ngrams, ref_ngrams):
    """The `[hyp, ref, match]` triplet for one n-gram order."""
    hyp_count = 0 if isEmpty(ref_ngrams) else counter_total(hyp_ngrams)
    return [hyp_count, counter_total(ref_ngrams), counter_matched(hyp_ngrams, ref_ngrams)]


def ref_stats(all_hyp, ref_ngs):
    """The match triplets of every order, concatenated for one reference."""
    return fold(range(len(all_hyp)), [], lambda acc, i: listConcat(acc, match_stats(
        unwrap_or(listGet(all_hyp, i), {}), unwrap_or(listGet(ref_ngs, i), {}))))


def chr_f_step(state, i, statistics, factor, eps):
    """One order's contribution to the chrF score."""
    n_hyp = unwrap_or(listGet(statistics, 3 * i), 0)
    n_ref = unwrap_or(listGet(statistics, 3 * i + 1), 0)
    n_match = unwrap_or(listGet(statistics, 3 * i + 2), 0)
    prec = toFloat(n_match) / toFloat(n_hyp) if n_hyp > 0 else eps
    rec = toFloat(n_match) / toFloat(n_ref) if n_ref > 0 else eps
    denom = factor * prec + rec
    contribution = (1.0 + factor) * prec * rec / denom if denom > 0.0 else eps
    counted = n_hyp > 0 and n_ref > 0
    return ChrFAcc(score=state.score + contribution,
                   avg_prec=state.avg_prec + (prec if counted else 0.0),
                   avg_rec=state.avg_rec + (rec if counted else 0.0),
                   effective_order=state.effective_order + (1 if counted else 0))


def chr_f_score(statistics, beta, order, eps_smoothing):
    """The final chrF score in [0, 100] from the flattened match triplets."""
    eps = 0.0000000000000001
    factor = toFloat(beta * beta)
    state = fold(range(order), ChrFAcc(score=0.0, avg_prec=0.0, avg_rec=0.0, effective_order=0),
                 lambda acc, i: chr_f_step(acc, i, statistics, factor, eps))
    if eps_smoothing:
        return 100.0 * state.score / toFloat(order)
    else:
        eff = state.effective_order
        avg_prec = state.avg_prec / toFloat(eff) if eff > 0 else 0.0
        avg_rec = state.avg_rec / toFloat(eff) if eff > 0 else 0.0
        if avg_prec + avg_rec > 0.0:
            return 100.0 * (1.0 + factor) * avg_prec * avg_rec / (factor * avg_prec + avg_rec)
        else:
            return 0.0


def preprocess(chrf, sent):
    """Lowercase (optionally) for chrF."""
    return toLowerCase(sent) if chrf.lowercase else sent


def order(chrf):
    """The number of n-gram orders: char orders plus word orders."""
    return chrf.char_order + chrf.word_order


class BestState:
    """The best reference's chrF score and match statistics so far."""
    score: float
    stats: list[int]


def pick_best(state, ref_ngs, all_hyp, chrf):
    """Keeps the reference with the higher chrF score."""
    stats = ref_stats(all_hyp, ref_ngs)
    score = chr_f_score(stats, chrf.beta, order(chrf), chrf.eps_smoothing)
    return BestState(score=score, stats=stats) if score > state.score else state


def ref_ngrams_list(refs, chrf, i, acc):
    """The per-reference n-gram counter lists of `refs`, accumulated."""
    if i >= len(refs):
        return acc
    else:
        return ref_ngrams_list(refs, chrf, i + 1, listAppend(acc, ngrams_list(preprocess(chrf, unwrap_or(listGet(refs, i), "")), chrf)))


def best_state_from(ref_ngrams, all_hyp, chrf, i, best):
    """The best-reference state over the reference n-gram lists."""
    if i >= len(ref_ngrams):
        return best
    else:
        return best_state_from(ref_ngrams, all_hyp, chrf, i + 1, pick_best(best, unwrap_or(listGet(ref_ngrams, i), []), all_hyp, chrf))


def segment_stats(chrf, hyp, refs):
    """The best-reference match statistics of one hypothesis."""
    all_hyp = ngrams_list(preprocess(chrf, hyp), chrf)
    ref_ngrams = ref_ngrams_list(refs, chrf, 0, [])
    return best_state_from(ref_ngrams, all_hyp, chrf, 0, BestState(score=-1.0, stats=[])).stats


def add_int_lists(xs, ys):
    """Elementwise sum of two int lists."""
    return toList(map(range(len(xs)), lambda i: unwrap_or(listGet(xs, i), 0) + unwrap_or(listGet(ys, i), 0)))


def zeros_int(n):
    """A list of `n` zeros."""
    return toList(map(range(n), lambda _: 0))


def score_sentence(chrf, hypothesis, references):
    """chrF for one hypothesis against one or more references."""
    stats = segment_stats(chrf, hypothesis, references)
    return CHRFScore(score=chr_f_score(stats, chrf.beta, order(chrf), chrf.eps_smoothing),
                     char_order=chrf.char_order, word_order=chrf.word_order, beta=chrf.beta)


def transpose_row(docs, i):
    """Reference segment `i` of every document."""
    return toList(map(range(len(docs)), lambda d: unwrap_or(listGet(unwrap_or(listGet(docs, d), []), i), "")))


def transpose(docs):
    """Documents (each a list of segments) as per-segment reference lists."""
    n = foldList(docs, 0, lambda m, doc: len(doc) if len(doc) > m else m)
    return toList(map(range(n), lambda i: transpose_row(docs, i)))


def score_corpus(chrf, hypotheses, references):
    """chrF over a corpus; `references` is a list of reference documents."""
    seg_refs = transpose(references)
    stats = fold(range(len(hypotheses)), zeros_int(3 * order(chrf)), lambda acc, i: add_int_lists(
        acc, segment_stats(chrf, unwrap_or(listGet(hypotheses, i), ""), unwrap_or(listGet(seg_refs, i), []))))
    return CHRFScore(score=chr_f_score(stats, chrf.beta, order(chrf), chrf.eps_smoothing),
                     char_order=chrf.char_order, word_order=chrf.word_order, beta=chrf.beta)


def sentence_chrf(hypothesis, references, char_order, word_order, beta, remove_whitespace, eps_smoothing):
    """`sacrebleu.sentence_chrf`: sentence-level chrF (compat entry point)."""
    metric = CHRF(char_order=char_order, word_order=word_order, beta=beta, lowercase=False,
                  whitespace=not remove_whitespace, eps_smoothing=eps_smoothing)
    return score_sentence(metric, hypothesis, references)


def corpus_chrf(hypotheses, references, char_order, word_order, beta, remove_whitespace, eps_smoothing):
    """`sacrebleu.corpus_chrf`: corpus-level chrF (compat entry point)."""
    metric = CHRF(char_order=char_order, word_order=word_order, beta=beta, lowercase=False,
                  whitespace=not remove_whitespace, eps_smoothing=eps_smoothing)
    return score_corpus(metric, hypotheses, references)
