# Qwen server — the Ollama and OpenAI protocols on Osprey

Osprey serves the local coding model itself ([LLM-SERVING]): a harness that
speaks the Ollama or OpenAI protocol points here instead of at a real Ollama,
which cannot host this port — `ollama create` runs a GGUF on Ollama's own
engines, so no Osprey code would execute. The listener is
`127.0.0.1:11435`, so it runs beside a real Ollama on 11434, which the baseline
and the evaluation need.

## Run it

`make -C macos qwen-server` rebuilds the compiler and runtime and serves the real
weights on Metal (`qwen-server-quick` serves with the toolchain as built); in a
second terminal, `make -C macos qwen-client` chats with it through the stock
`ollama` command (`OLLAMA_HOST=127.0.0.1:11435 ollama run qwen3.8:27b`), which
needs only `/`, `/api/show` and `/api/chat` from this server. By hand:

```sh
target/release/osprey src/osprey/examples/llms/qwen/server --compile --memory=arc -o target/llms/qwen-server/server-arc
printf '11435\n%s\n' ~/.cache/osprey-llm/gguf/Qwen3.8-27B-UD-Q4_K_XL.gguf \
  | OSPREY_GPU=metal target/llms/qwen-server/server-arc
```

Two lines of stdin: the port (empty, or a closed stdin, for 11435) and the GGUF
to serve. An empty second line serves the fixture table instead (see below). The
process prints `serving <path>` once the weights are mapped — about 15 s for the
17.6 GB Q4_K_XL file — and a file that is missing or not a `qwen35` GGUF ends
startup with `model error: …` and exit code 1; nothing is served from a model
that did not load. The runtime logs one structured line per request on stdout,
and the process stops the listener and exits cleanly when its lifetime ends.

That GGUF comes from `make -C llms qwen-pull`: it fetches the
pinned quant from `unsloth/Qwen3.8-27B-GGUF` into `~/.cache/osprey-llm/gguf`,
resuming a partial download and checking its SHA-256 against Hugging Face's LFS
metadata before the file is named `.gguf`. The baseline's other quants:
`QWEN_QUANT=UD-Q5_K_M make -C llms qwen-pull` (also
`QWEN_QUANT=UD-IQ3_XXS`).

## What it serves

| Endpoint | Method | Spec |
| --- | --- | --- |
| `/api/version` | GET | [LLM-API-OLLAMA] |
| `/api/tags` | GET | [LLM-API-OLLAMA] |
| `/api/show` | POST | [LLM-API-OLLAMA] — capabilities and context length |
| `/api/ps` | GET | [LLM-API-OLLAMA] |
| `/api/chat` | POST | [LLM-API-OLLAMA] — NDJSON streaming, `tool_calls` with JSON-object arguments |
| `/api/generate` | POST | [LLM-API-OLLAMA] — NDJSON streaming, the raw completion text |
| `/v1/models` | GET | [LLM-API-OPENAI] |
| `/v1/chat/completions` | POST | [LLM-API-OPENAI] — SSE streaming, `tool_calls` with JSON-string arguments |

A model the server does not serve is a 404 naming it, a route under the wrong
method is a 405, and a body without a model name is a 400; every failure is the
`{"error": message}` shape both clients raise a typed exception for.

## The model behind it

The inference seam is the `Generator` effect (`src/Inference.py`): a tokenizer
step each way, the model's end-of-turn token, and one sampling step that answers
with the token and the handle to continue from. A route never names a weight
file, a device or a vocabulary; `Server.py` chooses the handler at startup and
hands its handle to every request.

With a GGUF path the handler is `src/RealModel.py`: `Qwen3_5Gguf` (in
`../model`) maps every tensor of the file through `gpuMapFile`, quantized blocks
decoded by `gpuMatMulQuant` on the device, the tokenizer is built from the
file's own `tokenizer.ggml.*` tables, and the end-of-turn id is the one the
file declares (all three through `Qwen3_5Vocab` in `../model`). The handle is a
`Session`: the model, the tokenizer, and a one-slot channel holding the cache's
`State`: the tokens the KV and recurrent cache has seen, the logits after the
last of them, and the last four prompt checkpoints behind it. A step takes the
state out of the slot, feeds only the tokens the history adds past what the
cache holds, and puts it back, so a turn costs one forward pass per token (the
prompt is still fed one token at a time) and the next request starts where this
one ended; the slot is the one thing the server mutates, and a request waits on
it for the request before. A history that stops extending the live cache (an
agent that rewrote its last turn, or one that carries the answer without its
reasoning) continues from the newest checkpoint it still begins with, and only
from a fresh cache when none matches; the log says `cache: continued from N of
M tokens` for every prompt, so reuse is visible
(`2026-11-07_095000-server-real-lane-checkpoints.txt`: the second and the
rewritten third turn both continue from the first prompt's 14 tokens). The
renderer trims message content so an answer beginning with newlines does not
merge with the `assistant` prefix's newline into one token, which is what had
kept a history from matching its own prompt. Both handlers sample through
`src/Sampler.py` — `argmax` at temperature 0, else a seeded draw over the warped
softmax — so the same request options mean the same thing.

Receipt, 2026-11-07 (`target/local-coding-model/runs/2026-11-07_080714-server-real-weights.txt`,
Apple M4 Max, Metal, ARC): ready 15 s after launch; `/api/generate`,
`/api/chat` and `/v1/chat/completions` each answered from the real weights, the
OpenAI turn ending `</think>\n\npong` with `finish_reason` `stop` after 37
tokens. The model thinks before it answers; since later that day every chat
turn runs through the tool parser whether or not tools are advertised, so that
reasoning comes back in Ollama's `thinking` field and OpenAI's
`reasoning_content`, apart from the text, and a turn `num_predict` cuts off
inside its reasoning reports `length` with the reasoning so far rather than an
error. The real lane's four cases cover both
(`2026-11-07_091500-server-real-lane-thinking.txt`).

With an empty path the handler is `src/TableModel.py`, the CI stand-in: a real
model that is tiny — a twelve-token context whose successor counts come from
`CORPUS`, a transcript in the served chat framing. Its turns are the model's
own continuation of the prompt — `tests/model_reference.cpython` recomputes
them independently from the same corpus, and renders the prompt through
`tests/chat_template.jinja` so every byte the suite asserts is the template's —
never a string the server was handed.

A `/api/chat` turn runs its text through plan 0049 Phase 6's streaming tool
parser (`qwen.ToolParser` in `../model`): free text becomes
`content`, reasoning becomes `thinking`, and a call becomes `tool_calls` with
its arguments coerced by the advertised tool's JSON Schema. A call whose
arguments fail that schema is a 400 naming the violation, never a harness
fabrication ([LLM-TOOL-REPAIR]). `/api/generate` has no tools, so it returns its
text verbatim, as Ollama does.

The OpenAI surface answers the same turn: `/v1/models` lists the served model and
`/v1/chat/completions` answers whole or as `data:` events ending in `[DONE]`,
with `tool_calls` arguments as the JSON *string* OpenAI clients parse, and a
failure in OpenAI's `{"error": {"message": …}}` shape.

`/api/tags`, `/api/show` and `/api/ps` declare the pinned target model from plan
0049 — `Qwen3.8-27B` `UD-Q4_K_XL` at 64K context — and report no `digest`:
they describe the pin, not the file that was opened, under either handler.

## One generation at a time

The runtime's listener serves one accepted connection at a time, on the fiber
that started `httpListen`, so a second request waits in the accept backlog until
the first turn has been written: generation is serial and arrival-ordered
([LLM-API-QUEUE]). Each turn is bounded by `num_predict` and by the room left in
`num_ctx`, so a serving turn always ends and hands the listener on.

A client that disconnects mid-turn is *not* cancelled. The handler receives the
method, URL, headers and body and never the peer socket, so it cannot observe a
disconnect, and the runtime writes the finished response to a socket nobody
reads. The work is bounded rather than abandoned, and the listener stays
healthy: `tests/clients.cpython` abandons a connection mid-request and then has
the next turn answered normally. Detecting the peer is a missing HTTP ability
([HTTP-SERVER] has no per-request socket), reported to the plan's lead.

## A prompt the context cannot hold

A prompt whose token count exceeds the request's `num_ctx` is a 400 whose
message names both numbers — the prompt's token count and the context — and a
prompt that exactly fills `num_ctx` is the same error, because there is no room
to answer it ([LLM-API-LIMIT]). The server never truncates a prompt: a harness
that lost its instructions would answer wrongly with nothing to warn it.

## Limits of the served model

The fixture tokenizer's vocabulary is the characters of `CORPUS`, so under the
table a prompt with any other byte is a 400 naming that byte's offset; the real
handler tokenizes with the GGUF's vocabulary. The prompt is the GGUF's own
`tokenizer.chat_template`, rendered as `src/Prompts.py` writes it and checked
byte for byte against `tests/chat_template.jinja` (the GGUF's metadata, kept
under the model's Apache-2.0 notice): the `# Tools` system turn with each tool's
JSON, an assistant turn's `<tool_call>` blocks and a `tool` message's
`<tool_response>` are the template's, and what the server leaves to the model is
the `<think>` block of a history turn (rendered with `preserve_thinking=false`)
and the generation prompt's trailing `<think>`. `CORPUS` therefore carries the
template's tools turn, since the fixture vocabulary is the transcript's. Two
protocols' timestamps are one fixed instant because
Osprey has no clock builtin. Nothing here streams over the socket as it is
produced: the runtime writes one response body, so streaming means one NDJSON
line per token in that body, which both clients parse identically.

## Tests

The suite drives the compiled server with the real `ollama` and `openai` Python
packages over real sockets: every assertion reads the client's own parsed
objects, so a wire shape is only accepted when the client accepts it.

```sh
~/.cache/osprey-llm/oracle-venv/bin/python -I \
    src/osprey/examples/llms/qwen/server/tests/clients.cpython
```

`tests/harness.cpython` compiles the server, waits for its listener, runs every
registered case and reports one `ok <lane>/<case>` line each;
`tests/model_reference.cpython` is the independent CPython model the table's
answers are compared with, and `tests/limit_cases.cpython` holds the cases a
prompt over the context is rejected by.

Three lanes: `gc` and `arc` drive the fixture table (CI runs these; no weights
are downloaded), and `real` drives the ARC binary over the GGUF named by
`OSPREY_QWEN_GGUF` on Metal, asserting that the weights answer ("Paris",
"pong", a second turn that continues the first). Without that variable, or with
a path that is not a file, each real case prints `skip real/<case>: <reason>`
rather than passing silently. Pass lane names as arguments to narrow the run.
