# osprey: flavor=python
"""@osprey/rouge_score's suite: RougeScorer, tokenizer and Porter stemmer against
values the real rouge_score 0.1.2 + nltk 3.10.4 computed
(src/osprey/packages/rouge_score/oracle/rouge_score.cpython)."""
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


def field_at(p_tables, r_tables, f_tables, k):
    """Rouge type `k`'s precision, recall and fmeasure tables, named for its tag."""
    return Field(p=unwrap_or(listGet(p_tables, k), []), r=unwrap_or(listGet(r_tables, k), []), f=unwrap_or(listGet(f_tables, k), []), name=unwrap_or(listGet(ROUGE_TYPES, k), ""))


def field_of(k):
    return field_at([Expected.rouge1_p, Expected.rouge2_p, Expected.rougeL_p, Expected.rougeLsum_p],
                    [Expected.rouge1_r, Expected.rouge2_r, Expected.rougeL_r, Expected.rougeLsum_r],
                    [Expected.rouge1_f, Expected.rouge2_f, Expected.rougeL_f, Expected.rougeLsum_f], k)


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


def rouge_fields(i, field, scores):
    """Every rouge type's precision, recall and fmeasure for one case."""
    return fold(range(4), [], lambda acc, k: listConcat(acc, type_checks(i, field(k), unwrap_or(listGet(scores, k), zero_scored()))))


def rouge_case(i):
    return rouge_fields(i, field_of, rouge_scores(i))


def rouge_cases():
    return fold(range(len(Expected.ROUGE_TARGET)), [], lambda acc, i: listConcat(acc, rouge_case(i)))


def stem_cases():
    n = len(Expected.STEM_WORDS)
    return toList(map(range(n), lambda i: Check.equal(
        f"stem[{i}]", Stemmer.stem(str_at(Expected.STEM_WORDS, i)), str_at(Expected.STEM_OUT, i))))


def token_checks(label, stemmed, want, detail):
    """Every text tokenized, `stemmed` selecting the stemmed form, against its oracle lists."""
    return toList(map(range(len(Expected.TOKENIZE_TEXT)), lambda i: Check.truth(
        f"tokenize.{label}[{i}]", structuralEquals(Tokenize.tokenize(str_at(Expected.TOKENIZE_TEXT, i), stemmed), strlist_at(want, i)), detail)))


def tokenize_cases():
    return listConcat(token_checks("plain", False, Expected.TOKENIZE_PLAIN, "plain tokens"),
                      token_checks("stem", True, Expected.TOKENIZE_STEM, "stemmed tokens"))


def multi_field(k):
    return field_at([Expected.MULTI_rouge1_p, Expected.MULTI_rouge2_p, Expected.MULTI_rougeL_p, Expected.MULTI_rougeLsum_p],
                    [Expected.MULTI_rouge1_r, Expected.MULTI_rouge2_r, Expected.MULTI_rougeL_r, Expected.MULTI_rougeLsum_r],
                    [Expected.MULTI_rouge1_f, Expected.MULTI_rouge2_f, Expected.MULTI_rougeL_f, Expected.MULTI_rougeLsum_f], k)


def multi_case(i):
    scorer = RougeScorer.RougeScorer(rouge_types=ROUGE_TYPES, use_stemmer=False, split_summaries=False)
    return rouge_fields(i, multi_field, RougeScorer.score_multi(scorer, strlist_at(Expected.MULTI_TARGETS, i), str_at(Expected.MULTI_PRED, i)))


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
    return Check.run(all_cases)
