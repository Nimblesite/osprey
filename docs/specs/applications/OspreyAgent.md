# Osprey Agent

**Status:** most of this application contract is not implemented yet. [`examples/agent/src/engine.py`](../../../examples/agent/src/engine.py) answers every model with DeepSeek's endpoint and profile, `model.ospml` holds no effort and seeds one key from two `.env` names alone, `view.ospml` has no Settings pane, and the SwiftUI host selects one `Text` at a time. [Plan 0052](../../plans/0052-agent.md) delivers it over contract that already exists: the process capabilities [UI-EFFECTS-PROCESS] of [`@osprey/native`](../../../packages/native/README.md) and the thinking levels of [`@osprey/pydantic-ai`](../../../packages/pydantic_ai/README.md).

The Osprey Agent is the coding agent application at [`examples/agent/`](../../../examples/agent/README.md): one store, one `update` and one set of regions over `@osprey/native`, with model requests and tools driven through capabilities.

## What the person asked for [AGENT-REQUESTS]

Each request, in the words it was made, with the section that holds it. A request is open until its section's proof runs.

| Request | Section | State |
| --- | --- | --- |
| "We are out of ds credit! Add support for kimi and glm! port this into the package" | [AGENT-PROVIDERS] | Package done, app open |
| "The settings screen needs to allow us to CONFIGURE the PROVIDER -> SELECT MODEL" | [AGENT-SETTINGS] | Open |
| "We need to be able to select the model and its peculiar settings like effort/reasoning here" (the composer, beside the folder chip) | [AGENT-COMPOSER] | Open |
| "Where are the fucking settings?" | [AGENT-SETTINGS]: always visible at the sidebar's foot | Open |
| "finish the pydanticai app, including mcp support and the settings UI that allows the user to add/remove MCPs" | [AGENT-MCP] | Open |
| "port the MCP aspect of pydanticai into python flavor osprey and bring the MCP functionality into the app with a beautiful ui" | [AGENT-MCP] | Package done, app open |
| "How do I configure MCPs in this app?" | [AGENT-MCP], and the README's MCP section | Open |
| "I can only select one line of text. I need to be able to select a whole bunch of text in one hit" | [AGENT-SELECTION] | Open |
| "make sure that the syntax higlighting works good, verify in tests"; "no fucking syntax highlighting" | [AGENT-HIGHLIGHT] | Code done, golden open |
| "the button that says calc here should allow the user to change the path" | [AGENT-FOLDER] | Done |
| "there should be a note somewhere on the screen conveying that the agent is not sandboxed" | [AGENT-FOLDER] | Done |
| "We need to give credit to all the original authors of ported libraries and apps etc. on the website" | [the credits page](../../../website/src/credits.md) | Done |
| "automated tests to PROVE it fucking works!"; "allow the tests to run headless (with test doubles) OR for real with ACTUAL LLM CALLS" | [AGENT-PROOF] | Open |
| "prove a) all the actual agent harness UI works (remove controls that are not necessary and don't work) b) that it can solve coding problems c) that it looks fucking amazing" | [AGENT-PROOF] | Open |

## Providers, models and effort [AGENT-PROVIDERS]

Not implemented yet: `Engine.url` returns DeepSeek's Chat Completions endpoint for every model, `Engine.profile_of` chooses DeepSeek's or OpenAI's profile, `chatModel` holds a bare model name and there is no `chatEffort`.

The app's catalogue names DeepSeek, Kimi and GLM entries. The coding-plan entries reuse the upstream provider's profile with another base URL: `https://api.kimi.com/coding/v1` and `https://api.z.ai/api/coding/paas/v4`.

`chatModel` holds `provider/model`. A value with no `/` reads as DeepSeek, which is what saved conversations hold, so a saved conversation with no `/` restores as DeepSeek.

Each conversation carries its own `chatEffort`, whose choices come from the model's profile (`Profiles.thinking_levels`). A model whose profile returns none has no effort control. `chatEffort` round-trips through `chatJson`/`chatOf`.

## The composer [AGENT-COMPOSER]

Not implemented yet: the composer has a DeepSeek-only model picker and no effort control.

Beside the folder chip: a model picker listing the models of every provider with a key, labelled with provider and model such as `GLM Coding Plan · GLM-5.3`, the current one always listed; then an `Effort` picker with that model's own levels, absent when it has none. Both change only this conversation. Changing model keeps the effort when the new model has that level and clears it otherwise. With no key at all, a button `Add an API key` opens Settings and sending does nothing. A conversation whose provider lost its key cannot send, and says why. Each sidebar row names its conversation's model.

## Settings and keys [AGENT-SETTINGS]

Not implemented yet: the composer asks for one DeepSeek key, `Model::keyIn` seeds it from `.env` names `DEEPSEEKKEY` and `DEEPSEEK_API_KEY` alone, and the app writes no key file — only `chats.json`, which never holds the key.

A `Settings` button is always visible at the sidebar's foot, whether or not a key is set; it replaces today's `API key set` badge, and carries a warning symbol when the open conversation's provider has no key. It opens a pane in place of the conversation column, with a close control and two tabs: Providers and MCP servers.

Providers tab: first configure the provider, then select the model. A card per provider shows its label, its endpoint, whether a key is saved, a secure field with `Save key`, and `Remove key` once saved; a saved key's value is never shown. Below the cards, `Default model` is a `Provider` picker, then a `Model` picker of that provider's models, then an `Effort` picker when the model has levels. New conversations start with this choice.

A refused request says what to do in one sentence with an `Open Settings` button: 401 or 403 "This key was refused.", 402 "<provider> has no credit left. Pick another model, or add credit.", 429 "<provider> is rate limiting: try again shortly." The raw detail and `Try again` stay, and changing the model then pressing it works.

Provider keys are written to `providers.json` in the app's private store with mode 0600 — never `chats.json`, never the records. An empty key slot is still seeded from `.env` names `DEEPSEEKKEY`, `DEEPSEEK_API_KEY`, `MOONSHOT_API_KEY`, `KIMI_API_KEY`, `ZAI_API_KEY`, `GLM_API_KEY`.

## MCP servers [AGENT-MCP]

Not implemented yet: the app reads no `mcp.json` or `.mcp.json`, connects to no server and lists no MCP tools. The sans-IO client (`packages/pydantic_ai/src/mcp.py`) and the stdio and streamable-HTTP test server (`examples/agent/self-test/mcp_server.py`) exist unused by it.

`mcp.json` in the app's support folder configures servers for every project; `.mcp.json` in the project folder configures servers for that one project. The app reads and merges both, and the project entry wins on a name clash. Servers connect on launch and folder change.

The MCP tab of the Settings pane has a card per server: name, transport and command or URL (`env` and header values never shown), a coloured state (Off, Connecting, Connected with its tool count, Failed), the problem in words, a switch, `Reconnect`, `Remove`, and a disclosure listing its tools. `Add server` is a form with Name, Type (`Command` or `URL`), Command and Arguments or URL and Headers, and Environment, validated in words; `Paste JSON` takes the standard `{"mcpServers": ...}` block. Adding connects at once. Removing stops the process and takes its tools out of the next request. The tab writes `mcp.json` in the private store. `.mcp.json` servers are listed but off until the person turns them on, remembered per folder.

Servers speak stdio and streamable HTTP. Each tool joins the engine's tool list named `<server>_<tool>`. A tool call asks for no approval: none, the same as the built-in tools.

MCP `env` and `headers` values never reach `chats.json`, the records or the transcript.

## Selection [AGENT-SELECTION]

Not implemented yet: the SwiftUI host enables `.textSelection` per `Text` (`hosts/swift/Sources/OspreyNative/Leaves.swift`), so selection stops at a block.

The transcript renders as one selectable group of AppKit text views in the SwiftUI host, so a selection crosses transcript blocks, with Cmd-A and Cmd-C.

## Highlighting [AGENT-HIGHLIGHT]

Fenced code in an answer and `read_file` output draw through `Highlight.tokens_of` in [`examples/agent/src/highlight.py`](../../../examples/agent/src/highlight.py) as `Ui::rich` runs, with every token colour at 4.5:1 or better on the code fill in both themes.

## Folder [AGENT-FOLDER]

The folder chip in the composer changes the conversation's working folder. The window says the agent is not sandboxed.

## Blank replies [AGENT-BLANK-REPLY]

Not implemented yet: a request that returned no text leaves an empty gap in the transcript and looks like no answer (seen with two `yo` messages in a row). A turn that ends with no text says so in words, with `Try again`, and leaves no gap.

## Proof [AGENT-PROOF]

`examples/agent/self-test.sh` runs the real window with doubled requests and asserts records and goldens; `--live` runs it with real LLM calls through whichever provider has a key, with acceptance checks the model never sees. Controls that do nothing are removed, not hidden.
