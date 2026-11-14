# Plan 0052 — Osprey Agent

Contract: [OspreyAgent](../specs/applications/OspreyAgent.md); [UI-EFFECTS], [UI-STORE], [UI-EFFECTS-PROCESS] in [src/osprey/packages/native/README.md](../../src/osprey/packages/native/README.md); [UI-MACHINE-SELF-TEST] in [UiMachines](../specs/platforms/UiMachines.md); [Model Context Protocol specification](https://modelcontextprotocol.io/specification).

Out of scope: OAuth logins, fetching model lists, streaming replies, tool approval; MCP resources, prompts, sampling, elicitation.

## Prerequisites in place

- `@osprey/pydantic-ai` ships the Kimi/GLM providers, profiles and unified `thinking` (`src/osprey/packages/pydantic_ai/src/profiles.py`, `providers_http.py`) and the sans-IO MCP client `src/osprey/packages/pydantic_ai/src/mcp.py`, with tests and licence references.
- `src/osprey/examples/agent/src/highlight.py` and its `Ui::rich` drawing in `src/osprey/examples/agent/src/view.ospml` are done.
- Framework process capabilities `ProcessStart/Write/Read/Stop` are declared in `src/osprey/packages/native/src/capability.ospml`, specified as [UI-EFFECTS-PROCESS] in `src/osprey/packages/native/README.md`, and implemented in the SwiftUI host's `ProcessAdapter.swift` with real-process tests, stopped on quit.
- `src/osprey/examples/agent/self-test/mcp_server.py` (stdlib, stdio + streamable HTTP) exists.

## Selection

- [x] [UI-MACHINE-SELF-TEST] Render the transcript as one selectable group of AppKit text views in the SwiftUI host, so selection crosses blocks (today `.textSelection` is per `Text` in `src/toolchain/hosts/swift/Sources/OspreyNative/Leaves.swift`); add Cmd-A and Cmd-C. Acceptance: a new self-test drives real key events to select a run spanning transcript blocks and copies it, asserted in the records.

## Providers, models and effort

- [x] Replace DeepSeek-only `DEEPSEEK_URL`, `url` and `profile_of` in `src/osprey/examples/agent/src/engine.py` with a catalogue of DeepSeek, Kimi and GLM entries; the coding-plan entries reuse the provider's profile at base URLs `https://api.kimi.com/coding/v1` and `https://api.z.ai/api/coding/paas/v4`. Acceptance: host-free tests that `Engine.url`/`Engine.profile_of` return the right endpoint and profile per entry.
- [x] `src/osprey/examples/agent/src/model.ospml`: `chatModel` becomes `provider/model` (a value with no `/` reads as DeepSeek), and add a per-conversation `chatEffort` whose choices come from `Profiles.thinking_levels` (no picker when it returns none). Acceptance: a saved conversation with no `/` restores as DeepSeek, and `chatEffort` round-trips through `chatJson`/`chatOf`.

## Settings and keys

- [x] [AGENT-SETTINGS] A `Settings` button always visible at the sidebar's foot, replacing the `API key set` badge. Acceptance: `self-test/settings.json` presses it from a state with a key and from one without.
- [x] [AGENT-COMPOSER] The composer's model picker lists every provider with a key, with an `Effort` picker per model and `Add an API key` with none. Acceptance: test-app and `self-test/settings.json`.
- [x] [AGENT-SETTINGS] Refused requests say what to do, with `Open Settings`. Acceptance: test-app covers 401, 402, 403, 429.
- [x] [AGENT-BLANK-REPLY] A turn that ends with no text says so and leaves no gap. Acceptance: a failing test-app case first.

- [x] `src/osprey/examples/agent/src/view.ospml` + `model.ospml`: a Settings pane replacing the conversation column, opened from the sidebar foot, with Providers and MCP servers tabs; the composer's model picker gains the provider catalogue and an effort picker. Acceptance: `self-test/settings.json` drives provider + model + effort and matches goldens.
- [x] Key persistence in `src/osprey/examples/agent/src/app.ospml` + `model.ospml`: write `providers.json` in the app's private store (mode 0600), never `chats.json`, never records; seed an empty slot from `.env` names `DEEPSEEKKEY`, `DEEPSEEK_API_KEY`, `MOONSHOT_API_KEY`, `KIMI_API_KEY`, `ZAI_API_KEY`, `GLM_API_KEY`. Acceptance: a restart self-test shows a saved provider key and `grep -rq` finds it in neither `chats.json` nor the records.

## MCP in the app

- [x] `src/osprey/examples/agent/src/app.ospml` + `engine.py`: read and merge `mcp.json` (private store) and `.mcp.json` (project; the project entry wins on a name clash), connect on launch and folder change, list tools, dispatch `<server>_<tool>` calls through `Turn`, restart an exited server once a person asks; a slow or dead server fails only its call, never the turn or other servers; project servers stay off until turned on. Acceptance: `self-test/mcp.json` doubled run covers connect, a tool call, a tool error, a dead server, and a project server off then on.
- [x] Secrecy: MCP `env` and `headers` values never reach `chats.json`, the records or the transcript. Acceptance: extend `self-test.sh`'s existing `grep -rq` key check to a planted MCP secret.
- [x] `src/osprey/examples/agent/src/view.ospml`: the MCP tab adds, removes and toggles servers, with each server's state, tool count, its problem in words, a switch and Reconnect; an inspector section lists each enabled server and opens Settings; tool steps show the server's name.

## Self-tests and goldens

- [x] `src/osprey/examples/agent/test/main.py` `prov.*`: every catalogue model and every level: URL, exact body members, no `strict` where forbidden, `reasoning_effort` or `thinking` per the package, `reasoning_content` read and sent back, old `deepseek-chat` resolves, an unknown provider fails in words.
- [x] `src/osprey/examples/agent/test-app/main.ospml`: open and close settings; per provider save, restart, remove a key; `.env` fills only empty slots; provider then model then effort in Settings, inherited by a new chat; a composer pick changes only that chat; effort kept or cleared; sending blocked without a key; each failure status. MCP: config outcomes, add, remove, clash, toggle, project server off and remembered per folder, a dead server, tools gone after remove, secrets absent from every saved document.
- [x] Strict HTTP double (`requireBearer`, `requireUrl`) with Swift tests, and `self-test/verify_providers.py`, `self-test/verify_mcp.py`.
- [x] Read every golden and judge the look. Remove any control that does nothing.

- [x] `src/osprey/examples/agent/self-test/code-blocks.json` + goldens: a doubled run draws fenced code and tool output through `Highlight.tokens_of`; the `Ui::rich` runs are asserted, and every ink code is drawn with (the two frame inks and the ten highlight kinds) meets 4.5:1 on `codeFill` in light and dark via `Contrast::ratio`, in the fenced-block test of `src/osprey/examples/agent/test-app/main.ospml` (`readsOnCode`).
- [x] `src/osprey/examples/agent/self-test/settings.json` + goldens: configure each provider, pick a model and effort, and check the per-provider request shape against a strict double.
- [x] `src/osprey/examples/agent/self-test/mcp.json` + goldens per OS: run the doubled model through config, connect, a tool call, a tool error, a dead server, a project server off then on, and a secret kept out of the records, using `self-test/mcp_server.py` on stdio and streamable HTTP.

## Live proof

- [ ] `src/osprey/examples/agent/self-test.sh --live [dotenv] [provider/model]`: on the named model (script ready; waiting on a provider key with credit in `.env`), run a real coding task and an MCP task, each with acceptance checks the model never sees (today it is DeepSeek-only, `coding.json` + `tasks/`).

## Docs

- [x] `src/osprey/examples/agent/README.md`: sections for providers, keys, effort, and MCP servers ("How do I configure MCPs in this app?"), with a pasteable MCP config example.
