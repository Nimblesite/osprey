# osprey: flavor=python
"""@osprey/rouge_score's suite: RougeScorer, tokenizer and Porter stemmer against
values the real rouge_score 0.1.2 + nltk 3.10.4 computed
(scripts/oracles/rouge_score.py)."""
from testing import Check
from rouge_score_expected import Expected
from rouge_score import RougeScorer, Stemmer, Tokenize

DIV = 1e-12

ROUGE_TYPES = ["rouge1", "rouge2", "rougeL", "rougeLsum"]


class Field:
    p: list[float]
    r: list[float]
    f: list[float]
    name: str


def str_at(xs, i):
    return unwrap_or(listGet(xs, i), "")


def bool_at(xs, i):
    return unwrap_or(listGet(xs, i), False)


def float_at(xs, i):
    return unwrap_or(listGet(xs, i), 0.0)


def strlist_at(xs, i):
    return unwrap_or(listGet(xs, i), [])


def field_of(k):
    if k == 0:
        return Field(p=Expected.rouge1_p, r=Expected.rouge1_r, f=Expected.rouge1_f, name="rouge1")
    elif k == 1:
        return Field(p=Expected.rouge2_p, r=Expected.rouge2_r, f=Expected.rouge2_f, name="rouge2")
    elif k == 2:
        return Field(p=Expected.rougeL_p, r=Expected.rougeL_r, f=Expected.rougeL_f, name="rougeL")
    else:
        return Field(p=Expected.rougeLsum_p, r=Expected.rougeLsum_r, f=Expected.rougeLsum_f, name="rougeLsum")


def zero_scored():
    return RougeScorer.Scored(name="", precision=0.0, recall=0.0, fmeasure=0.0)


def type_checks(i, field, s):
    return [
        Check.close(f"{field.name}.p[{i}]", s.precision, float_at(field.p, i), DIV, DIV),
        Check.close(f"{field.name}.r[{i}]", s.recall, float_at(field.r, i), DIV, DIV),
        Check.close(f"{field.name}.f[{i}]", s.fmeasure, float_at(field.f, i), DIV, DIV),
    ]


def rouge_scores(i):
    scorer = RougeScorer.RougeScorer(rouge_types=ROUGE_TYPES,
                                     use_stemmer=bool_at(Expected.ROUGE_STEMMER, i),
                                     split_summaries=bool_at(Expected.ROUGE_SPLIT, i))
    return RougeScorer.score(scorer, str_at(Expected.ROUGE_TARGET, i), str_at(Expected.ROUGE_PRED, i))


def rouge_case(i):
    scores = rouge_scores(i)
    return fold(range(4), [], lambda acc, k: listConcat(acc, type_checks(
        i, field_of(k), unwrap_or(listGet(scores, k), zero_scored()))))


def rouge_cases():
    return fold(range(len(Expected.ROUGE_TARGET)), [], lambda acc, i: listConcat(acc, rouge_case(i)))


def stem_cases():
    n = len(Expected.STEM_WORDS)
    return toList(map(range(n), lambda i: Check.equal(
        f"stem[{i}]", Stemmer.stem(str_at(Expected.STEM_WORDS, i)), str_at(Expected.STEM_OUT, i))))


def tokenize_cases():
    n = len(Expected.TOKENIZE_TEXT)
    plain = toList(map(range(n), lambda i: Check.truth(
        f"tokenize.plain[{i}]", structuralEquals(Tokenize.tokenize(str_at(Expected.TOKENIZE_TEXT, i), False), strlist_at(Expected.TOKENIZE_PLAIN, i)), "plain tokens")))
    stem = toList(map(range(n), lambda i: Check.truth(
        f"tokenize.stem[{i}]", structuralEquals(Tokenize.tokenize(str_at(Expected.TOKENIZE_TEXT, i), True), strlist_at(Expected.TOKENIZE_STEM, i)), "stemmed tokens")))
    return listConcat(plain, stem)


def multi_field(k):
    if k == 0:
        return Field(p=Expected.MULTI_rouge1_p, r=Expected.MULTI_rouge1_r, f=Expected.MULTI_rouge1_f, name="rouge1")
    elif k == 1:
        return Field(p=Expected.MULTI_rouge2_p, r=Expected.MULTI_rouge2_r, f=Expected.MULTI_rouge2_f, name="rouge2")
    elif k == 2:
        return Field(p=Expected.MULTI_rougeL_p, r=Expected.MULTI_rougeL_r, f=Expected.MULTI_rougeL_f, name="rougeL")
    else:
        return Field(p=Expected.MULTI_rougeLsum_p, r=Expected.MULTI_rougeLsum_r, f=Expected.MULTI_rougeLsum_f, name="rougeLsum")


def multi_case(i):
    scorer = RougeScorer.RougeScorer(rouge_types=ROUGE_TYPES, use_stemmer=False, split_summaries=False)
    scores = RougeScorer.score_multi(scorer, strlist_at(Expected.MULTI_TARGETS, i), str_at(Expected.MULTI_PRED, i))
    return fold(range(4), [], lambda acc, k: listConcat(acc, type_checks(
        i, multi_field(k), unwrap_or(listGet(scores, k), zero_scored()))))


def multi_cases():
    return fold(range(len(Expected.MULTI_PRED)), [], lambda acc, i: listConcat(acc, multi_case(i)))


def fmeasure_cases():
    return [
        Check.close("fmeasure.normal", RougeScorer.fmeasure(0.5, 1.0), 2.0 * 0.5 * 1.0 / 1.5, DIV, DIV),
        Check.close("fmeasure.zero", RougeScorer.fmeasure(0.0, 0.0), 0.0, DIV, DIV),
    ]


def record_cases():
    s = RougeScorer.Score(precision=0.5, recall=1.0, fmeasure=2.0 / 3.0)
    return [
        Check.close("rouge.Score", s.precision, 0.5, DIV, DIV),
        Check.equal("rouge.Scored.name", RougeScorer.Scored(name="rouge1", precision=1.0, recall=1.0, fmeasure=1.0).name, "rouge1"),
    ]


def all_cases():
    return listConcat(listConcat(listConcat(listConcat(listConcat(
        rouge_cases(), stem_cases()), tokenize_cases()), multi_cases()), fmeasure_cases()), record_cases())


def main():
    match Check.strictly(lambda: Check.report(all_cases())):
        case Success(value=value):
            return value
        case Error(message=message):
            return print(f"suite stopped: {message}")
