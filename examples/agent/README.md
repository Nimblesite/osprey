# Osprey Agent

A coding agent as a desktop application, written in Osprey on [`@osprey/native`](../../packages/native/README.md) and powered by the [`@osprey/pydantic-ai`](../../packages/pydantic_ai/README.md) engine. Give it a project folder and a task. It reads, edits and runs commands through tools, and shows its work as it goes.

```sh
examples/agent/run.sh /path/to/project          # uses the keys in the repository's .env
examples/agent/run.sh /path/to/project key.env  # or a dotenv file you name
examples/agent/run.sh --test                    # the suite: no network, no window
```

![Osprey Agent fixing a project and summing up its work](screenshot.png)

Without a key for the conversation's provider, the send button becomes `Add an API key` and the composer says what is missing. A failed request never locks anything: every send asks the provider again. The key stays in memory and in the app's private support folder, and is never written to the conversations file. Conversations are saved after every turn, one write at a time, and a failed save is shown in the conversation.

## Providers, keys and effort

Open `Settings` at the foot of the sidebar. Under `Providers`, paste a key and press save; it is kept in `providers.json` in the app's private support folder, readable only by you. Pick the default model and effort there. A new conversation starts on them, and the composer's pickers change one conversation only.

| Provider | Key in a dotenv file |
| --- | --- |
| DeepSeek | `DEEPSEEKKEY` or `DEEPSEEK_API_KEY` |
| Kimi (Moonshot API) | `MOONSHOT_API_KEY` |
| Kimi Code plan | `KIMI_API_KEY` |
| GLM (Z.ai API) | `ZAI_API_KEY` or `GLM_API_KEY` |
| GLM Coding Plan | `ZAI_CODING_API_KEY` |

`run.sh` copies the dotenv file to the support folder as `key.env`, which also serves a launch from the Dock. A key saved in Settings wins over the file. `Effort` lists the levels the model has, and is absent when it has none.

## MCP servers

How do I configure MCPs in this app? Open `Settings`, then `MCP servers`. `Add server` takes a name, a command with one argument per line or a URL with `Name=value` headers, and environment lines. `Paste JSON` takes the standard block:

```json
{
  "mcpServers": {
    "files": {"command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", "/path/to/folder"]},
    "docs": {"url": "https://example.com/mcp", "headers": {"Authorization": "Bearer your-token"}}
  }
}
```

Those servers are saved in `mcp.json` in the private support folder and serve every project; `Remove` stops one and deletes it. A project can also ship a `.mcp.json` in the same shape: its servers are listed `Off` and run nothing until you switch them on, which is remembered per folder, and a project entry wins a name clash. A URL is HTTPS, or HTTP on this machine.

The model sees each tool as `<server>_<tool>`. A slow or dead server fails that one call, and `Reconnect` starts it again. Environment and header values stay in `mcp.json`: they are never shown, and never reach the conversations or the records.

## The agent has real power

The tools run on your machine with your permissions. `run_command` runs whatever shell command the model chooses, and `write_file` replaces files. There is no sandbox and no approval step yet. Point it only at a project you are happy for it to change, ideally one under version control.

## How it is built

| Path | What it holds |
| --- | --- |
| `src/engine.py` | One agent turn as pure steps over the engine: open a turn, build a request, accept a reply, turn a call into a process, feed the results back |
| `src/transcript.py` | The wire history read into display items, and the model's Markdown read into typed blocks, spans and tables |
| `src/highlight.py` | One language's code read into coloured tokens in one forward pass, and file extensions read into languages |
| `src/model.ospml` | The store and its session document |
| `src/app.ospml` | The action union and update: every step of a turn as a capability and its completion |
| `src/view.ospml` | The regions: sidebar, header, transcript, composer and inspector |
| `test/` | Offline tests of the turn logic, the commands, failures and Markdown |
| `../../hosts/swift/Tests/` | The SwiftUI host's process adapter: signals ignored, floods, background children and cancellation |
| `test-app/` | The real update path with scripted completions: typing, the whole tool loop, Stop, failures, switching and a restart |
| `drive/` | Drives the real window with real keystrokes and mouse events, for live testing |

A model request is an `Http` capability and a tool is a `Run` capability, so the window stays live while the agent works, and two conversations can work at once. This is `Agent.run_sync` taken apart step by step, as CPython's `agent.iter` does.

## Running on another host

Every pixel is a framework node, a frame and a theme token, so the application contains no host code. Only `Engine.tool_command` names a platform, to pick the shell, and it has POSIX and PowerShell forms. The [SwiftUI host](../../hosts/swift/README.md) draws it today. Its README lists what the WinUI host needs to draw the same app on Windows.

## Testing it

`make -C apps/tests agent` runs the host-free suites on any platform. `make -C apps/macos/tests agent-headless` and `agent-live` run the two modes below.

`self-test.sh` plays the same scripts in `self-test/` through the real views, update and capabilities on the SwiftUI host ([UI-MACHINE-SELF-TEST]):

```sh
examples/agent/self-test.sh                   # no window to see, the model a test double, pictures against goldens
examples/agent/self-test.sh --live            # a real window and real DeepSeek calls; spends tokens
examples/agent/self-test.sh --live key.env zai/glm-5.3  # the same with a dotenv file and a model you name
examples/agent/self-test.sh --update-goldens  # writes this macOS version's goldens from a doubled run
```

| Script | What it proves |
| --- | --- |
| `self-test/coding.json` | Fixes a project and adds a feature through the composer. Both modes run the project's tests afterwards, and `--live` also asks an LLM judge whether the agent's words match them |
| `self-test/controls.json` | Every control: the key field, suggestions, a failed request and Try again, the model picker, Thought and tool disclosures, Stop, switching, the inspector and its splitter, and Open folder. Doubled only, because a model's words cannot be scripted |
| `self-test/code-blocks.json` | One answer with fenced python, typescript, rust, bash, json and diff blocks, drawn with their syntax colours. Doubled only |
| `self-test/restart.json` | What `controls.json` saved comes back after a restart, without the key |
| `self-test/selection.json` | Dragging across paragraphs, lists, code blocks, tables and bubbles with real mouse events inside the window, Cmd-A and Cmd-C on a named pasteboard |
| `self-test/mcp.json` | Real MCP servers over stdio, HTTP and SSE with the model doubled: add through the form and Paste JSON, tool calls, a tool error, a dead server and Reconnect, a project server off then on, remove. `verify_mcp.py` checks what the model was sent, and `self-test.sh` keeps planted secrets out of the records |
| `self-test/mcp-restart.json` | The servers and switches come back after a restart |
| `self-test/settings.json` | Each provider's key, model and effort against a strict double, a 402 and removing a key |
| `self-test/tasks/` | `--live` only: harder tasks, each checked by a hidden `verify.py` the agent never sees |

A script acts through the events the views send, not with a mouse. `drive/app.sh` opens the app for a person's real keystrokes and clicks and captures only the app's window, which needs Accessibility and Screen Recording permission for the terminal.
