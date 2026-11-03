# Plan 0052 — Osprey Agent

Contract: [UI-EFFECTS], [UI-STORE], [UI-EFFECTS-PROCESS] in [packages/native/README.md](../../packages/native/README.md); [UI-MACHINE-SELF-TEST] in [UiMachines](../specs/platforms/UiMachines.md); [Model Context Protocol specification](https://modelcontextprotocol.io/specification).

Out of scope: OAuth logins, fetching model lists, streaming replies, tool approval; MCP resources, prompts, sampling, elicitation.

## Prerequisites in place

- `@osprey/pydantic-ai` ships the Kimi/GLM providers, profiles and unified `thinking` (`packages/pydantic_ai/src/profiles.py`, `providers_http.py`) and the sans-IO MCP client `packages/pydantic_ai/src/mcp.py`, with tests and licence references.
- `examples/agent/src/highlight.py` and its `Ui::rich` drawing in `examples/agent/src/view.ospml` are done.
- Framework process capabilities `ProcessStart/Write/Read/Stop` are declared in `packages/native/src/capability.ospml` and specified as [UI-EFFECTS-PROCESS] in `packages/native/README.md`, but the SwiftUI host has no adapter for them.
- `examples/agent/self-test/mcp_server.py` (stdlib, stdio + streamable HTTP) exists.

## Selection

- [ ] [UI-MACHINE-SELF-TEST] Render the transcript as one selectable group of AppKit text views in the SwiftUI host, so selection crosses blocks (today `.textSelection` is per `Text` in `hosts/swift/Sources/OspreyNative/Leaves.swift`); add Cmd-A and Cmd-C. Acceptance: a new self-test drives real key events to select a run spanning transcript blocks and copies it, asserted in the records.

## Providers, models and effort

- [ ] Replace DeepSeek-only `DEEPSEEK_URL`, `url` and `profile_of` in `examples/agent/src/engine.py` with a catalogue of DeepSeek, Kimi and GLM entries; the coding-plan entries reuse the provider's profile at base URLs `https://api.kimi.com/coding/v1` and `https://api.z.ai/api/coding/paas/v4`. Acceptance: host-free tests that `Engine.url`/`Engine.profile_of` return the right endpoint and profile per entry.
- [ ] `examples/agent/src/model.ospml`: `chatModel` becomes `provider/model` (a value with no `/` reads as DeepSeek), and add a per-conversation `chatEffort` whose choices come from `Profiles.thinking_levels` (no picker when it returns none). Acceptance: a saved conversation with no `/` restores as DeepSeek, and `chatEffort` round-trips through `chatJson`/`chatOf`.

## Settings and keys

- [ ] `examples/agent/src/view.ospml` + `model.ospml`: a Settings pane replacing the conversation column, opened from the sidebar foot, with Providers and MCP servers tabs; the composer's model picker gains the provider catalogue and an effort picker. Acceptance: `self-test/settings.json` drives provider + model + effort and matches goldens.
- [ ] Key persistence in `examples/agent/src/app.ospml` + `model.ospml`: write `providers.json` in the app's private store (mode 0600), never `chats.json`, never records; seed an empty slot from `.env` names `DEEPSEEKKEY`, `DEEPSEEK_API_KEY`, `MOONSHOT_API_KEY`, `KIMI_API_KEY`, `ZAI_API_KEY`, `GLM_API_KEY`. Acceptance: a restart self-test shows a saved provider key and `grep -rq` finds it in neither `chats.json` nor the records.

## MCP in the app

- [ ] Implement the four process capabilities [UI-EFFECTS-PROCESS] in the SwiftUI host: an adapter for `process-start/process-write/process-read/process-stop` (`hosts/swift/Sources/OspreyNative/ProcessAdapter.swift` currently only `run`), registered in `Adapters.standard` (`hosts/swift/Sources/OspreyNative/RegionStore.swift`), with every process stopped on quit. Acceptance: host tests for the four kinds and stop-on-quit; an unregistered kind still completes with the typed unknown-kind error.
- [ ] `examples/agent/src/app.ospml` + `engine.py`: read and merge `mcp.json` (private store) and `.mcp.json` (project; the project entry wins on a name clash), connect on launch and folder change, list tools, dispatch `<server>_<tool>` calls through `Turn`, restart an exited server once a person asks; a slow or dead server fails only its call, never the turn or other servers; project servers stay off until turned on. Acceptance: `self-test/mcp.json` doubled run covers connect, a tool call, a tool error, a dead server, and a project server off then on.
- [ ] Secrecy: MCP `env` and `headers` values never reach `chats.json`, the records or the transcript. Acceptance: extend `self-test.sh`'s existing `grep -rq` key check to a planted MCP secret.
- [ ] `examples/agent/src/view.ospml`: the MCP tab adds, removes and toggles servers; an inspector section shows each server's state, tool count, its problem in words, a switch, and buttons to open the config file and reload; tool steps show the server's name.

## Self-tests and goldens

- [ ] `examples/agent/self-test/code-blocks.json` + goldens: a doubled run draws fenced code and tool output through `Highlight.tokens_of`; assert the `Ui::rich` runs and that every code token pair meets 4.5:1 via `packages/native` `Contrast::ratio` (`packages/native/src/theme.ospml`).
- [ ] `examples/agent/self-test/settings.json` + goldens: configure each provider, pick a model and effort, and check the per-provider request shape against a strict double.
- [ ] `examples/agent/self-test/mcp.json` + goldens per OS: run the doubled model through config, connect, a tool call, a tool error, a dead server, a project server off then on, and a secret kept out of the records, using `self-test/mcp_server.py` on stdio and streamable HTTP.

## Live proof

- [ ] `examples/agent/self-test.sh --live`: pick whichever provider has a key, run a real coding task and an MCP task, each with acceptance checks the model never sees (today it is DeepSeek-only, `coding.json` + `tasks/`).

## Docs

- [ ] `examples/agent/README.md`: sections for providers, effort, and MCP servers, with a pasteable MCP config example.
