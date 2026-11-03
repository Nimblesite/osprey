# Plan 0051 — MCP servers in Osprey Agent

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
| Tool names | `mcp__<server>__<tool>`, each joining the engine's tool list | No clash with `run_command`, and the transcript can name the server |
| Approval | None, the same as the built-in tools; the "Not sandboxed" note stays true | An approval step is its own feature |
| Secrets | `env` and `headers` values never reach `chats.json`, the records or the transcript | Same rule the API key follows, same self-test check |

## Phases

- [ ] **Protocol.** `mcp.py`: `initialize`, `tools/list` with pagination, `tools/call`, error and `isError` results, text and non-text content. Tests in `packages/pydantic_ai/tests`. Licence: add the upstream module and the MCP specification to `references` in `packages/license-audit.json`, with a recorded comparison in `source-audit.json`.
- [ ] **Process capabilities.** The four capabilities in `packages/native/src/capability.ospml` and README, `ProcessAdapter.swift` on the SwiftUI host, with every process stopped on quit; hosts without them complete with a typed error.
- [ ] **Config.** Read and merge both files, with plain messages for a broken file, a missing command, or an unsupported entry. Host-free tests, with many permutations.
- [ ] **Connect and call.** Connect on launch and on folder change; list tools; dispatch `mcp__…` calls through `Turn`; restart a server that exited, once a person asks. A slow or dead server fails that call and never blocks the turn or other servers.
- [ ] **Window.** An inspector section: each server's state, its tool count, its problem in words, a switch for each server, and buttons to open the config file and reload. Tool steps show the server's name.
- [ ] **Proof.** An in-repo test MCP server, python standard library only, on both transports. `self-test/mcp.json` runs the doubled model through config, connect, a tool call, a tool error, a dead server, a project server left off and then turned on, a secret kept out of the records, goldens per OS. `self-test.sh --live` gives DeepSeek a task that it can only solve through the server, with acceptance checks it never sees.
- [ ] **Docs.** The README section "MCP servers", with a pasteable example.

## Out of scope

MCP resources, prompts, sampling, OAuth login flows and any tool approval step: each gets its own plan if wanted.
