"""CPython oracle for @osprey/rouge_score: RougeScorer and Porter-stemmer values.

Writes packages/rouge_score/tests/expected.py. Run with the oracle virtualenv:
`python scripts/oracles/rouge_score.py` (no `-I`).
"""
import sys

from expected_module import write

# This script is named rouge_score.py; drop its own directory from sys.path so
# `import rouge_score` resolves to the real package, not to this file.
sys.path.pop(0)

from nltk.stem.porter import PorterStemmer
from rouge_score import rouge_scorer, tokenize

STEMMER = PorterStemmer(mode=PorterStemmer.NLTK_EXTENSIONS)

ROUGE_TYPES = ["rouge1", "rouge2", "rougeL", "rougeLsum"]

# (target, prediction) pairs; empty, singleton, identical, repeated, punctuation,
# digits, unicode, multi-line (for rougeLsum's newline sentence split).
CASES = [
    ("", ""),
    ("", "hello world"),
    ("hello world", ""),
    ("hello world", "hello world"),
    ("hello", "hello"),
    ("the", "the"),
    ("the cat sat on the mat", "the cat sat on the mat"),
    ("the quick brown fox jumps over the lazy dog", "the quick brown fox jumps over the lazy dog"),
    ("the quick brown fox", "the quick brown dog"),
    ("cat cat cat cat", "cat"),
    ("the cat the cat", "the cat"),
    ("dog dog dog", "cat cat cat"),
    ("hello", "hello world"),
    ("hello world", "hello"),
    ("one two three four five six", "one two three four five six"),
    ("one two three", "one two four"),
    ("It's a test.", "It is a test."),
    ("He said \"hello\" (loudly)!", "He said \"hello\" (loudly)!"),
    ("3.5 million, 50%", "3.5 million, 50%"),
    ("café naïve", "café naïve"),
    ("你好世界", "你好世界"),
    ("emoji 👍 test", "emoji 👍 test"),
    ("a b c d e f g h i j", "a b c d e f g h i j"),
    ("word " * 60, "word " * 60),
    ("word " * 60, "other " * 60),
    ("first line\nsecond line", "first line\nsecond line"),
    ("the cat sat\non the mat", "the cat sat\non the mat"),
    ("sentence one\nsentence two", "sentence one\nsentence three"),
    ("the cat", "a cat"),
    ("running runs runner", "run runs running"),
    ("relational conditional", "relate condition"),
    ("happiness hopefulness", "happy hopeful"),
    ("caresses ponies cats", "caress poni cat"),
    ("relational", "relate"),
    ("conditional", "condition"),
    ("valenci", "valence"),
    ("digitizer", "digitize"),
    ("triplicate", "triplic"),
    ("probate rate cease", "probat rate ceas"),
    ("controll roll", "control roll"),
]

# (use_stemmer, split_summaries) — only split_summaries=False (newline split) is
# ported; nltk's sentence tokenizer is not.
CONFIGS = [
    (False, False),
    (True, False),
]


def rouge_rows():
    targets, preds, stemmers, splits = [], [], [], []
    fields = {f"{t}_{m}": [] for t in ROUGE_TYPES for m in ("p", "r", "f")}
    for target, pred in CASES:
        for use_stemmer, split in CONFIGS:
            scorer = rouge_scorer.RougeScorer(
                ROUGE_TYPES, use_stemmer=use_stemmer, split_summaries=split)
            result = scorer.score(target, pred)
            targets.append(target)
            preds.append(pred)
            stemmers.append(use_stemmer)
            splits.append(split)
            for t in ROUGE_TYPES:
                fields[f"{t}_p"].append(float(result[t].precision))
                fields[f"{t}_r"].append(float(result[t].recall))
                fields[f"{t}_f"].append(float(result[t].fmeasure))
    out = {
        "ROUGE_TARGET": targets, "ROUGE_PRED": preds,
        "ROUGE_STEMMER": stemmers, "ROUGE_SPLIT": splits,
    }
    out.update(fields)
    return out


def tokenize_rows():
    texts = [
        "", "hello world", "Hello, World!", "the cat's hat",
        "3.5 million, 50%", "café naïve", "你好世界", "emoji 👍 test",
        "a  b   c", "  leading trailing  ", "running runs runner",
        "It's a test.", "He said \"hello\" (loudly)!", "word " * 10,
        "a.b.c", "under_score dash-dash", "123 456", "don't won't can't",
    ]
    plain = [tokenize.tokenize(t, None) for t in texts]
    stemmed = [tokenize.tokenize(t, STEMMER) for t in texts]
    return {"TOKENIZE_TEXT": texts, "TOKENIZE_PLAIN": plain, "TOKENIZE_STEM": stemmed}


def score_multi_rows():
    """`RougeScorer.score_multi` over a few target lists."""
    pairs = [
        (["hello world", "hello there world"], "hello world"),
        (["the cat sat", "a cat sat"], "the cat sat"),
        (["one", "one two three"], "one two three four"),
    ]
    targets, preds = [], []
    fields = {f"MULTI_{t}_{m}": [] for t in ROUGE_TYPES for m in ("p", "r", "f")}
    for targets_list, pred in pairs:
        scorer = rouge_scorer.RougeScorer(ROUGE_TYPES)
        result = scorer.score_multi(targets_list, pred)
        targets.append(targets_list)
        preds.append(pred)
        for t in ROUGE_TYPES:
            fields[f"MULTI_{t}_p"].append(float(result[t].precision))
            fields[f"MULTI_{t}_r"].append(float(result[t].recall))
            fields[f"MULTI_{t}_f"].append(float(result[t].fmeasure))
    out = {"MULTI_TARGETS": targets, "MULTI_PRED": preds}
    out.update(fields)
    return out


STEM_WORDS = [
    "caresses", "ponies", "ties", "caress", "cats", "feed", "agreed",
    "plastered", "bled", "motoring", "sing", "conflated", "troubled", "sized",
    "hopping", "tanned", "falling", "hissing", "fizzing", "failing", "filing",
    "happy", "sky", "relational", "conditional", "rational", "valenci",
    "hesitanci", "digitizer", "conformabli", "radicalli", "differentli",
    "vileli", "analogousli", "vietnamization", "predication", "operator",
    "feudalism", "decisiveness", "hopefulness", "callousness", "formaliti",
    "sensitiviti", "sensibiliti", "triplicate", "formative", "formalize",
    "electriciti", "electrical", "hopeful", "goodness", "revival", "allowance",
    "inference", "airliner", "gyroscopic", "adjustable", "defensible",
    "irritant", "replacement", "adjustment", "dependent", "adoption",
    "homologou", "communism", "activate", "angulariti", "homologous",
    "effective", "bowdlerize", "probate", "rate", "cease", "controll", "roll",
    "the", "is", "to", "a", "sky", "die", "lie", "tie", "news", "inning",
    "outing", "canning", "howe", "proceed", "exceed", "succeed", "spied",
    "died", "flies", "dies", "happy", "enjoy", "spy", "fly", "try", "food",
]


def main():
    """Compute every case with the real package and write tests/expected.py."""
    constants = rouge_rows()
    constants.update(tokenize_rows())
    constants.update(score_multi_rows())
    constants.update({
        "STEM_WORDS": STEM_WORDS,
        "STEM_OUT": [STEMMER.stem(w) for w in STEM_WORDS],
    })
    import rouge_score
    import nltk
    write("rouge_score", "RougeScorer and Porter-stemmer values from rouge_score.",
          {"rouge-score": "0.1.2", "nltk": nltk.__version__}, constants)


if __name__ == "__main__":
    main()
