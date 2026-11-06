# Qwen3.5 model

The Qwen3.5 text model and its tool-call layer, running on Osprey GPU buffers. It sits
beside the [GPT-2 example](../../gpt2/) and builds on
[`@osprey/transformers`](../../../../packages/transformers/), whose generation layer
(logits processors, sampling and stopping criteria) is model-independent. The
[server](../server/) serves it, [`run`](../run/) and [`speed`](../speed/) are the one-file
real-weights program and its kernel benchmarks, and [`eval`](../eval/) is the Phase 0
baseline, all for [plan 0049](../../../../../docs/plans/0049-local-coding-model.md).

## Origin, license and thanks

**Original project:** the `qwen3_5` model in
[Transformers](https://github.com/huggingface/transformers) 5.20.2
(`src/transformers/models/qwen3_5/modeling_qwen3_5.py`), by the Qwen team and Hugging Face.

**How it was ported.** The model modules (`config`, `norm`, `rope`, `mlp`, `attention`,
`deltanet`, `cache`, `model`, `hf`) translate Transformers' PyTorch code into Osprey GPU
code, keeping its structure, names and arithmetic, so they are treated as derived from
that code and each says so in its header. `device` and `shapes` are Osprey's own kernel
plumbing. The tool-call modules are a reimplementation written for Osprey.

**License.** The derived model modules stay under Apache-2.0, and so does the Qwen chat
template carried as a test oracle (copies in [LICENSES/](LICENSES/)). The rest is MIT,
Copyright NIMBLESITE PTY LTD, Australia; that grant does not relicense any Apache-2.0
code. [NOTICE.md](NOTICE.md) names every derived file and what was changed.

**Thanks** to the Qwen team at Alibaba Cloud for Qwen3.5 and its chat template, to Hugging
Face and the Transformers contributors, and to the [vLLM](https://github.com/vllm-project/vllm)
project, whose `Qwen3CoderToolParser` set the tool-call framing rules. Naming them credits
their work; it does not imply that they endorse this port.

## Tests

`tests/` runs 2,315 assertions under `Check.strictly`, identical under GC, ARC, Metal GC and
Metal ARC and golden in `tests/expected_output.txt`: the tool-call modules (`ToolSupport`,
`ParserCases`, `ConstrainCases`), the device model against the fixture (`ModelCases`,
`FixtureCases`, `TiledCases`) and the GGUF loader against `tests/fixtures/` (`GgufCases`,
written by `oracle/qwen3_5_gguf.cpython`). Run it with
`osprey src/osprey/examples/llms/qwen/model/tests --run --memory=gc` (and `--memory=arc`,
and `OSPREY_GPU=metal` for the device); it is not in the CI package matrix yet.

## The tiny `Qwen3_5TextConfig` fixture

`tests/expected.py`'s `MODEL_*` constants are the `qwen3_5` text model's reference
activations for Phase 3 of [plan 0049](../../../../../docs/plans/0049-local-coding-model.md),
written by `oracle/qwen3_5.cpython` (run with
`python -I src/osprey/packages/testing/oracle/oracle.cpython qwen3_5` in the torch oracle
environment). Transformers
5.20.2 builds a seeded `torch.manual_seed(20261105)` `Qwen3_5TextModel` in float32 on CPU,
and the oracle records, for `MODEL_PARAM_*`, every parameter tensor's name, shape, dtype and
row-major values, and for `MODEL_HIDDEN_*` every layer's output hidden state and the final
norm output over three inputs: one token, eight tokens in one pass, and the same eight fed
one token at a time through `DynamicCache`. `MODEL_CACHE_*` is the conv, recurrent and
key/value state that cached run leaves behind, `MODEL_HOOK_NAMES`/`MODEL_HOOK_SHAPES` the
activations a forward hook can record (in fire order), and `MODEL_DTYPE_PATH` the dtype each
value carries from `input_ids` (`torch.int64`) to state (`torch.float32`). Values are the
shortest decimals that read back as the same binary32 bits, and the whole module is about
2.2 MB, so CI never downloads weights.

4 layers keep the class's own `layer_types` pattern (three `linear_attention` layers then
one `full_attention`); only the linear-attention widths and head counts are smaller than
the class defaults (8/8 over 2/4 heads, keeping the real 16:32 key:value ratio), which is
what keeps 154,512 parameters inside a 3 MB fixture. The suite checks the fixture's shapes
and counts (the declared parameter count is the sum of the recorded shapes, and every
tensor holds one value per element), and the device model built from those parameters
reproduces every recorded activation and cache state (`[LLM-MODEL-ORACLE]`).

## Tool calls: `tool_*`

The Qwen3.5 tool-call layer: a streaming parser for the chat template's
`<tool_call><function=…><parameter=…>` format, a logits processor that makes a
schema-invalid call unsamplable, and the typed tool error a harness recovers from
([LLM-TOOL-PARSE], [LLM-TOOL-CONSTRAIN], [LLM-TOOL-REPAIR] of
[LocalCodingModel](../../../../../docs/specs/applications/LocalCodingModel.md)). There is no
Hugging Face analogue to port: the format is the model's, so the reference is the
published chat template and the parser a serving engine ships for it.

| Module | Contents |
| --- | --- |
| `qwen.ToolSchema` | the JSON Schema sub-language: `validate`, `coerce`, the readers (`kind`, `members`, `property_names`, `property_schema`, `required_names`, `items_schema`, `allows_extra`, `enum_values`, `kind_matches`), the tool-list readers (`tool_list`, `tool_name`, `tool_named`, `parameters`) and violation text (`violation`, `violation_keyword`, `violation_path`) |
| `qwen.ToolParser` | `start(tools)`, `feed(state, chunk)`, `finish(state)`, `parse(tools, text)`, and the `Step`/`Call` results; `error_field(error, name)` reads a tool error |
| `qwen.ToolJsonScan` | `begin(schema)`, `feed(stack, byte)`, `finished(stack)`: the schema-directed JSON scanner the mask is built on |
| `qwen.ToolConstrain` | `Constrain(tools, vocabulary, filter_value)`, `apply`, `mask`, `start`, `advance`, `complete`, `accepts`, `decode` |

### The format

The template at `Qwen/Qwen3.8-27B` revision `1d4bf0f2ff6012fd82039f2fa52739d0dd7c60c0`
(`oracle/chat_template.jinja`, sha256 `c3cf9e34…`) writes a call as
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

The tool-call cases run under `Check.strictly`; `tests/expected_tools.py` is
regenerated by `oracle/tool_calls.cpython` (run through
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
