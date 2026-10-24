"""CPython oracle for @osprey/sacrebleu: BLEU, chrF and tokenizer values.

Writes packages/sacrebleu/tests/expected.py. Run with the oracle virtualenv:
`python scripts/oracles/sacrebleu.py` (no `-I`: the script dir must be on sys.path
so `from expected_module import write` resolves).
"""
import sys

from expected_module import write

# This script is named sacrebleu.py; drop its own directory from sys.path so
# `import sacrebleu` resolves to the real package, not to this file.
sys.path.pop(0)

import sacrebleu
from sacrebleu.metrics.bleu import _get_tokenizer
from sacrebleu.metrics import BLEU, CHRF
import logging
logging.getLogger("sacrebleu").setLevel(logging.ERROR)

TOK_13A = _get_tokenizer("13a")()
TOK_CHAR = _get_tokenizer("char")()
TOK_NONE = _get_tokenizer("none")()

# ---- Tokenizers -----------------------------------------------------------

TOK_CASES = [
    "", "hello world", "a.b", "3.5", "3,5", "abc.", ".abc", "3.", ".,", "a.,b",
    "3-x", "x-3", "a&.b", "it's", "don't", '"quoted"', "(hi)", "a/b", "50%",
    "x=y", "a@b.com", "café", "你好世界", "emoji 👍 test", "multi  space",
    "  leading trailing  ", "Tab\there", "a-b-c", "e.g. test", "no.1", "1.no",
    "<skipped>", "a-\nb", "&quot;x&quot; &amp; &lt; &gt;", ".5", "5,", "é.",
    ".é", "3.é", "é.3", "a,b,c", "..", ",,", ".-.", "--", "3-", "-3", "a..b",
    "a,,b", "π.5", ".5π", "100%", "a!b?c;d:", "[]{}()", "a_b c^d~e", "x|y",
    "state-of-the-art", "3D", "v1.2.3",
]

# ---- BLEU ----------------------------------------------------------------

HYP_REFS = [
    ("", [""]),
    ("", ["the cat"]),
    ("the cat", [""]),
    ("the cat sat on the mat", ["the cat sat on the mat"]),
    ("cat", ["cat"]),
    ("a", ["a"]),
    ("the", ["the"]),
    ("cat cat cat cat", ["cat"]),
    ("the cat the cat", ["the cat"]),
    ("dog dog dog", ["cat cat cat"]),
    ("hello", ["hello world"]),
    ("hello world", ["hello"]),
    ("one two three four five six", ["one two three four five six"]),
    ("The quick brown fox jumps over the lazy dog", ["The quick brown fox jumps over the lazy dog"]),
    ("The quick brown fox", ["The quick brown dog"]),
    ("It's a test.", ["It is a test."]),
    ('He said "hello" (loudly)!', ['He said "hello" (loudly)!']),
    ("3.5 million, 50%", ["3.5 million, 50%"]),
    ("café naïve", ["cafe naive"]),
    ("你好世界", ["你好世界"]),
    ("emoji 👍 test", ["emoji 👍 test"]),
    ("a b c d e f g h i j", ["a b c d e f g h i j", "a b c d e f g h i k"]),
    ("a b c", ["a b c d", "a b"]),
    ("x y", ["x y z", "x"]),
    ("word " * 60, ["word " * 60]),
    ("one two three", ["one two three", "one two four", "one two"]),
    ("syntactic ambiguity", ["syntactic ambiguity", "syntactic ambiguity"]),
    ("a", ["a", "a", "a"]),
]

# (smooth_method, smooth_value, max_ngram_order, effective_order, lowercase, tokenize)
BLEU_CONFIGS = [
    ("exp", None, 4, True, False, "13a"),
    ("exp", None, 4, False, False, "13a"),
    ("floor", None, 4, True, False, "13a"),
    ("floor", 0.1, 4, True, False, "13a"),
    ("add-k", None, 4, True, False, "13a"),
    ("add-k", 2.0, 4, True, False, "13a"),
    ("none", None, 4, True, False, "13a"),
    ("none", None, 4, False, False, "13a"),
    ("exp", None, 4, True, True, "13a"),
    ("exp", None, 4, True, False, "none"),
    ("exp", None, 4, True, False, "char"),
    ("exp", None, 2, True, False, "13a"),
    ("floor", None, 3, False, False, "13a"),
]


def bleu_rows(prefix, pairs, configs, score):
    """BLEU constants under `prefix` for every (hypothesis, references) pair x
    config, scored by `score(metric, hyp, refs)`; the inputs and every field of
    the returned BLEUScore become parallel lists."""
    rows = {name: [] for name in ("HYP", "REFS", "SMOOTH", "SMOOTH_VALUE", "MAX_ORDER", "EFF",
                                   "LOWERCASE", "TOKENIZE", "SCORE", "COUNTS", "TOTALS", "PREC",
                                   "BP", "SYS_LEN", "REF_LEN")}
    for hyp, refs in pairs:
        for smooth, sv, max_order, eff, lc, tok in configs:
            m = BLEU(lowercase=lc, force=False, tokenize=tok,
                     smooth_method=smooth, smooth_value=sv,
                     max_ngram_order=max_order, effective_order=eff)
            sc = score(m, hyp, refs)
            values = (hyp, refs, smooth, -1.0 if sv is None else sv, max_order, eff, lc, tok, sc.score,
                      [float(c) for c in sc.counts], [float(t) for t in sc.totals], sc.precisions,
                      sc.bp, sc.sys_len, sc.ref_len)
            for name, value in zip(rows, values):
                rows[name].append(value)
    return {f"{prefix}_{name}": values for name, values in rows.items()}


def bleu_sentence_rows():
    return bleu_rows("BLEU", HYP_REFS, BLEU_CONFIGS, lambda m, hyp, refs: m.sentence_score(hyp, refs))


# Corpus-level BLEU: several sentence pairs scored together.
CORPUS = [
    (["the cat sat", "a dog ran", "hello world"], [["the cat sat", "a dog ran", "hello world"]]),
    (["the cat", "dog"], [["the cat sat on the mat", "a big dog ran fast"]]),
    ([""], [["the cat"]]),
    (["the cat", "the dog", "the bird"], [["the cat", "the dog", "the bird"], ["a cat", "a dog", "a bird"]]),
    (["one", "two three", "four"], [["one", "two three", "four five"]]),
]
CORPUS_CONFIGS = [
    ("exp", None, 4, False, False, "13a"),
    ("none", None, 4, False, False, "13a"),
    ("add-k", None, 4, False, False, "13a"),
    ("floor", None, 4, False, False, "13a"),
]


def bleu_corpus_rows():
    rows = bleu_rows("CORPUS", CORPUS, CORPUS_CONFIGS, lambda m, hyps, refs: m.corpus_score(hyps, refs))
    del rows["CORPUS_MAX_ORDER"]
    return rows


# ---- chrF ----------------------------------------------------------------

CHRF_CASES = [
    ("", [""]),
    ("", ["hello"]),
    ("hello", [""]),
    ("hello world", ["hello world"]),
    ("hello", ["hello world"]),
    ("the cat sat", ["the cat sat"]),
    ("The quick brown fox", ["The quick brown dog"]),
    ("café", ["café"]),
    ("你好世界", ["你好世界"]),
    ("emoji 👍 test", ["emoji 👍 test"]),
    ("a b c d e", ["a b c d e"]),
    ("kitten", ["sitting"]),
    ("abc", ["abd"]),
    ("a" * 40, ["a" * 40]),
    ("It's a test.", ["It is a test."]),
    ("The quick brown fox jumps", ["The quick brown fox jumps", "A quick brown fox jumps"]),
]
# (char_order, word_order, beta, remove_whitespace, eps_smoothing, lowercase)
CHRF_CONFIGS = [
    (6, 0, 2, True, False, False),
    (6, 2, 2, True, False, False),
    (6, 0, 2, False, False, False),
    (6, 0, 2, True, True, False),
    (6, 2, 2, True, True, False),
    (3, 0, 1, True, False, False),
    (6, 0, 2, True, False, True),
    (6, 2, 2, True, False, True),
]


def chrf_rows():
    hyps, refss, cos, wos, betas, rws, epss, lcs = [], [], [], [], [], [], [], []
    scores = []
    for hyp, refs in CHRF_CASES:
        for co, wo, beta, rw, eps, lc in CHRF_CONFIGS:
            m = CHRF(char_order=co, word_order=wo, beta=beta, lowercase=lc,
                     whitespace=not rw, eps_smoothing=eps)
            sc = m.sentence_score(hyp, refs)
            hyps.append(hyp)
            refss.append(refs)
            cos.append(co)
            wos.append(wo)
            betas.append(beta)
            rws.append(rw)
            epss.append(eps)
            lcs.append(lc)
            scores.append(sc.score)
    return {
        "CHRF_HYP": hyps, "CHRF_REFS": refss, "CHRF_CHAR_ORDER": cos,
        "CHRF_WORD_ORDER": wos, "CHRF_BETA": betas, "CHRF_REMOVE_WS": rws,
        "CHRF_EPS": epss, "CHRF_LOWERCASE": lcs, "CHRF_SCORE": scores,
    }


# chrF corpus: one representative corpus case per config of interest.
CHRF_CORPUS = [
    (["the cat", "a dog", "hello"], [["the cat", "a dog", "hello"]]),
    (["kitten", "sitting"], [["kitten", "sitting"]]),
    ([""], [["hello"]]),
]
CHRF_CORPUS_CONFIGS = [
    (6, 0, 2, True, False, False),
    (6, 2, 2, True, False, False),
]


def chrf_corpus_rows():
    hyps, refss, cos, wos, betas, rws, epss, lcs = [], [], [], [], [], [], [], []
    scores = []
    for hyps_, refs_ in CHRF_CORPUS:
        for co, wo, beta, rw, eps, lc in CHRF_CORPUS_CONFIGS:
            m = CHRF(char_order=co, word_order=wo, beta=beta, lowercase=lc,
                     whitespace=not rw, eps_smoothing=eps)
            sc = m.corpus_score(hyps_, refs_)
            hyps.append(hyps_)
            refss.append(refs_)
            cos.append(co)
            wos.append(wo)
            betas.append(beta)
            rws.append(rw)
            epss.append(eps)
            lcs.append(lc)
            scores.append(sc.score)
    return {
        "CHRF_CORPUS_HYP": hyps, "CHRF_CORPUS_REFS": refss,
        "CHRF_CORPUS_CHAR_ORDER": cos, "CHRF_CORPUS_WORD_ORDER": wos,
        "CHRF_CORPUS_BETA": betas, "CHRF_CORPUS_REMOVE_WS": rws,
        "CHRF_CORPUS_EPS": epss, "CHRF_CORPUS_LOWERCASE": lcs,
        "CHRF_CORPUS_SCORE": scores,
    }


def compute_bleu_rows():
    """`BLEU.compute_bleu` over a fixed grid of sufficient statistics."""
    rows = []
    for correct, total in [([1, 0, 0, 0], [2, 1, 0, 0]), ([2, 1, 0, 0], [2, 1, 0, 0])]:
        rows.append(BLEU.compute_bleu(correct, total, 2, 2, smooth_method="none",
                                      effective_order=True, max_ngram_order=4))
    return rows


def main():
    """Compute every case with the real package and write tests/expected.py."""
    constants = {
        "TOK_CASES": TOK_CASES,
        "TOK_13A": [TOK_13A(s) for s in TOK_CASES],
        "TOK_CHAR": [TOK_CHAR(s) for s in TOK_CASES],
        "TOK_NONE": [TOK_NONE(s) for s in TOK_CASES],
    }
    constants.update(bleu_sentence_rows())
    constants.update(bleu_corpus_rows())
    constants.update(chrf_rows())
    constants.update(chrf_corpus_rows())
    constants.update({
        "CBL_CORRECT": [[1.0, 0.0, 0.0, 0.0], [2.0, 1.0, 0.0, 0.0]],
        "CBL_TOTAL": [[2.0, 1.0, 0.0, 0.0], [2.0, 1.0, 0.0, 0.0]],
        "CBL_SCORE": [c.score for c in compute_bleu_rows()],
        "CBL_PREC": [c.precisions for c in compute_bleu_rows()],
        "CBL_BP": [c.bp for c in compute_bleu_rows()],
    })
    write("sacrebleu", "BLEU, chrF and tokenizer values from sacrebleu.",
          {"sacrebleu": sacrebleu.__version__}, constants)


if __name__ == "__main__":
    main()
