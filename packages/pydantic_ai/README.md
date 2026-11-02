# @osprey/pydantic_ai

An Osprey Python-flavor port of the [pydantic-ai](https://github.com/pydantic/pydantic-ai) 2.46.0 agent engine: enough of the library to run a tool-calling agent against any OpenAI-compatible chat API, and to cut an application such as NAP across from CPython.

| Module | CPython module | What is ported |
| --- | --- | --- |
| `Messages` | `pydantic_ai.messages` | `SystemPromptPart`, `UserPromptPart`, `ToolReturnPart`, `RetryPromptPart`, `InstructionPart`, `TextPart`, `ThinkingPart`, `ToolCallPart`, `ModelRequest`, `ModelResponse`, the stream events, `ModelResponse.text`, `args_as_dict`, `args_as_json_str`, `RetryPromptPart.model_response`, and `ModelMessagesTypeAdapter.dump_python`/`validate_python` as `dump_messages`/`validate_messages` |
| `Usage` | `pydantic_ai.usage` | `RequestUsage`, `RunUsage`, `incr`, `total_tokens` |
| `Settings` | `pydantic_ai.settings` | `ModelSettings` as a JSON map, `merge_model_settings` |
| `Profiles` | `pydantic_ai.profiles` | `ModelProfile` as a JSON map, `merge_profile`, `openai_model_profile`, DeepSeek and Ollama profiles |
| `Tools` | `pydantic_ai.tools` | `ToolDefinition`, `Tool` |
| `Models` | `pydantic_ai.models` | `ModelRequestParameters`, `Model` |
| `Providers` | `pydantic_ai.providers` | the `Provider` record and `scripted` for tests |
| `ProvidersHttp` | `pydantic_ai.providers.*` | `OpenAIProvider`, `OllamaProvider`, `DeepSeekProvider` over the runtime HTTPS client |
| `ModelsOpenai` | `pydantic_ai.models.openai` | `OpenAIChatModel`: message, tool and settings mapping to a Chat Completions request, response parsing, usage, finish reasons, thinking tags and fields |
| `JsonSchema` | `pydantic_ai._json_schema`, `profiles.openai` | the OpenAI strict-mode schema transformer |
| `ModelsTest` | `pydantic_ai.models.test` | `TestModel` with its schema-driven argument generator and usage estimate |
| `Direct` | `pydantic_ai.direct` | `model_request`, `model_request_stream` (events replayed from the complete response) |
| `Agent` | `pydantic_ai.agent` | `Agent`, `tool_plain`, `run_sync`: the tool-calling loop with `ModelRetry` as a tool's `Error`, the request limit, `AgentRunResult` |
| `Exceptions` | `pydantic_ai.exceptions` | `ModelHTTPError`, `ModelAPIError`, `UserError`, `UnexpectedModelBehavior`, `UsageLimitExceeded` as the `ModelError` union, carried by `Outcome[T]` |

## What differs from CPython

- Osprey has no exceptions, optional fields or runtime introspection. A model request or agent run answers `Outcome[T]`: `Ok(value=)` or `Failed(error=)` with a `ModelError` variant. Optional strings are empty (`instructions`, `finish_reason`, `tool_name` of a plain retry, `model_name`), and `ModelMessage` wraps the two records as `Request(request=)` / `Response(response=)`.
- A `Tool` spells its name, description and parameter schema beside its function, which takes the arguments as a JSON object and answers `Result[str, str]`; CPython derives those from the function signature and `ModelRetry`.
- `ModelSettings` and `ModelProfile` are JSON maps keyed by the CPython names. `ToolDefinition.strict` is `False` for CPython's `None` (inferred by the schema transformer).
- The runtime has no streaming bodies, so `model_request_stream` performs the request and replays the parts as start and end events. Files, structured output, output tools, native tools, the Responses API, instrumentation, dependencies and retries of validation errors are not ported.
- Tool call ids are the provider's; `TestModel` uses `pyd_ai_tool_call_id__<name>` as CPython does.

## Tests

The [Python-flavor ports contract](../../docs/specs/ecosystem/0043-PythonFlavorPorts.md) defines [PORT-SUITE] and [PORT-TEST-ORACLE]. `tests/main.py` compares the port with values the real pydantic-ai computed over an httpx mock transport in `scripts/oracles/pydantic_ai.py`: request bodies, parsed replies, the wire form, `TestModel` runs, a scripted multi-step tool-calling run and the error texts. `src/` is Osprey source, not CPython; run it with the Osprey compiler.

```sh
target/release/osprey packages/pydantic_ai/tests --run
target/release/osprey packages/pydantic_ai/tests --run --memory=gc
OSPREY_ARC_DEBUG=1 target/release/osprey packages/pydantic_ai/tests --run --memory=arc
```

`live/` is the real thing: an agent with two tools against DeepSeek over HTTPS, its answer judged by a second model call. It reads `DEEPSEEKKEY` from `.env` in the working directory, spends tokens and is not part of the offline matrix:

```sh
target/release/osprey packages/pydantic_ai/live --run
```

The Rust integration harness discovers `tests/osprey.toml`, compares stdout with `tests/expected_output.txt` under all three memory backends, and requires ARC to exit without live objects. Presence of a suite does not establish that it passes; compiler diagnostics, assertion failures and missing golden files remain failures.
