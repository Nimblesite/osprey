# Plan 0052 — Osprey Agent: providers, models, effort, a settings screen, and code that looks like code

Specs: [UI-EFFECTS], [UI-STORE], [UI-MACHINE-SELF-TEST]. Follows from: plan [0051](0052-agent.md), whose MCP work surfaces in this plan's settings screen.

## Goal

Osprey Agent only speaks DeepSeek, keeps its key in memory, draws fenced code in one colour and selects one line at a time. After this plan a person opens Settings, configures a provider (DeepSeek, Kimi or GLM, each with its API and coding plan endpoints), adds and removes MCP servers, picks a model and its effort in the composer, reads highlighted code and selects any run of the transcript. Every claim has an automated test: host-free checks, doubled window runs with goldens, and a `--live` run when a provider's key is present.

## Decisions

| Question | Decision | Why |
| --- | --- | --- |
| Package surface | `MoonshotAIProvider`, `ZaiProvider`, `moonshotai_model_profile`, `zai_model_profile` and the unified `thinking` setting in `packages/pydantic_ai`, under upstream's names | They exist upstream in 2.46.0; the app stays a thin caller |
| Coding plan endpoints | App catalogue entries that reuse the upstream provider's profile with another base URL: `https://api.kimi.com/coding/v1`, `https://api.z.ai/api/coding/paas/v4` | Upstream has no such providers, and coding plan keys only work there |
| Model identity | `provider/model` in `chatModel`; a value with no `/` is DeepSeek, which is what saved conversations hold | One field, one rule |
| Effort | A per-conversation `chatEffort`, whose choices come from the model's profile. No control when the model has none. GLM-5.3 offers Low, High, Max; GLM-5.2 every level; older GLM on or off; Kimi reasoning models Minimal to High and never off | Each model's real, peculiar settings, taken from upstream's profiles |
| Settings | A pane replacing the conversation column, opened from the sidebar foot, with Providers and MCP servers tabs | A pane is drawn by the headless self-test, so it has goldens |
| Keys | `providers.json` in the app's private store, mode 0600, never `chats.json`, never records. `.env` names `DEEPSEEKKEY`, `DEEPSEEK_API_KEY`, `MOONSHOT_API_KEY`, `KIMI_API_KEY`, `ZAI_API_KEY`, `GLM_API_KEY` still seed an empty slot | Survives a restart; same secrecy check as today, extended to every provider |
| MCP add and remove | The MCP tab writes `mcp.json` in the private store; `.mcp.json` servers appear off until turned on (plan 0052) | The window is the normal way, the file stays pasteable |
| Highlighting | A pure table-driven scanner, `examples/agent/src/highlight.py`, drawn as `Ui::rich` runs | No dependency; host-free tests see every token |
| Selection | AppKit text views with one selection group per transcript | SwiftUI's selection stops at each `Text` |

## Phases

- [x] **Package.** Providers, profiles and `thinking` mapping, the MCP protocol module finished (`osprey.toml`, tests, README, audit). Golden totals rise.
- [ ] **Highlighting.** `highlight.py`, drawing, theme tokens with contrast of at least 4.5:1, tool output by file extension, `self-test/code-blocks.json`, goldens.
- [ ] **Selection.** Multi-line selection across blocks, Cmd-A and Cmd-C, proved by real events in a self-test.
- [ ] **Providers and settings.** Catalogue, settings pane, key persistence, composer's model and effort pickers, request shape per provider against a strict double, `self-test/settings.json`, goldens.
- [ ] **MCP in the app.** Process capabilities, config, connect and call, add, remove and toggle in Settings, inspector section, `self-test/mcp.json` with the stdlib test server on both transports (plan 0052).
- [ ] **Live proof.** `self-test.sh --live` picks whichever provider has a key, runs a real coding task and an MCP task with acceptance checks the model never sees.
- [x] **Credits.** The website's announcements and credits page.
- [ ] **Docs.** README sections for providers, effort, MCP.

## Out of scope

OAuth logins, fetching model lists from a provider, streaming replies, tool approval.

# Plan 0052 — MCP servers in Osprey Agent

Specs: [UI-EFFECTS] and [UI-MACHINE-SELF-TEST] in `packages/native/README.md`; the [Model Context Protocol specification](https://modelcontextprotocol.io/specification).
Follows from: the `@osprey/pydantic-ai` port and Osprey Agent in [`examples/agent`](../../examples/agent/README.md).

## Goal

Osprey Agent's model has four hardcoded tools in `examples/agent/src/engine.py`, and neither the app nor `packages/pydantic_ai` knows MCP. After this plan, a person configures MCP servers the way other agents do, sees each server's state in the window, and the model can call those servers' tools in a real conversation.

## Decisions

| Question | Decision | Why |
| --- | --- | --- |
| Config format | The common `{"mcpServers": {name: {command, args, env} or {url, headers}}}` | A person can paste the block every MCP server's README already gives |
| Config location | `mcp.json` in the app's support folder for every project; `.mcp.json` in the project folder for one | Same split as other agents; the project entry wins on a name clash |
| Project servers | Listed but off until the person turns them on, remembered per folder | A cloned repo's `.mcp.json` starts commands: changing folder must never run them unasked, whatever the agent may do once asked |
| Transports | stdio and streamable HTTP | HTTP fits the existing `Http` capability; most local servers are stdio |
| Long-lived processes | New `@osprey/native` capabilities `ProcessStart`, `ProcessWrite`, `ProcessRead`, `ProcessStop` | `Run` is one shot. Every request stays bounded: a read returns the output so far, within a limit and a deadline, as one completion |
| Protocol code | `packages/pydantic_ai/src/mcp.py`: JSON-RPC messages and results, sans-IO, following upstream `pydantic_ai.mcp` names | Testable without a process; the app owns the IO through capabilities |
| Tool names | `<server>_<tool>`, each joining the engine's tool list | What upstream's `PrefixedToolset` does; the transcript can name the server |
| Approval | None, the same as the built-in tools; the "Not sandboxed" note stays true | An approval step is its own feature |
| Secrets | `env` and `headers` values never reach `chats.json`, the records or the transcript | Same rule the API key follows, same self-test check |

## Phases

- [x] **Protocol.** `mcp.py`: `initialize`, `tools/list` with pagination, `tools/call`, error and `isError` results, text and non-text content. Tests in `packages/pydantic_ai/tests`. Licence: add the upstream module and the MCP specification to `references` in `packages/license-audit.json`, with a recorded comparison in `source-audit.json`.
- [ ] **Process capabilities.** The four capabilities in `packages/native/src/capability.ospml` and README, `ProcessAdapter.swift` on the SwiftUI host, with every process stopped on quit; hosts without them complete with a typed error.
- [ ] **Config.** Read and merge both files, with plain messages for a broken file, a missing command, or an unsupported entry. Host-free tests, with many permutations.
- [ ] **Connect and call.** Connect on launch and on folder change; list tools; dispatch server tool calls through `Turn`; restart a server that exited, once a person asks. A slow or dead server fails that call and never blocks the turn or other servers.
- [ ] **Window.** An inspector section: each server's state, its tool count, its problem in words, a switch for each server, and buttons to open the config file and reload. Tool steps show the server's name.
- [ ] **Proof.** An in-repo test MCP server, python standard library only, on both transports. `self-test/mcp.json` runs the doubled model through config, connect, a tool call, a tool error, a dead server, a project server left off and then turned on, a secret kept out of the records, goldens per OS. `self-test.sh --live` gives DeepSeek a task that it can only solve through the server, with acceptance checks it never sees.
- [ ] **Docs.** The README section "MCP servers", with a pasteable example.

## Out of scope

MCP resources, prompts, sampling, OAuth login flows and any tool approval step: each gets its own plan if wanted.
