# Osprey Agent

A coding agent as a desktop application, written in Osprey on [`@osprey/native`](../../packages/native/README.md) and powered by the [`@osprey/pydantic_ai`](../../packages/pydantic_ai/README.md) engine. Give it a project folder and a task. It reads, edits and runs commands through tools, and shows its work as it goes.

```sh
examples/agent/run.sh /path/to/project          # uses DEEPSEEKKEY from the repository's .env
examples/agent/run.sh /path/to/project key.env  # or a dotenv file you name
examples/agent/run.sh --test                    # the suite: no network, no window
```

![Osprey Agent fixing a project and summing up its work](screenshot.png)

Without a key, the composer asks for one. The key stays in memory and in the app's private support folder, and is never written to the conversations file. Conversations are saved after every turn.

## The agent has real power

The tools run on your machine with your permissions. `run_command` runs whatever shell command the model chooses, and `write_file` replaces files. There is no sandbox and no approval step yet. Point it only at a project you are happy for it to change, ideally one under version control.

## How it is built

| Path | What it holds |
| --- | --- |
| `src/engine.py` | One agent turn as pure steps over the engine: open a turn, build a request, accept a reply, turn a call into a process, feed the results back |
| `src/transcript.py` | The wire history read into display items, and the model's Markdown read into typed blocks, spans and tables |
| `src/model.ospml` | The store and its session document |
| `src/app.ospml` | The action union and update: every step of a turn as a capability and its completion |
| `src/view.ospml` | The regions: sidebar, header, transcript, composer and inspector |
| `test/` | Offline tests of the turn logic, the commands, failures and Markdown |
| `test-app/` | The real update path with scripted completions: typing, the whole tool loop, Stop, failures, switching and a restart |
| `drive/` | Drives the real window with real keystrokes and mouse events, for live testing |

A model request is an `Http` capability and a tool is a `Run` capability, so the window stays live while the agent works, and two conversations can work at once. This is `Agent.run_sync` taken apart step by step, as CPython's `agent.iter` does.

## Running on another host

Every pixel is a framework node, a frame and a theme token, so the application contains no host code. Only `Engine.tool_command` names a platform, to pick the shell, and it has POSIX and PowerShell forms. The [SwiftUI host](../../hosts/swift/README.md) draws it today. Its README lists what the WinUI host needs to draw the same app on Windows.

## Testing it live

`drive/app.sh` opens the app, types with real keystrokes, clicks with real mouse events and captures only the app's window:

```sh
examples/agent/drive/demo-project.sh /tmp/osprey-agent-demo
examples/agent/drive/app.sh open /tmp/osprey-agent-demo "$PWD/.env"
examples/agent/drive/app.sh type "Run the tests and fix calc."
examples/agent/drive/app.sh key return
examples/agent/drive/app.sh shot /tmp/agent.png
```

It needs Accessibility and Screen Recording permission for the terminal. Check the outcome independently of what the agent says, for example with `python3 test_calc.py` in the demo project.
