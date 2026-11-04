# @osprey/transformers

A port of Hugging Face Transformers' model-independent generation layer to the Osprey Python flavor ([spec 0043](../../../../docs/specs/ecosystem/0043-PythonFlavorPorts.md)). Compared with **transformers 5.20.2** and **torch 2.14.1** (float32 in the oracle, `torch.set_default_dtype(torch.float32)`).

The port owns the generation layer only — logits processors/warpers, the sampling primitives, and stopping criteria — operating on a batch of logits rows given as `list[list[float32]]` and token histories as `list[list[int]]`. Model architectures arrive later on top of `@osprey/torch`.

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
- **`NoBadWordsLogitsProcessor` dtype bug.** transformers 5.20.2 crashes on length-1 bad words under torch float64 (an `Index put` dtype mismatch in `SequenceBiasLogitsProcessor._prepare_bias_variables`). The port implements the intended semantics (bias `-inf`), cross-checked against the real processor for length ≥ 2 sequences in the oracle.
- **Not present in 5.20.2.** `FrequencyPenalty`/`PresencePenalty` are not `LogitsProcessor` classes in this version (they are applied inside `generate`), and `BeamHypotheses`/`length_penalty` were removed when beam search moved to `candidate_generator.py`; neither is ported.
- **Sampling RNG.** torch's multinomial RNG stream is not reproduced. `Math.multinomial(probs, u)` takes an explicit uniform draw `u` in `[0, 1)` and selects the first index whose cumulative probability strictly exceeds `u` — the oracle computes the same rule as `torch.searchsorted(torch.cumsum(probs, -1), u, right=True)` clamped to the last index when none exceeds `u`. `Math.argmax` returns the first index holding the maximum, matching `torch.argmax` on ties (the oracle's tie rows all confirm torch's first-max tie rule).
- **`generate` driver.** transformers' `generate` is not a pure logits-row function, so the port ships its own `generate(next_logits, tokens, processors, stoppers, sampler, max_new_tokens)` higher-order driver; the suite reproduces its reference flow with the real processors and greedy argmax.

## Tests

`tests/` runs 397 assertions under `Check.strictly` against `tests/expected.py`, regenerated by `src/osprey/packages/transformers/oracle/transformers.cpython` (the CPython oracle, written through the shared `src/osprey/packages/testing/oracle/expected_module.cpython` writer). The oracle computes `softmax`/`log_softmax`/`argmax`/`multinomial` with torch itself, builds logits rows with ties, all-equal entries, `-inf` entries, negatives and extreme magnitudes, and records every real processor's output (`-inf` exactly, `rtol=1e-12` for arithmetic, `1e-9` downstream of binary32 exp/ln). It also runs the real `MaxLengthCriteria`/`EosTokenCriteria` and Python `str.endswith` for the stop-string check. The two additional checks retain empty softmax and log-softmax rows.

Softmax uses a fixed four-lane binary32 sum and reciprocal multiplication matching the pinned ARM CPU oracle's [reduction order](https://github.com/pytorch/pytorch/blob/main/aten/src/ATen/cpu/vec/functional_base.h) and [normalization](https://github.com/pytorch/pytorch/blob/main/aten/src/ATen/native/cpu/SoftMaxKernel.cpp). Other upstream CPU and GPU kernels can use a different reduction order.

## Bug hunts

`hunt/` holds `osprey hunt` drivers ([PORT-HUNT]), each mixing symbolic logits with concrete NaN, ±inf, huge (`exp(700.0)`) and subnormal (`exp(-745.0)`) values: `math` (softmax, log_softmax, argmax, multinomial; 7,165 paths), `warpers` (temperature with a symbolic temperature, so `0.0` reaches the divide-by-zero policy, min-p, epsilon; 680 paths) and `topp` (top-p, typical, eta; 680 paths). No findings.

## Tool calls: `qwen3_5_tool_*`

The Qwen3.5 tool-call layer: a streaming parser for the chat template's
`<tool_call><function=…><parameter=…>` format, a logits processor that makes a
schema-invalid call unsamplable, and the typed tool error a harness recovers from
([LLM-TOOL-PARSE], [LLM-TOOL-CONSTRAIN], [LLM-TOOL-REPAIR] of
[LocalCodingModel](../../../../docs/specs/applications/LocalCodingModel.md)). There is no
Hugging Face analogue to port: the format is the model's, so the reference is the
published chat template and the parser a serving engine ships for it.

| Module | Contents |
| --- | --- |
| `transformers.ToolSchema` | the JSON Schema sub-language: `validate`, `coerce`, the readers (`kind`, `members`, `property_names`, `property_schema`, `required_names`, `items_schema`, `allows_extra`, `enum_values`, `kind_matches`), the tool-list readers (`tool_list`, `tool_name`, `tool_named`, `parameters`) and violation text (`violation`, `violation_keyword`, `violation_path`) |
| `transformers.ToolParser` | `start(tools)`, `feed(state, chunk)`, `finish(state)`, `parse(tools, text)`, and the `Step`/`Call` results; `error_field(error, name)` reads a tool error |
| `transformers.ToolJsonScan` | `begin(schema)`, `feed(stack, byte)`, `finished(stack)`: the schema-directed JSON scanner the mask is built on |
| `transformers.ToolConstrain` | `Constrain(tools, vocabulary, filter_value)`, `apply`, `mask`, `start`, `advance`, `complete`, `accepts`, `decode` |

### The format

The template at `Qwen/Qwen3.8-27B` revision `1d4bf0f2ff6012fd82039f2fa52739d0dd7c60c0`
(`src/osprey/packages/transformers/oracle/qwen3_5_chat_template.jinja`, sha256 `c3cf9e34…`) writes a call as
`<tool_call>`, `<function=NAME>`, one `<parameter=NAME>` per argument, `</function>`,
`</tool_call>`, and a turn as `<think>reasoning</think>` followed by free text. The
`tokenizer.chat_template` inside the pinned `unsloth/Qwen3.8-27B-GGUF` weights renders
the identical `<think>`, `<tool_call>`, `<function=` and `<parameter=` bytes; only
system-message merging differs. A string argument is written verbatim, so it may span
lines; every other argument is the JSON literal the template's `tojson` produced.
vLLM's [`Qwen3CoderToolParser`](https://github.com/vllm-project/vllm/blob/v0.12.0/vllm/entrypoints/openai/tool_parsers/qwen3coder_tool_parser.py)
reads the same tags and strips one newline at each end of a value; its framing rules are
the reference here.

`feed` is incremental: a chunk may split anywhere, including inside a tag, and any split
produces the same steps as one whole-text call (the suite parses every Phase 0 turn in
1-, 2-, 5- and 11-byte chunks). `text` and `think` are the turn's own bytes — everything
between `</think>` and `<tool_call>` is text — so a harness trims what it displays.
`finish` reports a turn that stopped inside `<think>` or `<tool_call>` as a
`truncated` error instead of truncating it silently.

A call whose arguments do not satisfy the tool's schema closes as a `Call` with
`arguments = JsonNull` and a JSON tool error in `error`:
`{"kind", "keyword", "parameter", "detail", "recoverable": true}`, where `keyword` is the
JSON Schema keyword that failed. The harness sends that text back inside
`<tool_response>`, so the model can correct itself; fabricated arguments never reach it.

### The mask

Once `<tool_call>` opens, `apply`/`mask` leave `filter_value` (`-inf`) on every token
that would leave the language: only declared function names, declared and unwritten
parameter names, and values the parameter's schema accepts. Before `<tool_call>` and
after `</tool_call>` nothing is masked, so free text stays free. The language is the
template's own, so the mask never admits a turn the parser rejects — and a JSON argument
is scanned against its nested schema, which is why `{"mode": "delete"}` cannot be
sampled where the schema declares `enum ["check","apply","preview"]`, and why an object
closes only once its `required` properties are present.

`apply` follows `LogitsProcess.apply`'s shape (a token history and one logits row);
`mask` takes the machine `start`/`advance` built, so a sampler between steps pays no
replay. Both are `fold`s over the byte range, never a stack frame per byte: a 64K-token
turn must not exhaust the stack.

### Divergences

- **`Result` errors, not exceptions** (`[FAILURE-EXPLICIT]`), everywhere.
- **Coercion is stricter than vLLM's**, which degenerates to the raw string (and to
  `false` for a bad boolean) when conversion fails. Here a bad literal is a `type`
  violation, so the harness sees a recoverable tool error instead of a made-up value.
  A string argument keeps the template's text verbatim; `integer` takes a JSON integer
  literal only (`jsonschema`'s integer type would also accept `3.0`, which the template
  never writes for an integer argument); `number`, `boolean`, `array` and `object` take
  one JSON literal.
- **The mask is stricter than the parser** in one place: it requires the template's
  newline between `<parameter=NAME>` and the value, where the parser also accepts a value
  immediately after the tag. A masked turn therefore always parses, but a hand-written
  turn may parse where the mask would have refused.
- **`ToolSchema.validate` reports the first violation** in `jsonschema`'s own order
  (`type`, `enum`, `items`, `properties`, `required`, `additionalProperties`) as
  `keyword@path`, and the suite compares that pair with `jsonschema` 4.26.0's for every
  mutation of every corpus argument.

### Tests

`tests/` runs 2,138 assertions under `Check.strictly`; `tests/expected_tools.py` is
regenerated by `src/osprey/packages/transformers/oracle/tool_calls.cpython` (run through
`python -I src/osprey/packages/testing/oracle/oracle.cpython tool_calls`), which renders every turn of the 200
`src/osprey/examples/llms/qwen/eval/tool_cases.py` cases with the **real** chat template through jinja2 3.1.6
and records:

- `CASES`: the rendered turn, its `<think>`/text split, the call it encodes, and that
  case's constrained-sampling fixture (a token table, the prefix token ids and the ids
  the corpus turn uses);
- `COERCE`: every parameter text the template wrote (round-tripped to the argument the
  caller passed) plus boundary texts, each with `jsonschema`'s verdict;
- `VALIDATE`: every single-part mutation of a real argument with `jsonschema` 4.26.0's
  keyword and instance path;
- `REPAIR`: calls the template would never write, each with `jsonschema`'s keyword for
  why it fails.

The suite parses all 200 turns whole and in chunks, validates and coerces the oracle
cases, walks each of the 200 masked target paths (the mask must admit every token of the
corpus turn), samples the same 200 cases adversarially (a seeded draw is taken whenever
the mask admits it, so a corrupting token is what the run takes by default; all 200 end a
schema-valid call), and keeps four clean controls: with the mask off, the same seeded
source stops producing a parseable call. Every case's fixture vocabulary carries the tags,
the declared names, the nested property names and the members it needs, and closes it
under the remainders of every confined literal, so a seeded draw can never strand the
grammar inside a name it has no token to finish.

Not covered: the real 248k-token Qwen vocabulary (the fixture is a per-case table of a few
dozen tokens, so a token boundary the real vocabulary has and this one does not is
untested) and a live model.
