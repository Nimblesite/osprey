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

A model the server does not serve is a 404 naming it, a route under the wrong
method is a 405, and a body without a model name is a 400; every failure is the
`{"error": message}` shape both clients raise a typed exception for.

## What it does not serve yet

`/api/chat`, `/api/generate`, `/v1/models` and `/v1/chat/completions` are not
routed yet, so the server answers no generation request and installs no
inference handler. They arrive next, behind the `Generator` effect, with a real
fixture model sampled through `@osprey/transformers`' `generate`.

`/api/tags`, `/api/show` and `/api/ps` declare the pinned target model from plan
0049 — `Qwen3.8-27B` `UD-Q4_K_XL` at 64K context — and report no `digest`,
because no weights file has been read.

## Tests

The suite drives the compiled server with the real `ollama` and `openai` Python
packages over real sockets: every assertion reads the client's own parsed
objects, so a wire shape is only accepted when the client accepts it.

```sh
~/.cache/osprey-llm/oracle-venv/bin/python -I \
    src/osprey/examples/llms/qwen/server/tests/clients.cpython
```

Pass `gc` or `arc` as arguments to narrow the run; it builds and drives both
managed backends by default.
