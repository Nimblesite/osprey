# Osprey Agent

**Status:** none of this application contract is implemented yet. [`examples/agent/src/engine.py`](../../../examples/agent/src/engine.py) answers every model with DeepSeek's endpoint and profile, `model.ospml` holds no effort and seeds one key from two `.env` names alone, `view.ospml` has no Settings pane, and the SwiftUI host selects one `Text` at a time. [Plan 0052](../../plans/0052-agent.md) delivers it over contract that already exists: the process capabilities [UI-EFFECTS-PROCESS] of [`@osprey/native`](../../../packages/native/README.md) and the thinking levels of [`@osprey/pydantic-ai`](../../../packages/pydantic_ai/README.md).

The Osprey Agent is the coding agent application at [`examples/agent/`](../../../examples/agent/README.md): one store, one `update` and one set of regions over `@osprey/native`, with model requests and tools driven through capabilities.

## Providers, models and effort [AGENT-PROVIDERS]

Not implemented yet: `Engine.url` returns DeepSeek's Chat Completions endpoint for every model, `Engine.profile_of` chooses DeepSeek's or OpenAI's profile, `chatModel` holds a bare model name and there is no `chatEffort`.

The app's catalogue names DeepSeek, Kimi and GLM entries. The coding-plan entries reuse the upstream provider's profile with another base URL: `https://api.kimi.com/coding/v1` and `https://api.z.ai/api/coding/paas/v4`.

`chatModel` holds `provider/model`. A value with no `/` reads as DeepSeek, which is what saved conversations hold, so a saved conversation with no `/` restores as DeepSeek.

Each conversation carries its own `chatEffort`, whose choices come from the model's profile (`Profiles.thinking_levels`). A model whose profile returns none has no effort control. `chatEffort` round-trips through `chatJson`/`chatOf`.

## Settings and keys [AGENT-SETTINGS]

Not implemented yet: the composer asks for one DeepSeek key, `Model::keyIn` seeds it from `.env` names `DEEPSEEKKEY` and `DEEPSEEK_API_KEY` alone, and the app writes no key file — only `chats.json`, which never holds the key.

A Settings pane replaces the conversation column, opened from the sidebar foot, with Providers and MCP servers tabs.

Provider keys are written to `providers.json` in the app's private store with mode 0600 — never `chats.json`, never the records. An empty key slot is still seeded from `.env` names `DEEPSEEKKEY`, `DEEPSEEK_API_KEY`, `MOONSHOT_API_KEY`, `KIMI_API_KEY`, `ZAI_API_KEY`, `GLM_API_KEY`.

## MCP servers [AGENT-MCP]

Not implemented yet: the app reads no `mcp.json` or `.mcp.json`, connects to no server and lists no MCP tools. The sans-IO client (`packages/pydantic_ai/src/mcp.py`) and the stdio and streamable-HTTP test server (`examples/agent/self-test/mcp_server.py`) exist unused by it.

`mcp.json` in the app's support folder configures servers for every project; `.mcp.json` in the project folder configures servers for that one project. The app reads and merges both, and the project entry wins on a name clash. Servers connect on launch and folder change.

The MCP tab of the Settings pane adds, removes and toggles servers, writing `mcp.json` in the private store. `.mcp.json` servers are listed but off until the person turns them on, remembered per folder.

Servers speak stdio and streamable HTTP. Each tool joins the engine's tool list named `<server>_<tool>`. A tool call asks for no approval: none, the same as the built-in tools.

MCP `env` and `headers` values never reach `chats.json`, the records or the transcript.

## Selection [AGENT-SELECTION]

Not implemented yet: the SwiftUI host enables `.textSelection` per `Text` (`hosts/swift/Sources/OspreyNative/Leaves.swift`), so selection stops at a block.

The transcript renders as one selectable group of AppKit text views in the SwiftUI host, so a selection crosses transcript blocks, with Cmd-A and Cmd-C.
