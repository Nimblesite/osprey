# osprey: flavor=python
"""@osprey/sacrebleu's suite: tokenizers, BLEU and chrF against values the real
sacrebleu 2.6.0 computed (scripts/oracles/sacrebleu.py)."""
from testing import Check
from sacrebleu_expected import Expected
from sacrebleu import Bleu, Chrf, Ngrams, Text, Tokenizer

EXP = 1e-9
DIV = 1e-12


class BleuCfg:
    smooth: str
    sv: float
    max_order: int
    eff: bool
    lc: bool
    tok: str


class ChrfCfg:
    co: int
    wo: int
    beta: int
    rw: bool
    eps: bool
    lc: bool


def str_at(xs, i):
    return unwrap_or(listGet(xs, i), "")


def int_at(xs, i):
    return unwrap_or(listGet(xs, i), 0)


def float_at(xs, i):
    return unwrap_or(listGet(xs, i), 0.0)


def bool_at(xs, i):
    return unwrap_or(listGet(xs, i), False)


def strlist_at(xs, i):
    return unwrap_or(listGet(xs, i), [])


def floatlist_at(xs, i):
    return unwrap_or(listGet(xs, i), [])


def text_cases():
    return [
        Check.ints_equal("text.codepoints", Text.codepoints("aé"), [97, 233]),
        Check.equal("text.from_codepoints", Text.from_codepoints([97, 233]), "aé"),
        Check.equal("text.map", Text.map_codepoints("a1!", lambda cp: 120 if cp == 97 else cp), "x1!"),
        Check.truth("text.digit", Text.is_ascii_digit(57) and not Text.is_ascii_digit(58), "digit"),
        Check.truth("text.alnum", Text.is_ascii_lower_alnum(122) and not Text.is_ascii_lower_alnum(91), "alnum"),
        Check.equal("text.collapse", Text.collapse_whitespace("  a\t b\n "), "a b"),
    ]


def ngram_cases():
    tok = ["the", "cat", "the", "cat"]
    return [
        Check.truth("ngrams.word", structuralEquals(Ngrams.word_ngrams("the cat the", 1, 2), {"the cat": 1, "cat the": 1, "the": 2, "cat": 1}), "word ngrams"),
        Check.truth("ngrams.order2", structuralEquals(Ngrams.word_ngrams_from_tokens(tok, 2), {"the cat": 2, "cat the": 1}), "order-2 ngrams"),
        Check.truth("ngrams.char", structuralEquals(Ngrams.char_ngrams("abc", 2, False), {"ab": 1, "bc": 1}), "char ngrams"),
        Check.equal("ngrams.total", Ngrams.counter_total(Ngrams.word_ngrams_from_tokens(tok, 1)), 4),
        Check.equal("ngrams.matched", Ngrams.counter_matched(Ngrams.word_ngrams("cat", 1, 1), Ngrams.word_ngrams("cat dog", 1, 1)), 1),
        Check.truth("ngrams.merge", structuralEquals(Ngrams.counter_merge_max({"a": 2}, {"a": 3, "b": 1}), {"a": 3, "b": 1}), "merge max"),
        Check.equal("ngrams.get", Ngrams.counter_get({"x": 5}, "x"), 5),
        Check.truth("ngrams.increment", structuralEquals(Ngrams.counter_increment({"x": 1}, "x"), {"x": 2}), "increment"),
    ]


def tokenizer_cases():
    n = len(Expected.TOK_CASES)
    plain = toList(map(range(n), lambda i: Check.equal(f"tok.none[{i}]", Tokenizer.tokenize_none(str_at(Expected.TOK_CASES, i)), str_at(Expected.TOK_NONE, i))))
    char = toList(map(range(n), lambda i: Check.equal(f"tok.char[{i}]", Tokenizer.tokenize_char(str_at(Expected.TOK_CASES, i)), str_at(Expected.TOK_CHAR, i))))
    v13a = toList(map(range(n), lambda i: Check.equal(f"tok.13a[{i}]", Tokenizer.tokenize_13a(str_at(Expected.TOK_CASES, i)), str_at(Expected.TOK_13A, i))))
    dispatch = toList(map(range(n), lambda i: Check.equal(f"tok.dispatch[{i}]", Tokenizer.tokenize(str_at(Expected.TOK_CASES, i), "13a"), str_at(Expected.TOK_13A, i))))
    return listConcat(listConcat(plain, char), listConcat(v13a, dispatch))


def bleu_config(i):
    return BleuCfg(smooth=str_at(Expected.BLEU_SMOOTH, i), sv=float_at(Expected.BLEU_SMOOTH_VALUE, i),
                   max_order=int_at(Expected.BLEU_MAX_ORDER, i), eff=bool_at(Expected.BLEU_EFF, i),
                   lc=bool_at(Expected.BLEU_LOWERCASE, i), tok=str_at(Expected.BLEU_TOKENIZE, i))


def bleu_score_of(i):
    cfg = bleu_config(i)
    hyp = str_at(Expected.BLEU_HYP, i)
    refs = strlist_at(Expected.BLEU_REFS, i)
    if cfg.max_order == 4:
        return Bleu.sentence_bleu(hyp, refs, cfg.smooth, cfg.sv, cfg.lc, cfg.tok, cfg.eff)
    else:
        metric = Bleu.BLEU(lowercase=cfg.lc, force=False, tokenize=cfg.tok, smooth_method=cfg.smooth,
                           smooth_value=cfg.sv, max_ngram_order=cfg.max_order, effective_order=cfg.eff)
        return Bleu.score_sentence(metric, hyp, refs)


def score_fields(prefix, i, sc, score, bp, sys_len, ref_len, prec, counts, totals):
    return [
        Check.close(f"{prefix}.score[{i}]", sc.score, score, EXP, EXP),
        Check.close(f"{prefix}.bp[{i}]", sc.bp, bp, EXP, EXP),
        Check.equal(f"{prefix}.sys[{i}]", sc.sys_len, sys_len),
        Check.equal(f"{prefix}.ref[{i}]", sc.ref_len, ref_len),
        Check.close_all(f"{prefix}.prec[{i}]", sc.precisions, prec, DIV, DIV),
        Check.close_all(f"{prefix}.counts[{i}]", sc.counts, counts, DIV, DIV),
        Check.close_all(f"{prefix}.totals[{i}]", sc.totals, totals, DIV, DIV),
    ]


def bleu_checks(i, sc):
    return score_fields("bleu", i, sc, float_at(Expected.BLEU_SCORE, i), float_at(Expected.BLEU_BP, i),
                        int_at(Expected.BLEU_SYS_LEN, i), int_at(Expected.BLEU_REF_LEN, i),
                        floatlist_at(Expected.BLEU_PREC, i), floatlist_at(Expected.BLEU_COUNTS, i),
                        floatlist_at(Expected.BLEU_TOTALS, i))


def corpus_checks(i, sc):
    return score_fields("corpus", i, sc, float_at(Expected.CORPUS_SCORE, i), float_at(Expected.CORPUS_BP, i),
                        int_at(Expected.CORPUS_SYS_LEN, i), int_at(Expected.CORPUS_REF_LEN, i),
                        floatlist_at(Expected.CORPUS_PREC, i), floatlist_at(Expected.CORPUS_COUNTS, i),
                        floatlist_at(Expected.CORPUS_TOTALS, i))


def bleu_cases():
    return fold(range(len(Expected.BLEU_SCORE)), [], lambda acc, i: listConcat(acc, bleu_checks(i, bleu_score_of(i))))


def corpus_bleu_of(i):
    hyps = strlist_at(Expected.CORPUS_HYP, i)
    refs = strlist_at(Expected.CORPUS_REFS, i)
    return Bleu.corpus_bleu(hyps, refs, str_at(Expected.CORPUS_SMOOTH, i), float_at(Expected.CORPUS_SMOOTH_VALUE, i),
                            False, bool_at(Expected.CORPUS_LOWERCASE, i), str_at(Expected.CORPUS_TOKENIZE, i),
                            bool_at(Expected.CORPUS_EFF, i))


def corpus_case(i):
    return corpus_checks(i, corpus_bleu_of(i))


def corpus_cases():
    return fold(range(len(Expected.CORPUS_SCORE)), [], lambda acc, i: listConcat(acc, corpus_case(i)))


def raw_cases():
    return [
        Check.close("raw.score", Bleu.raw_corpus_bleu(["the cat"], [["the cat"]], 0.1).score, 100.0, EXP, EXP),
        Check.close("raw.score2", Bleu.raw_corpus_bleu(["a"], [["the cat"]], 0.1).score, 0.0, EXP, EXP),
    ]


def compute_bleu_cases():
    return fold(range(len(Expected.CBL_SCORE)), [], lambda acc, i: listConcat(acc, [
        Check.close(f"compute.score[{i}]", Bleu.compute_bleu(floatlist_at(Expected.CBL_CORRECT, i), floatlist_at(Expected.CBL_TOTAL, i), 2, 2, "none", -1.0, True, 4).score, float_at(Expected.CBL_SCORE, i), EXP, EXP),
        Check.close_all(f"compute.prec[{i}]", Bleu.compute_bleu(floatlist_at(Expected.CBL_CORRECT, i), floatlist_at(Expected.CBL_TOTAL, i), 2, 2, "none", -1.0, True, 4).precisions, floatlist_at(Expected.CBL_PREC, i), DIV, DIV),
        Check.close(f"compute.bp[{i}]", Bleu.compute_bleu(floatlist_at(Expected.CBL_CORRECT, i), floatlist_at(Expected.CBL_TOTAL, i), 2, 2, "none", -1.0, True, 4).bp, float_at(Expected.CBL_BP, i), EXP, EXP),
    ]))


def chrf_config(i):
    return ChrfCfg(co=int_at(Expected.CHRF_CHAR_ORDER, i), wo=int_at(Expected.CHRF_WORD_ORDER, i),
                   beta=int_at(Expected.CHRF_BETA, i), rw=bool_at(Expected.CHRF_REMOVE_WS, i),
                   eps=bool_at(Expected.CHRF_EPS, i), lc=bool_at(Expected.CHRF_LOWERCASE, i))


def chrf_of(i):
    cfg = chrf_config(i)
    hyp = str_at(Expected.CHRF_HYP, i)
    refs = strlist_at(Expected.CHRF_REFS, i)
    if cfg.lc:
        metric = Chrf.CHRF(char_order=cfg.co, word_order=cfg.wo, beta=cfg.beta, lowercase=True,
                           whitespace=not cfg.rw, eps_smoothing=cfg.eps)
        return Chrf.score_sentence(metric, hyp, refs)
    else:
        return Chrf.sentence_chrf(hyp, refs, cfg.co, cfg.wo, cfg.beta, cfg.rw, cfg.eps)


def chrf_cases():
    return toList(map(range(len(Expected.CHRF_SCORE)), lambda i: Check.close(
        f"chrf.score[{i}]", chrf_of(i).score, float_at(Expected.CHRF_SCORE, i), DIV, DIV)))


def chrf_corpus_of(i):
    hyps = strlist_at(Expected.CHRF_CORPUS_HYP, i)
    refs = strlist_at(Expected.CHRF_CORPUS_REFS, i)
    return Chrf.corpus_chrf(hyps, refs, int_at(Expected.CHRF_CORPUS_CHAR_ORDER, i),
                            int_at(Expected.CHRF_CORPUS_WORD_ORDER, i), int_at(Expected.CHRF_CORPUS_BETA, i),
                            bool_at(Expected.CHRF_CORPUS_REMOVE_WS, i), bool_at(Expected.CHRF_CORPUS_EPS, i))


def chrf_corpus_cases():
    return toList(map(range(len(Expected.CHRF_CORPUS_SCORE)), lambda i: Check.close(
        f"chrf.corpus[{i}]", chrf_corpus_of(i).score, float_at(Expected.CHRF_CORPUS_SCORE, i), DIV, DIV)))


def score_corpus_cases():
    bmetric = Bleu.BLEU(lowercase=False, force=False, tokenize="13a", smooth_method="exp",
                        smooth_value=-1.0, max_ngram_order=4, effective_order=False)
    bleu_sc = Bleu.score_corpus(bmetric, ["the cat sat", "a dog ran", "hello world"],
                                [["the cat sat", "a dog ran", "hello world"]])
    cmetric = Chrf.CHRF(char_order=6, word_order=0, beta=2, lowercase=False, whitespace=True, eps_smoothing=False)
    chrf_sc = Chrf.score_corpus(cmetric, ["the cat", "a dog", "hello"], [["the cat", "a dog", "hello"]])
    return [
        Check.close("bleu.score_corpus", bleu_sc.score, float_at(Expected.CORPUS_SCORE, 0), EXP, EXP),
        Check.close("chrf.score_corpus", chrf_sc.score, float_at(Expected.CHRF_CORPUS_SCORE, 0), DIV, DIV),
    ]


def record_cases():
    bs = Bleu.BLEUScore(score=1.0, counts=[1.0], totals=[2.0], precisions=[50.0], bp=1.0, sys_len=1, ref_len=1)
    cs = Chrf.CHRFScore(score=42.0, char_order=6, word_order=2, beta=2)
    return [
        Check.equal("bleu.BLEUScore", bs.sys_len, 1),
        Check.close("chrf.CHRFScore", cs.score, 42.0, DIV, DIV),
    ]


def all_cases():
    base = listConcat(listConcat(listConcat(listConcat(listConcat(listConcat(
        text_cases(), ngram_cases()), tokenizer_cases()), bleu_cases()), corpus_cases()),
        raw_cases()), compute_bleu_cases())
    return listConcat(base, listConcat(listConcat(listConcat(chrf_cases(), chrf_corpus_cases()),
        score_corpus_cases()), record_cases()))


def main():
    match Check.strictly(lambda: Check.report(all_cases())):
        case Success(value=value):
            return value
        case Error(message=message):
            return print(f"suite stopped: {message}")
