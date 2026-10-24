# @osprey/transformers

A port of Hugging Face Transformers' model-independent generation layer to the Osprey Python flavor ([spec 0043](../../docs/specs/ecosystem/0043-PythonFlavorPorts.md)). Compared with **transformers 5.20.2** and **torch 2.14.1** (float64 in the oracle, `torch.set_default_dtype(torch.float64)`).

The port owns the generation layer only — logits processors/warpers, the sampling primitives, and stopping criteria — operating on a batch of logits rows given as `list[list[float]]` and token histories as `list[list[int]]`. Model architectures arrive later on top of `@osprey/torch`.

## Layout

| Module | Contents |
| --- | --- |
| `transformers.Math` | stable `softmax`, `log_softmax`, `argmax`, `multinomial`, `neg_inf` |
| `transformers.Lists` | shared `contains_int`, `last_n` over `list[int]` |
| `transformers.LogitsProcess` | every processor/warper constructor, `Processor`, `apply`, `apply_list` |
| `transformers.StoppingCriteria` | `MaxLengthCriteria`, `EosTokenCriteria`, `ends_with_any`, `Criterion`, `stopped`, `stopped_any` |
| `transformers.Generation` | `generate`, a model-independent driver |

## API map

Each Hugging Face class becomes a constructor function returning a tagged `Processor` record; `apply(p, input_ids, scores)` runs one and `apply_list(ps, input_ids, scores)` is `LogitsProcessorList.__call__` (left-to-right composition). Warpers operate on one logits row; `input_ids` is the token history.

| Osprey function | Original (`transformers`) |
| --- | --- |
| `TemperatureLogitsWarper(temperature)` | `TemperatureLogitsWarper` |
| `TopKLogitsWarper(top_k, filter_value, min_tokens_to_keep)` | `TopKLogitsWarper` |
| `TopPLogitsWarper(top_p, filter_value, min_tokens_to_keep)` | `TopPLogitsWarper` |
| `MinPLogitsWarper(min_p, filter_value, min_tokens_to_keep)` | `MinPLogitsWarper` |
| `TypicalLogitsWarper(mass, filter_value, min_tokens_to_keep)` | `TypicalLogitsWarper` |
| `EpsilonLogitsWarper(epsilon, filter_value, min_tokens_to_keep)` | `EpsilonLogitsWarper` |
| `EtaLogitsWarper(epsilon, filter_value, min_tokens_to_keep)` | `EtaLogitsWarper` |
| `RepetitionPenaltyLogitsProcessor(penalty, prompt_ignore_length)` | `RepetitionPenaltyLogitsProcessor` |
| `NoRepeatNGramLogitsProcessor(ngram_size)` | `NoRepeatNGramLogitsProcessor` |
| `MinLengthLogitsProcessor(min_length, eos_ids)` | `MinLengthLogitsProcessor` |
| `MinNewTokensLengthLogitsProcessor(prompt_length_to_skip, min_new_tokens, eos_ids)` | `MinNewTokensLengthLogitsProcessor` |
| `ForcedBOSTokenLogitsProcessor(bos_token_id)` | `ForcedBOSTokenLogitsProcessor` |
| `ForcedEOSTokenLogitsProcessor(max_length, eos_ids)` | `ForcedEOSTokenLogitsProcessor` |
| `SuppressTokensLogitsProcessor(suppress_ids)` | `SuppressTokensLogitsProcessor` |
| `NoBadWordsLogitsProcessor(bad_words_ids, eos_ids)` | `NoBadWordsLogitsProcessor(bad_words_ids, eos_token_id)` |
| `apply_list(processors, input_ids, scores)` | `LogitsProcessorList.__call__` |
| `MaxLengthCriteria(max_length)` | `MaxLengthCriteria` |
| `EosTokenCriteria(eos_ids)` | `EosTokenCriteria` |
| `ends_with_any(text, stop_strings)` | `StopStringCriteria` (suffix check only, see below) |
| `stopped_any(criteria, tokens)` | `StoppingCriteriaList.__call__` |
| `Math.softmax` / `Math.log_softmax` | `torch.softmax` / `torch.log_softmax` (max subtraction) |
| `Math.argmax` | `torch.argmax` (first maximal index on ties) |
| `Math.multinomial(probs, u)` | `torch.multinomial` (inverse CDF via `torch.searchsorted`) |
| `Generation.generate` | `GenerationMixin.generate` (our own pure-logits driver) |

## Divergences

- **Tagged records, not classes.** The Python flavor has no `@union` or OOP, so each processor/criterion is a `Processor`/`Criterion` record with a `kind` tag and `apply`/`stopped` dispatch statically. Each HF constructor becomes a function returning that record.
- **No optional arguments.** `filter_value` and `min_tokens_to_keep` are explicit on every warper; `eos_token_id` is always `list[int]` (wrap a single id as `[id]`); `prompt_ignore_length` is `int` (use `0` for "none"); `EosTokenCriteria` checks the single newest token (`new_token_length=1`, the transformers default).
- **Sort tie order.** `torch.sort` on CPU is only stable for short tensors (≤ 16 in our tests); this port sorts ascending with ties broken by the smaller index, which is exactly `torch.sort`'s order at those sizes and the documented order for larger vocabularies. `torch.topk`'s tie order is unspecified, so `MinPLogitsWarper` keeps smallest-index ties and its suite uses distinct scores.
- **`StopStringCriteria` is not ported.** Osprey's effect system cannot carry a `list[int] -> str` decode callback through a record; `ends_with_any(text, stop_strings)` is the suffix check, and callers run their own decode first. transformers precompiles stop strings against a tokenizer's vocabulary, so this only diverges on partial-token overhangs.
- **`NoBadWordsLogitsProcessor` float64 bug.** transformers 5.20.2 crashes on length-1 bad words under torch float64 (an `Index put` dtype mismatch in `SequenceBiasLogitsProcessor._prepare_bias_variables`). The port implements the intended semantics (bias `-inf`), cross-checked against the real processor for length ≥ 2 sequences in the oracle.
- **Not present in 5.20.2.** `FrequencyPenalty`/`PresencePenalty` are not `LogitsProcessor` classes in this version (they are applied inside `generate`), and `BeamHypotheses`/`length_penalty` were removed when beam search moved to `candidate_generator.py`; neither is ported.
- **Sampling RNG.** torch's multinomial RNG stream is not reproduced. `Math.multinomial(probs, u)` takes an explicit uniform draw `u` in `[0, 1)` and selects the first index whose cumulative probability strictly exceeds `u` — the oracle computes the same rule as `torch.searchsorted(torch.cumsum(probs, -1), u, right=True)` clamped to the last index when none exceeds `u`. `Math.argmax` returns the first index holding the maximum, matching `torch.argmax` on ties (the oracle's tie rows all confirm torch's first-max tie rule).
- **`generate` driver.** transformers' `generate` is not a pure logits-row function, so the port ships its own `generate(next_logits, tokens, processors, stoppers, sampler, max_new_tokens)` higher-order driver; the suite reproduces its reference flow with the real processors and greedy argmax.

## Tests

`tests/` runs 375 assertions under `Check.strictly` against `tests/expected.py`, regenerated by `scripts/oracles/transformers.py` (the CPython oracle, written through the shared `scripts/oracles/expected_module.py` writer). The oracle computes `softmax`/`log_softmax`/`argmax`/`multinomial` with torch itself, builds logits rows with ties, all-equal entries, `-inf` entries, negatives and extreme magnitudes, and records every real processor's output exactly (`-inf` exactly, `rtol=1e-12` for arithmetic, `1e-9` downstream of exp/ln). It also runs the real `MaxLengthCriteria`/`EosTokenCriteria` and Python `str.endswith` for the stop-string check.

## Bug hunts

`hunt/` holds `osprey hunt` drivers ([PORT-HUNT]), each mixing symbolic logits with concrete NaN, ±inf, huge (`exp(700.0)`) and subnormal (`exp(-745.0)`) values: `math` (softmax, log_softmax, argmax, multinomial; 7,165 paths), `warpers` (temperature with a symbolic temperature, so `0.0` reaches the divide-by-zero policy, min-p, epsilon; 680 paths) and `topp` (top-p, typical, eta; 680 paths). No findings.
