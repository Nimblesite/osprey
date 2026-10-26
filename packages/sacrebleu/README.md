# @osprey/sacrebleu

A port of [sacrebleu](https://github.com/mjpost/sacrebleu) 2.6.0's BLEU and chrF metrics and
its `none`, `13a` and `char` tokenizers to the Osprey Python flavor. Every value the suite checks
was computed by the real sacrebleu (`scripts/oracles/sacrebleu.py`, run as
`python -I scripts/oracles/oracle.py sacrebleu`): 3084 oracle assertions over sentence and corpus
BLEU with every smoothing method, effective order, lowercasing and multiple references, chrF2 and
chrF++, and the tokenizers over ASCII, punctuation, digits, accents, CJK and emoji text.

## API

| Osprey | sacrebleu | Notes |
| --- | --- | --- |
| `Bleu.BLEU(lowercase, force, tokenize, smooth_method, smooth_value, max_ngram_order, effective_order)` | `sacrebleu.metrics.BLEU(...)` | A record of the options; `smooth_method` is `exp`, `floor`, `add-k` or `none` |
| `Bleu.sentence_bleu(hypothesis, references)`, `Bleu.corpus_bleu(hypotheses, references)` | `sacrebleu.sentence_bleu`, `sacrebleu.corpus_bleu` | Default options, as the module-level helpers |
| `Bleu.score_sentence(bleu, hypothesis, references)`, `Bleu.score_corpus(bleu, hypotheses, references)` | `BLEU.sentence_score`, `BLEU.corpus_score` | Options from the `BLEU` record |
| `Bleu.raw_corpus_bleu(hypotheses, references, smooth_value)` | `sacrebleu.raw_corpus_bleu` | `tokenize='none'`, floor smoothing |
| `Bleu.compute_bleu(correct, total, sys_len, ref_len, smooth_method, smooth_value, effective_order, max_ngram_order)` | `BLEU.compute_bleu` | The closed form over n-gram statistics |
| `Bleu.BLEUScore` | `BLEUScore` | `score`, `counts`, `totals`, `precisions`, `bp`, `sys_len`, `ref_len` |
| `Chrf.CHRF(char_order, word_order, beta, lowercase, whitespace, eps_smoothing)` | `sacrebleu.metrics.CHRF(...)` | `word_order=2` is chrF++ |
| `Chrf.sentence_chrf`, `Chrf.corpus_chrf`, `Chrf.score_sentence`, `Chrf.score_corpus` | `sentence_chrf`, `corpus_chrf`, `CHRF.sentence_score`, `CHRF.corpus_score` | |
| `Chrf.CHRFScore` | `CHRFScore` | `score`, `char_order`, `word_order`, `beta` |
| `Tokenizer.tokenize(name, line)`, `tokenize_13a`, `tokenize_char`, `tokenize_none` | `sacrebleu.tokenizers` `13a`, `char`, `none` | `13a` is `TokenizerV13a`'s rule sequence written as explicit scans |
| `Ngrams.word_ngrams`, `char_ngrams`, `counter_*` | `sacrebleu.utils.extract_all_word_ngrams`, `extract_all_char_ngrams` | Counters are sorted `(ngram, count)` lists |
| `Text.codepoints`, `from_codepoints`, `map_codepoints`, `collapse_whitespace`, ... | — | Codepoint primitives shared with `@osprey/rouge-score` |

## Divergences

- **Tokenizers.** `none`, `13a` and `char` are ported. `intl`, `zh`, `ja-mecab`, `ko-mecab`, `spm`
  and `flores` are not: they need Unicode category tables or external models.
- **TER** and the `sacrebleu` command line are not ported.
- **Signatures.** `BLEUScore.format()` is not ported; the score fields are compared instead.
- Strings are UTF-8 and tokenization runs over codepoints, so `char` n-grams are codepoints, as in
  sacrebleu's `str` handling.
- Hypotheses and references are lists of strings; a corpus with several references is a list of
  reference streams, as sacrebleu takes them.

## Tests

`tests/main.py` runs under `@osprey/testing`'s strict arithmetic policy on the default, GC and ARC
backends and prints `3084 passed, 0 failed`; ARC exits with zero live objects.
