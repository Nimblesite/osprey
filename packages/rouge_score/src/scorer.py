# osprey: flavor=python
# osprey: namespace=rouge_score
# osprey: module=RougeScorer
"""ROUGE scoring (rouge1, rouge2, rougeL, rougeLsum) as rouge_score computes it.

`RougeScorer` holds the configuration; `score` reproduces `RougeScorer.score`
(one `Scored` per requested type, in `rouge_types` order) and `score_multi`
reproduces `RougeScorer.score_multi` (the maximum-f-measure target per type).
rougeLsum splits sentences on newlines; `split_summaries=True` (nltk's sentence
tokenizer) is not ported. Only the four standard types are supported.
"""

from sacrebleu.Ngrams import word_ngrams_from_tokens, counter_total, counter_matched, counter_get, counter_increment
from rouge_score.Tokenize import tokenize

__all__ = ["RougeScorer", "Score", "Scored", "fmeasure", "score", "score_multi"]


class RougeScorer:
    """ROUGE configuration: the types to compute and stemmer/newline-split flags."""
    rouge_types: list[str]
    use_stemmer: bool
    split_summaries: bool


class Score:
    """A precision/recall/f-measure triple."""
    precision: float
    recall: float
    fmeasure: float


class Scored:
    """A `Score` labelled with its rouge type."""
    name: str
    precision: float
    recall: float
    fmeasure: float


class SumState:
    """The running hits and token counts of summary-level LCS."""
    hits: int
    cnt_r: dict[str, int]
    cnt_c: dict[str, int]


def fmeasure(precision, recall):
    """The harmonic mean of precision and recall."""
    return 2.0 * precision * recall / (precision + recall) if precision + recall > 0.0 else 0.0


def max_i(a, b):
    """The larger of two ints."""
    return a if a > b else b


def token_at(xs, i):
    """Element `i` of a token list, or the empty string."""
    return unwrap_or(listGet(xs, i), "")


def row_at(row, j):
    """Cell `j` of an LCS table row."""
    return unwrap_or(listGet(row, j), 0)


def cell(table, i, j):
    """Cell `(i, j)` of an LCS table."""
    return unwrap_or(listGet(unwrap_or(listGet(table, i), []), j), 0)


def zero_row(n):
    """A row of `n + 1` zeros."""
    return toList(map(range(n + 1), lambda _: 0))


def lcs_cell(ref, can, prev_row, cur_row, i, j):
    """One LCS table cell."""
    if token_at(ref, i - 1) == token_at(can, j - 1):
        return row_at(prev_row, j - 1) + 1
    else:
        return max_i(row_at(prev_row, j), row_at(cur_row, j - 1))


def lcs_row(ref, can, prev_row, i, cols):
    """The `i`-th row of the LCS table."""
    return fold(range(1, cols + 1), [0], lambda cur, j: listAppend(cur, lcs_cell(ref, can, prev_row, cur, i, j)))


def lcs_table(ref, can):
    """The full LCS dynamic-programming table."""
    cols = len(can)
    return fold(range(1, len(ref) + 1), [zero_row(cols)],
                lambda table, i: listAppend(table, lcs_row(ref, can, unwrap_or(listGet(table, i - 1), []), i, cols)))


def lcs_length(ref, can):
    """The longest-common-subsequence length of two token lists."""
    return cell(lcs_table(ref, can), len(ref), len(can))


def backtrack(t, ref, can, i, j, acc):
    """Reads one LCS out of the table as reference indices."""
    if i > 0 and j > 0:
        if token_at(ref, i - 1) == token_at(can, j - 1):
            return backtrack(t, ref, can, i - 1, j - 1, listPrepend(acc, i - 1))
        elif cell(t, i, j - 1) > cell(t, i - 1, j):
            return backtrack(t, ref, can, i, j - 1, acc)
        else:
            return backtrack(t, ref, can, i - 1, j, acc)
    else:
        return acc


def lcs_ind(ref, can):
    """The reference indices of one longest common subsequence."""
    return backtrack(lcs_table(ref, can), ref, can, len(ref), len(can), [])


def unique_sorted_from(xs, i, prev, acc):
    """Deduplicates an ascending list."""
    if i >= len(xs):
        return acc
    else:
        x = unwrap_or(listGet(xs, i), 0)
        if x == prev:
            return unique_sorted_from(xs, i + 1, prev, acc)
        else:
            return unique_sorted_from(xs, i + 1, x, listAppend(acc, x))


def find_union(lcs_list):
    """The sorted union of several LCS index lists."""
    merged = foldList(lcs_list, [], lambda acc, xs: listConcat(acc, xs))
    return unique_sorted_from(listSort(merged), 0, -1, [])


def union_lcs(ref, c_list):
    """The union-LCS tokens of `ref` against the candidate sentences `c_list`."""
    lcs_list = mapList(c_list, lambda c: lcs_ind(ref, c))
    return mapList(find_union(lcs_list), lambda i: token_at(ref, i))


def score_lcs(target_tokens, prediction_tokens):
    """rougeL: longest-common-subsequence precision/recall/f-measure."""
    if isEmpty(target_tokens) or isEmpty(prediction_tokens):
        return Score(precision=0.0, recall=0.0, fmeasure=0.0)
    else:
        l = lcs_length(target_tokens, prediction_tokens)
        precision = toFloat(l) / toFloat(len(prediction_tokens))
        recall = toFloat(l) / toFloat(len(target_tokens))
        return Score(precision=precision, recall=recall, fmeasure=fmeasure(precision, recall))


def score_ngrams(target_ngrams, prediction_ngrams):
    """rougeN: n-gram overlap precision/recall/f-measure."""
    intersection = counter_matched(target_ngrams, prediction_ngrams)
    precision = toFloat(intersection) / toFloat(max_i(counter_total(prediction_ngrams), 1))
    recall = toFloat(intersection) / toFloat(max_i(counter_total(target_ngrams), 1))
    return Score(precision=precision, recall=recall, fmeasure=fmeasure(precision, recall))


def tokens_counter(sentences):
    """A counter over every token of every sentence."""
    return foldList(sentences, {}, lambda acc, s: foldList(s, acc, lambda a2, t: counter_increment(a2, t)))


def total_tokens(sentences):
    """The total token count over all sentences."""
    return foldList(sentences, 0, lambda acc, s: acc + len(s))


def hit_if_available(state, t):
    """Counts one union-LCS hit and decrements both token counters."""
    if counter_get(state.cnt_c, t) > 0 and counter_get(state.cnt_r, t) > 0:
        return SumState(hits=state.hits + 1,
                        cnt_r=mapSet(state.cnt_r, t, counter_get(state.cnt_r, t) - 1),
                        cnt_c=mapSet(state.cnt_c, t, counter_get(state.cnt_c, t) - 1))
    else:
        return state


def count_sentence_hits(state, r, can_sent):
    """The union-LCS hits of one reference sentence."""
    return foldList(union_lcs(r, can_sent), state, hit_if_available)


def summary_level_lcs(ref_sent, can_sent):
    """rougeLsum: summary-level LCS with double-counting prevention."""
    if isEmpty(ref_sent) or isEmpty(can_sent):
        return Score(precision=0.0, recall=0.0, fmeasure=0.0)
    else:
        m = total_tokens(ref_sent)
        n = total_tokens(can_sent)
        if m == 0 or n == 0:
            return Score(precision=0.0, recall=0.0, fmeasure=0.0)
        else:
            seed = SumState(hits=0, cnt_r=tokens_counter(ref_sent), cnt_c=tokens_counter(can_sent))
            state = foldList(ref_sent, seed, lambda acc, r: count_sentence_hits(acc, r, can_sent))
            precision = toFloat(state.hits) / toFloat(n)
            recall = toFloat(state.hits) / toFloat(m)
            return Score(precision=precision, recall=recall, fmeasure=fmeasure(precision, recall))


def split_sentences(text):
    """Newline sentence split (the `split_summaries=False` path), empties dropped."""
    return filterList(unwrap_or(split(text, "\n"), [text]), lambda x: not isEmpty(x))


def sentence_tokens(text, use_stemmer):
    """The token lists of each newline-separated sentence of `text`."""
    return mapList(split_sentences(text), lambda s: tokenize(s, use_stemmer))


def scored(name, s):
    """A `Score` labelled with its rouge type."""
    return Scored(name=name, precision=s.precision, recall=s.recall, fmeasure=s.fmeasure)


def one_rouge(t, tokens_t, tokens_p, target, prediction, use_stemmer):
    """One rouge type's score for a pair of texts."""
    if t == "rougeL":
        return scored(t, score_lcs(tokens_t, tokens_p))
    elif t == "rougeLsum":
        return scored(t, summary_level_lcs(sentence_tokens(target, use_stemmer), sentence_tokens(prediction, use_stemmer)))
    elif t == "rouge1":
        return scored(t, score_ngrams(word_ngrams_from_tokens(tokens_t, 1), word_ngrams_from_tokens(tokens_p, 1)))
    else:
        return scored(t, score_ngrams(word_ngrams_from_tokens(tokens_t, 2), word_ngrams_from_tokens(tokens_p, 2)))


def score(scorer, target, prediction):
    """`RougeScorer.score`: one `Scored` per rouge type, in `rouge_types` order."""
    if len(scorer.rouge_types) == 1 and unwrap_or(listGet(scorer.rouge_types, 0), "") == "rougeLsum":
        return [one_rouge("rougeLsum", [], [], target, prediction, scorer.use_stemmer)]
    else:
        tokens_t = tokenize(target, scorer.use_stemmer)
        tokens_p = tokenize(prediction, scorer.use_stemmer)
        return mapList(scorer.rouge_types, lambda t: one_rouge(t, tokens_t, tokens_p, target, prediction, scorer.use_stemmer))


def pick_at(k, best, per_target):
    """Keeps the higher-f-measure score at index `k` across targets."""
    s = unwrap_or(listGet(per_target, k), Scored(name="", precision=0.0, recall=0.0, fmeasure=-1.0))
    return s if s.fmeasure > best.fmeasure else best


def best_by_index(k, score_dicts):
    """The highest-f-measure `Scored` at index `k` across all targets."""
    seed = Scored(name="", precision=0.0, recall=0.0, fmeasure=-1.0)
    return foldList(score_dicts, seed, lambda best, per_target: pick_at(k, best, per_target))


def score_multi(scorer, targets, prediction):
    """`RougeScorer.score_multi`: the maximum-f-measure target per type."""
    score_dicts = mapList(targets, lambda t: score(scorer, t, prediction))
    return toList(map(range(len(scorer.rouge_types)), lambda k: best_by_index(k, score_dicts)))
