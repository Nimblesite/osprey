# @osprey/tokenizers

A port of Hugging Face [`tokenizers`](https://github.com/huggingface/tokenizers) (compared with **0.23.3**) to the Osprey Python flavor. It reimplements the WordPiece (BERT), Unigram (SentencePiece/T5) and WordLevel models, the BERT normalizer and pre-tokenizer, the Metaspace / Whitespace / Punctuation / Digits / CharDelimiterSplit pre-tokenizers, the WordPiece / Metaspace / ByteFallback decoders, and the BERT TemplateProcessing post-processor. Byte-level BPE (GPT-2/tiktoken style) is `@osprey/tiktoken`'s job and is not duplicated here.

The suite compares ids, tokens, char offsets, type ids and decodes with the real package on empty, whitespace, punctuation, accented Latin, CJK, emoji, very-long-word and unknown inputs.

## API map

| Osprey module | Function | Original |
| --- | --- | --- |
| `Unicode` | `is_whitespace`, `is_control`, `is_chinese_char`, `is_punctuation`, `is_ascii_punctuation`, `is_numeric`, `is_word_char`, `is_mark_nonspacing`, `lowercase`, `strip_base` | Rust `char`/`unicode_categories` helpers used by the tokenizers normalizers/pre-tokenizers |
| `Normalizers` | `bert_normalize(text, clean, handle, strip, lower)` | `tokenizers.normalizers.BertNormalizer(...).normalize_str` |
| `PreTokenizers` | `bert_pretokenize`, `whitespace_pretokenize`, `whitespace_split`, `punctuation_pretokenize`, `digits_pretokenize`, `char_delimiter_split`, `metaspace_pretokenize` | `pre_tokenizers.BertPreTokenizer`, `Whitespace`, `WhitespaceSplit`, `Punctuation`, `Digits`, `CharDelimiterSplit`, `Metaspace` |
| `Models` | `build_wordpiece` + `wordpiece_tokenize`, `build_unigram` + `unigram_tokenize`/`unigram_encode`, `build_wordlevel` + `wordlevel_tokenize` | `models.WordPiece`, `models.Unigram`, `models.WordLevel` |
| `Decoders` | `wordpiece_decode_chain`, `metaspace_decode_chain`, `byte_fallback_decode_chain` | `decoders.WordPiece`, `decoders.Metaspace`, `decoders.ByteFallback` |
| `Processors` | `bert_template(encodings, cls_id, sep_id)` | `processors.TemplateProcessing(single="[CLS]:0 $A:0 [SEP]:0", pair="[CLS]:0 $A:0 [SEP]:0 $B:1 [SEP]:1")` |
| `Tokenizer` | `wordpiece_encode`, `wordpiece_encode_pair`, `unigram_encode_pipeline`, `wordlevel_encode_pipeline`, and the `*_decode_pipeline` counterparts | `Tokenizer.encode` / `Tokenizer.encode_pair` / `Tokenizer.decode` (Python binding's char offsets) |

## Divergences

- **Lowercase/strip-accent tables** cover ASCII, Latin-1 Supplement, Latin Extended-A, basic Greek, fullwidth Latin, the dotted İ and sharp ẞ; other scripts are passed through unchanged. Strip-accents implements NFD-then-drop-`Mn` for those ranges (the combining marks U+0300..U+036F).
- **`is_control`** covers the Cc controls (U+0000..U+001F, U+007F..U+009F); the wider Unicode "C" categories (Cf/Cs/Co/Cn) are not tabulated, so e.g. soft hyphens are kept rather than dropped. The test corpus stays inside Cc.
- **`is_word_char`** (the `Whitespace` pre-tokenizer's `\w`) tabulates letters, marks, decimal digits, connector punctuation and join controls from the Unicode Character Database, matching the Rust regex `\w` for the corpus.
- **WordPieceTrainer / UnigramTrainer are not ported.** Both iterate a Rust `HashMap` whose order varies per process, so training twice on the same corpus yields different vocabularies (non-deterministic). The suite uses a fixed hand-built WordPiece vocabulary and a Unigram vocabulary that was trained once and frozen in `scripts/oracles/tokenizers.py`; the algorithms the trainers produce are exercised directly.
- The record `Norm` (chars + per-codepoint original byte ranges) models tokenizers' `NormalizedString` alignment; offsets are converted to the Python binding's **char** offsets.
- TemplateProcessing is specialized to the BERT templates above rather than a generic template parser.

## Compiler workarounds

`best_set` in `Models` is built with `fold` (not a `map` lambda) and `byte_tokens` with recursion. Both were written around an ARC leak in `toList(map(...))` closures that capture a record; the suite found that compiler bug and it is fixed (the owner now lives in the loop body's frame), so the forms are a style choice now, not a workaround.
