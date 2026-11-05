# Qwen server — the Ollama and OpenAI protocols on Osprey

Osprey serves the local coding model itself ([LLM-SERVING]): a harness that
speaks the Ollama or OpenAI protocol points here instead of at a real Ollama,
which cannot host this port — `ollama create` runs a GGUF on Ollama's own
engines, so no Osprey code would execute. The listener is
`127.0.0.1:11435`, so it runs beside a real Ollama on 11434, which the baseline
and the evaluation need.

## Run it

```sh
target/debug/osprey src/osprey/examples/llms/qwen/server --compile -o target/llms/qwen-server/server
printf '11435\n' | target/llms/qwen-server/server      # one line of stdin names the port
```

`printf '\n'` — or a closed stdin — serves the default port 11435. The runtime
logs one structured line per request on stdout; the process stops the listener
and exits cleanly when its lifetime ends.

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
step each way, the model's end-of-turn token, and one sampling step. A route
never names a weight file, a device or a vocabulary, so plan 0049 Phase 3
replaces the handler and no route changes.

The handler installed today is `src/TableModel.py`, a real model that is tiny: a
twelve-token context whose successor counts come from `CORPUS`, a transcript in
the served chat framing, sampled through `@osprey/transformers`' `generate`. Its
turns are the model's own continuation of the prompt — `tests/model_reference.cpython`
recomputes them independently from the same corpus, and the suite compares the
two — never a string the server was handed.

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
0049 — `Qwen3.8-27B` `UD-Q4_K_XL` at 64K context — and report no `digest`,
because no weights file has been read.

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

The fixture tokenizer's vocabulary is the characters of `CORPUS`, so a prompt
with any other byte is a 400 naming that byte's offset; the real Qwen tokenizer
arrives with [LLM-TOKENIZER]. Two protocols' timestamps are one fixed instant
because Osprey has no clock builtin. Nothing here streams over the socket as it
is produced: the runtime writes one response body, so streaming means one NDJSON
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
registered case and reports one `ok <backend>/<case>` line each;
`tests/model_reference.cpython` is the independent CPython model the answers are
compared with.

Pass `gc` or `arc` as arguments to narrow the run; it builds and drives both
managed backends by default.
