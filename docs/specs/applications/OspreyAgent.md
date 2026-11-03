# Osprey Agent

**Status:** most of this application contract is not implemented yet. [`examples/agent/src/engine.py`](../../../examples/agent/src/engine.py) answers every model with DeepSeek's endpoint and profile, `model.ospml` holds no effort and seeds one key from two `.env` names alone, `view.ospml` has no Settings pane, and the SwiftUI host selects one `Text` at a time. [Plan 0052](../../plans/0052-agent.md) delivers it over contract that already exists: the process capabilities [UI-EFFECTS-PROCESS] of [`@osprey/native`](../../../packages/native/README.md) and the thinking levels of [`@osprey/pydantic-ai`](../../../packages/pydantic_ai/README.md).

The Osprey Agent is the coding agent application at [`examples/agent/`](../../../examples/agent/README.md): one store, one `update` and one set of regions over `@osprey/native`, with model requests and tools driven through capabilities.

## What the person asked for [AGENT-REQUESTS]

Each request, in the words it was made, with the section that holds it. A request is open until its section's proof runs.

| Request | Section | State |
| --- | --- | --- |
| "We are out of ds credit! Add support for kimi and glm! port this into the package" | [AGENT-PROVIDERS] | Done, doubled; no live call yet |
| "The settings screen needs to allow us to CONFIGURE the PROVIDER -> SELECT MODEL" | [AGENT-SETTINGS] | Done, doubled; no live call yet |
| "why is this locked! Does the app actually KNOW that there is no credit… This is no good unless it can be 100% sure that there is no fucking credit left and becomes unlocked reacreively"; "at least put a tooltiop there telling them wtf the is WRONG" | [AGENT-COMPOSER] | Done, doubled; no live call yet |
| "We need to be able to select the model and its peculiar settings like effort/reasoning here" (the composer, beside the folder chip) | [AGENT-COMPOSER] | Done, doubled; no live call yet |
| "Where are the fucking settings?" | [AGENT-SETTINGS]: always visible at the sidebar's foot | Done |
| "finish the pydanticai app, including mcp support and the settings UI that allows the user to add/remove MCPs" | [AGENT-MCP] | Done, with real servers and a doubled model; no live call yet |
| "port the MCP aspect of pydanticai into python flavor osprey and bring the MCP functionality into the app with a beautiful ui" | [AGENT-MCP] | Done, with real servers and a doubled model; no live call yet |
| "How do I configure MCPs in this app?" | [AGENT-MCP], and the README's MCP section | Done |
| "I can only select one line of text. I need to be able to select a whole bunch of text in one hit" | [AGENT-SELECTION] | Done, with in-process mouse events |
| "make sure that the syntax higlighting works good, verify in tests"; "no fucking syntax highlighting" | [AGENT-HIGHLIGHT] | Done |
| "the button that says calc here should allow the user to change the path" | [AGENT-FOLDER] | Done |
| "there should be a note somewhere on the screen conveying that the agent is not sandboxed" | [AGENT-FOLDER] | Done |
| "We need to give credit to all the original authors of ported libraries and apps etc. on the website" | [the credits page](../../../website/src/credits.md) | Done |
| "automated tests to PROVE it fucking works!"; "allow the tests to run headless (with test doubles) OR for real with ACTUAL LLM CALLS" | [AGENT-PROOF] | Open |
| "prove a) all the actual agent harness UI works (remove controls that are not necessary and don't work) b) that it can solve coding problems c) that it looks fucking amazing" | [AGENT-PROOF] | Open |

## Providers, models and effort [AGENT-PROVIDERS]

The catalogue is one table in `examples/agent/src/engine.py`, `Provider(id, label, keyNames, base, models)` and `ModelEntry(id, label, levels, wire)`, and every lookup goes through it. Nothing else names a provider.

| Id | Label | Base URL | Key names in `.env` |
| --- | --- | --- | --- |
| `deepseek` | DeepSeek | `https://api.deepseek.com` | `DEEPSEEKKEY`, `DEEPSEEK_API_KEY` |
| `moonshotai` | Kimi (Moonshot API) | `https://api.moonshot.ai/v1` | `MOONSHOT_API_KEY` |
| `kimi-coding` | Kimi Code plan | `https://api.kimi.com/coding/v1` | `KIMI_API_KEY` |
| `zai` | GLM (Z.ai API) | `https://api.z.ai/api/paas/v4` | `ZAI_API_KEY`, `GLM_API_KEY` |
| `zai-coding` | GLM Coding Plan | `https://api.z.ai/api/coding/paas/v4` | `ZAI_CODING_API_KEY` |

The coding-plan entries reuse the upstream provider's profile at another base URL, overridden per model where the provider requires it (no `strict`, `reasoning_content` required). Each model lists its own effort levels in order, such as `k3`: Low, High, Max. Switching provider mid-conversation produces a body the new provider accepts, even with another provider's thinking parts in the history.

`chatModel` holds `provider/model`. A value with no `/` reads as DeepSeek, which is what saved conversations hold, so a saved conversation with no `/` restores as DeepSeek.

Each conversation carries its own `chatEffort`, whose choices come from the model's profile (`Profiles.thinking_levels`). A model whose profile returns none has no effort control. `chatEffort` round-trips through `chatJson`/`chatOf`.

## The composer [AGENT-COMPOSER]

Beside the folder chip: a model picker listing the models of every provider with a key, labelled with provider and model such as `GLM Coding Plan · GLM-5.3`, the current one always listed; then an `Effort` picker with that model's own levels, absent when it has none. Both change only this conversation. Changing model keeps the effort when the new model has that level and clears it otherwise. When the conversation's provider has no key, the send button becomes a labelled `Add an API key` button that opens Settings, and a line under the composer says `No API key for <provider>. Add one to send.`; never a lone icon. A key is the only thing that stops a send, and adding one takes effect at once. The app never guesses about credit: a 402, a refused key, a rate limit or a server fault is shown on that turn, `Send` stays live, and every send asks the provider again. A launch with no dotenv argument, such as from the Dock, reads `key.env` from the app's own store. Each sidebar row names its conversation's model.

## Settings and keys [AGENT-SETTINGS]

A `Settings` button is always visible at the sidebar's foot, whether or not a key is set; it replaces today's `API key set` badge, and carries a warning symbol when the open conversation's provider has no key. It opens a pane in place of the conversation column, with a close control and two tabs: Providers and MCP servers.

Providers tab: first configure the provider, then select the model. A card per provider shows its label, its endpoint, whether a key is saved, a secure field with `Save key`, and `Remove key` once saved; a saved key's value is never shown. Below the cards, `Default model` is a `Provider` picker, then a `Model` picker of that provider's models, then an `Effort` picker when the model has levels. New conversations start with this choice.

A refused request says what to do in one sentence with an `Open Settings` button: 401 or 403 "This key was refused.", 402 "<provider> has no credit left. Pick another model, or add credit.", 429 "<provider> is rate limiting: try again shortly." The raw detail and `Try again` stay, and changing the model then pressing it works.

`providers.json` in the app's private store, mode 0600, holds the provider keys, the default model and the default effort, and is never `chats.json` or the records. Writes are serial, sharing the conversation file's save code, and a failed write says so in words. A `.env` at launch fills only empty key slots, by the names in [AGENT-PROVIDERS]: a saved or pasted key always wins.

## MCP servers [AGENT-MCP]

`mcp.json` in the app's support folder configures servers for every project; `.mcp.json` in the project folder configures servers for that one project. The app reads and merges both, and the project entry wins on a name clash. Servers connect on launch and folder change.

The MCP tab of the Settings pane has a card per server: name, transport and command or URL (`env` and header values never shown), a coloured state (Off, Connecting, Connected with its tool count, Failed), the problem in words, a switch, `Reconnect`, `Remove`, and a disclosure listing its tools. `Add server` is a form with Name, Type (`Command` or `URL`), Command and Arguments or URL and Headers, and Environment, validated in words; `Paste JSON` takes the standard `{"mcpServers": ...}` block. Adding connects at once. Removing stops the process and takes its tools out of the next request. The tab writes `mcp.json` in the private store. `.mcp.json` servers are listed with a `.mcp.json` label and no `Remove`, and stay off until the person turns them on, because a cloned repository's file starts commands. Both per-server switches and the per-folder memory live in `mcp-state.json` beside `providers.json`.

A missing document configures nothing and is no problem. A broken file, a missing command and an unsupported entry (`cwd`, or a URL that is neither HTTPS nor HTTP on this machine) each get a plain message, and an unusable entry never starts. The host allows plain HTTP to loopback only. The form forgets what it was given after `Add`, and shows with no servers. `Reconnect` restarts a server that exited, only when a person presses it. A slow or dead server fails that one call, with a deadline, and never blocks the turn or other servers. `isError` goes back to the model as the tool's text. A server's `ping` is answered. `${VAR}` expands from what a person configured, never the app's own secrets. Streamable HTTP keeps session ids and reads SSE, through the `Http` capability.

The inspector has an `MCP` section with each enabled server's state and tool count, opening Settings when pressed. A tool step names its server.

Servers speak stdio and streamable HTTP. Each tool joins the engine's tool list named `<server>_<tool>`. A tool call asks for no approval: none, the same as the built-in tools.

MCP `env` and `headers` values never reach `chats.json`, the records or the transcript.

## Selection [AGENT-SELECTION]

The transcript renders as one selectable group of AppKit text views in the SwiftUI host, so a selection crosses transcript blocks, with Cmd-A and Cmd-C. Each text view lies over its hidden SwiftUI twin, which keeps doing the layout, so a screen does not move and highlighting and inline code keep their colours.

## Highlighting [AGENT-HIGHLIGHT]

Fenced code in an answer and `read_file` output draw through `Highlight.tokens_of` in [`examples/agent/src/highlight.py`](../../../examples/agent/src/highlight.py) as `Ui::rich` runs, with every token colour at 4.5:1 or better on the code fill in both themes.

## Folder [AGENT-FOLDER]

The folder chip in the composer changes the conversation's working folder. The window says the agent is not sandboxed.

## Blank replies [AGENT-BLANK-REPLY]

A turn that ends with no text says so in words, with `Try again`, and leaves no gap.

## Proof [AGENT-PROOF]

`examples/agent/self-test.sh` runs the real window with doubled requests and asserts records and goldens; `--live` runs it with real LLM calls through the first provider with a key, in the order `zai-coding`, `zai`, `kimi-coding`, `moonshotai`, `deepseek`, with acceptance checks the model never sees, and never prints the key. MCP proofs use real server processes (`self-test/mcp_server.py`), not doubles. A strict HTTP double can require a bearer token and a URL and answers 401 or 404 otherwise, as a real server would, and `self-test/verify_providers.py` and `verify_mcp.py` assert each request's URL and exact body. Controls that do nothing are removed, not hidden. The look is judged by reading every golden.
