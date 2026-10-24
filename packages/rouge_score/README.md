# @osprey/rouge_score

A port of Google's [rouge-score](https://github.com/google-research/google-research/tree/master/rouge)
0.1.2 to the Osprey Python flavor: `RougeScorer` for `rouge1`, `rouge2`, `rougeL` and `rougeLsum`,
its default tokenizer, and the Porter stemmer as nltk 3.10.4's `PorterStemmer` implements it, which
`use_stemmer=True` applies. Every value the suite checks was computed by the real packages
(`scripts/oracles/rouge_score.py`, run as `python -I scripts/oracles/oracle.py rouge_score`): 1137
oracle assertions over targets and predictions of every length, repeated n-grams, punctuation,
digits, multi-sentence summaries and several hundred stemmer inputs.

## API

| Osprey | rouge_score | Notes |
| --- | --- | --- |
| `RougeScorer.RougeScorer(rouge_types, use_stemmer, split_summaries)` | `rouge_scorer.RougeScorer(rouge_types, use_stemmer, split_summaries)` | A record of the options |
| `RougeScorer.score(scorer, target, prediction)` | `RougeScorer.score` | A list of `Scored` rows, one per requested type, in the order requested |
| `RougeScorer.score_multi(scorer, targets, prediction)` | `RougeScorer.score_multi` | The best `fmeasure` per type over the targets |
| `RougeScorer.Score`, `RougeScorer.Scored` | `scoring.Score` | `precision`, `recall`, `fmeasure`; `Scored` names the type |
| `RougeScorer.fmeasure(precision, recall)` | `scoring.fmeasure` | |
| `Tokenize.tokenize(text, use_stemmer)` | `tokenize.tokenize` | Lowercase, non-`[a-z0-9]` to spaces, optional stemming |
| `Stemmer.stem(word)` | `nltk.stem.PorterStemmer(mode=NLTK_EXTENSIONS).stem` | The NLTK extensions mode rouge_score uses |

## Divergences

- `rougeLsum` splits sentences on newlines; `split_summaries=True`'s nltk sentence splitter is not
  ported, so the newline split is used for it as well.
- `scoring.BootstrapAggregator` is not ported: it samples randomly.
- Shared codepoint helpers come from `@osprey/sacrebleu`'s `Text` and `Ngrams` modules, which this
  package depends on.

## Tests

`tests/main.py` runs under `@osprey/testing`'s strict arithmetic policy on the default, GC and ARC
backends and prints `1137 passed, 0 failed`; ARC exits with zero live objects.
