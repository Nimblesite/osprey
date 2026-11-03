# osprey: flavor=python
# osprey: namespace=agent
# osprey: module=Engine
"""The coding agent's turn logic over @osprey/pydantic-ai, with every effect
left to the host: a model request is an `Http` capability and a tool is a `Run`
capability, so a turn is a sequence of pure steps the application drives from
completions, and the window stays live while the model thinks.

This is `Agent.run_sync` taken apart, as CPython's `agent.iter` takes it apart:
`add_user` opens a turn, `request_body` and `accept` make one model step,
`tool_command` turns a call into a process and `add_returns` feeds the results
back. The history stays in the `ModelMessagesTypeAdapter` wire form, which is
what the store persists. Nothing here knows which host or operating system it
runs on except `tool_command`, which names the platform's shell.
"""
from pydantic_ai import Exceptions as E
from pydantic_ai import JsonUtil as J
from pydantic_ai import Messages as M
from pydantic_ai import Mcp
from pydantic_ai import Models
from pydantic_ai import ModelsOpenai
from pydantic_ai import Profiles
from pydantic_ai import Settings
from pydantic_ai import Tools

__all__ = [
    "ToolCall", "ToolResult", "Reply", "Command", "new_history", "add_user", "add_returns", "request_body", "request_body_with_tools", "request_body_with_mcp", "definitions_of",
    "accept", "tool_command", "tool_summary", "tool_result_text", "url", "profile_of", "usage_of", "Totals",
    "files_changed", "pending", "last_calls", "OUTPUT_LIMIT", "cut_text", "has_turns",
    "Provider", "ModelEntry", "Wire", "PROVIDERS", "provider_id_of", "model_id_of", "provider_label", "provider_base",
    "provider_key_names", "provider_ids", "model_ids", "model_label", "model_levels", "full_model", "provider_problem",
]

OUTPUT_LIMIT = 40000
"""The most tool output, in bytes, a model is shown; the rest is cut with a note."""


class Wire:
    """How one model's requests travel: the profile family and the endpoint's quirks, read from the public model data."""
    kind: str
    strict: bool
    reasoning: bool
    sendBack: bool
    maxTokens: str


class ModelEntry:
    """One model: its id, label, effort levels in order, and its wire profile."""
    id: str
    label: str
    levels: list[str]
    wire: Wire


class Provider:
    """One provider: its id, label, dotenv key names, base URL and models, newest first."""
    id: str
    label: str
    keyNames: list[str]
    base: str
    models: list[ModelEntry]


DEEPSEEK_MODELS = [
    ModelEntry(id="deepseek-flash", label="DeepSeek V4.1 Flash", levels=["low", "high", "max"], wire=Wire(kind="deepseek", strict=True, reasoning=True, sendBack=True, maxTokens="max_tokens")),
    ModelEntry(id="deepseek-v4-pro", label="DeepSeek V4 Pro", levels=["high", "max"], wire=Wire(kind="deepseek", strict=True, reasoning=True, sendBack=True, maxTokens="max_tokens")),
    ModelEntry(id="deepseek-chat", label="DeepSeek Chat", levels=[], wire=Wire(kind="deepseek", strict=True, reasoning=False, sendBack=True, maxTokens="max_tokens")),
    ModelEntry(id="deepseek-reasoner", label="DeepSeek Reasoner", levels=[], wire=Wire(kind="deepseek", strict=True, reasoning=True, sendBack=True, maxTokens="max_tokens")),
]

MOONSHOT_MODELS = [
    ModelEntry(id="kimi-k3", label="Kimi K3", levels=["low", "high", "max"], wire=Wire(kind="moonshotai", strict=False, reasoning=True, sendBack=True, maxTokens="max_tokens")),
    ModelEntry(id="kimi-k2.7-code-highspeed", label="Kimi K2.7 Code HighSpeed", levels=[], wire=Wire(kind="moonshotai", strict=False, reasoning=True, sendBack=False, maxTokens="max_tokens")),
    ModelEntry(id="kimi-k2.7-code", label="Kimi K2.7 Code", levels=[], wire=Wire(kind="moonshotai", strict=False, reasoning=True, sendBack=False, maxTokens="max_tokens")),
    ModelEntry(id="kimi-k2.6", label="Kimi K2.6", levels=[], wire=Wire(kind="moonshotai", strict=False, reasoning=True, sendBack=False, maxTokens="max_tokens")),
]

KIMI_CODING_MODELS = [
    ModelEntry(id="k3", label="Kimi K3", levels=["low", "high", "max"], wire=Wire(kind="moonshotai", strict=False, reasoning=True, sendBack=True, maxTokens="max_tokens")),
    ModelEntry(id="k3-256k", label="Kimi K3-256K", levels=["low", "high", "max"], wire=Wire(kind="moonshotai", strict=False, reasoning=True, sendBack=True, maxTokens="max_tokens")),
    ModelEntry(id="kimi-for-coding", label="Kimi For Coding", levels=["low", "high", "max"], wire=Wire(kind="moonshotai", strict=False, reasoning=True, sendBack=True, maxTokens="max_tokens")),
    ModelEntry(id="kimi-for-coding-highspeed", label="Kimi For Coding HighSpeed", levels=[], wire=Wire(kind="moonshotai", strict=False, reasoning=True, sendBack=False, maxTokens="max_tokens")),
]

GLM_MODELS = [
    ModelEntry(id="glm-5.3", label="GLM-5.3", levels=["low", "high", "max"], wire=Wire(kind="zai", strict=True, reasoning=True, sendBack=False, maxTokens="max_tokens")),
    ModelEntry(id="glm-5.3-flash", label="GLM-5.3-Flash", levels=["low", "high", "max"], wire=Wire(kind="zai", strict=True, reasoning=True, sendBack=False, maxTokens="max_tokens")),
    ModelEntry(id="glm-5.3-highspeed", label="GLM-5.3 Highspeed", levels=["low", "high", "max"], wire=Wire(kind="zai", strict=True, reasoning=True, sendBack=False, maxTokens="max_tokens")),
    ModelEntry(id="glm-5.2", label="GLM-5.2", levels=["off", "high", "max"], wire=Wire(kind="zai", strict=True, reasoning=True, sendBack=False, maxTokens="max_tokens")),
    ModelEntry(id="glm-5.2-highspeed", label="GLM-5.2 Highspeed", levels=["off", "high", "max"], wire=Wire(kind="zai", strict=True, reasoning=True, sendBack=False, maxTokens="max_tokens")),
    ModelEntry(id="glm-5-turbo", label="GLM-5-Turbo", levels=[], wire=Wire(kind="zai", strict=True, reasoning=True, sendBack=False, maxTokens="max_tokens")),
    ModelEntry(id="glm-4.7", label="GLM-4.7", levels=[], wire=Wire(kind="zai", strict=True, reasoning=True, sendBack=False, maxTokens="max_tokens")),
]

PROVIDERS = [
    Provider(id="deepseek", label="DeepSeek", keyNames=["DEEPSEEKKEY", "DEEPSEEK_API_KEY"], base="https://api.deepseek.com", models=DEEPSEEK_MODELS),
    Provider(id="moonshotai", label="Kimi (Moonshot API)", keyNames=["MOONSHOT_API_KEY"], base="https://api.moonshot.ai/v1", models=MOONSHOT_MODELS),
    Provider(id="kimi-coding", label="Kimi Code plan", keyNames=["KIMI_API_KEY"], base="https://api.kimi.com/coding/v1", models=KIMI_CODING_MODELS),
    Provider(id="zai", label="GLM (Z.ai API)", keyNames=["ZAI_API_KEY", "GLM_API_KEY"], base="https://api.z.ai/api/paas/v4", models=GLM_MODELS),
    Provider(id="zai-coding", label="GLM Coding Plan", keyNames=["ZAI_CODING_API_KEY"], base="https://api.z.ai/api/coding/paas/v4", models=GLM_MODELS),
]
"""The catalogue: every provider with its models, newest first."""


def find_provider(provider_id):
    """The provider with `provider_id`, or an error."""
    return listGet(filterList(PROVIDERS, lambda p: p.id == provider_id), 0)


def find_entry(provider, model_id):
    """`model_id` among `provider`'s models, or an error."""
    return listGet(filterList(provider.models, lambda m: m.id == model_id), 0)


def provider_by_id(provider_id):
    """The provider with `provider_id`; an unknown id reads as an empty provider of that name."""
    return unwrap_or(find_provider(provider_id), Provider(id=provider_id, label=provider_id, keyNames=[], base="", models=[]))


def provider_id_of(model):
    """The provider of `provider/model`; a value with no `/` is DeepSeek's, as saved conversations hold."""
    parts = unwrap_or(split(model, "/"), [model])
    return "deepseek" if len(parts) < 2 else unwrap_or(listGet(parts, 0), "deepseek")


def model_id_of(model):
    """The bare model name of `provider/model`; a value with no `/` is its own name."""
    parts = unwrap_or(split(model, "/"), [model])
    return model if len(parts) < 2 else unwrap_or(listGet(parts, 1), model)


def entry_of(model):
    """The catalogue entry for `model`; an unknown one reads as an empty entry of that name."""
    provider = provider_by_id(provider_id_of(model))
    mid = model_id_of(model)
    return unwrap_or(find_entry(provider, mid), ModelEntry(id=mid, label=mid, levels=[], wire=Wire(kind="openai", strict=True, reasoning=False, sendBack=False, maxTokens="max_tokens")))


def provider_problem(model):
    """Why `model` cannot be used, in words; empty when it can."""
    pid = provider_id_of(model)
    match find_provider(pid):
        case Error(message=_m):
            return f"Unknown provider '{pid}'."
        case Success(value=provider):
            mid = model_id_of(model)
            match find_entry(provider, mid):
                case Error(message=_m):
                    return f"{provider.label} has no model named '{mid}'."
                case Success(value=_v):
                    return ""


def provider_ids():
    """Every provider id, in catalogue order."""
    return mapList(PROVIDERS, lambda p: p.id)


def provider_label(provider_id):
    """A provider's label."""
    return provider_by_id(provider_id).label


def provider_base(provider_id):
    """A provider's base URL."""
    return provider_by_id(provider_id).base


def provider_key_names(provider_id):
    """A provider's dotenv key names."""
    return provider_by_id(provider_id).keyNames


def model_ids(provider_id):
    """The models of `provider_id`, in catalogue order."""
    return mapList(provider_by_id(provider_id).models, lambda m: m.id)


def model_label(model):
    """The catalogue label of `provider/model` or a bare name."""
    return entry_of(model).label


def model_levels(model):
    """The effort levels of `model`, in order; empty when it has none."""
    return entry_of(model).levels


def full_model(provider_id, model_id):
    """`provider/model`."""
    return f"{provider_id}/{model_id}"


class ToolCall:
    """A call the model asked for."""
    callId: str
    callName: str
    callArgs: str


class ToolResult:
    """What a tool answered for one call."""
    resultId: str
    resultName: str
    resultContent: str


class Reply:
    """One model step: the history with the response added, or why it failed, and the calls to run."""
    replyOk: bool
    replyProblem: str
    replyHistory: Json
    replyCalls: list[ToolCall]


class Command:
    """The process a tool call becomes."""
    executable: str
    arguments: list[str]


class Totals:
    """Usage across a history: requests, tool calls, tokens, and the input tokens of the last request, which is how full the context is."""
    totalRequests: int
    totalTools: int
    totalInput: int
    totalOutput: int
    lastInput: int


def new_history():
    """No messages yet."""
    return JsonArray([])


def has_turns(history):
    """Whether anything has been said in the conversation."""
    return len(messages_of(history)) > 0


def messages_of(history):
    """The typed messages of a stored history; a corrupt store reads as empty, and the next turn starts clean."""
    return unwrap_or(M.validate_messages(history), [])


def system_prompt(workspace):
    return "You are a senior software engineer working in the project at " + workspace + " through tools. Read before you edit, make the smallest correct change and run the tests after every change. Paths are relative to the project. Answer concisely in Markdown; when you finish a task, say what you changed and how you verified it."


def system_of(part):
    """A system prompt part's content, empty for any other part."""
    match part:
        case M.SystemPromptPart(content=content):
            return content
        case _:
            return ""


def newest_system_prompt(messages):
    """The most recent system prompt in the history, empty when there is none."""
    parts = foldList(M.requests(messages), [], lambda acc, request: listConcat(acc, request.parts))
    prompts = filterList(mapList(parts, system_of), lambda text: text != "")
    return unwrap_or(listGet(prompts, len(prompts) - 1), "")


def add_user(history, workspace, text):
    """Opens a turn: the person's words, with the system prompt whenever the newest one in the history does not name this folder, so a conversation moved to another project hears the move."""
    prior = messages_of(history)
    system = [] if newest_system_prompt(prior) == system_prompt(workspace) else [M.SystemPromptPart(content=system_prompt(workspace))]
    request = M.ModelRequest(parts=listAppend(system, M.UserPromptPart(content=text)), instructions="")
    return M.dump_messages(listAppend(prior, M.of_request(request)))


def return_part(result):
    return M.ToolReturnPart(tool_name=result.resultName, content=result.resultContent, tool_call_id=result.resultId)


def add_returns(history, results):
    """Feeds every tool's answer back as one request, as the agent graph does."""
    request = M.ModelRequest(parts=mapList(results, return_part), instructions="")
    return M.dump_messages(listAppend(messages_of(history), M.of_request(request)))


def schema(text):
    return unwrap_or(jsonDecode(text), JsonNull())


def definitions(extra):
    """The built-in tools followed by every connected server's MCP tools."""
    return listConcat([
        Tools.tool_definition("list_files", "Lists every file in the project.", schema("{\"type\":\"object\",\"properties\":{},\"required\":[]}")),
        Tools.tool_definition("read_file", "Reads a text file.", schema("{\"type\":\"object\",\"properties\":{\"path\":{\"type\":\"string\",\"description\":\"Path relative to the project\"}},\"required\":[\"path\"]}")),
        Tools.tool_definition("write_file", "Replaces a whole text file, creating folders as needed.", schema("{\"type\":\"object\",\"properties\":{\"path\":{\"type\":\"string\",\"description\":\"Path relative to the project\"},\"content\":{\"type\":\"string\",\"description\":\"The whole new content\"}},\"required\":[\"path\",\"content\"]}")),
        Tools.tool_definition("run_command", "Runs a shell command in the project and returns its exit status and output.", schema("{\"type\":\"object\",\"properties\":{\"command\":{\"type\":\"string\",\"description\":\"The command line\"}},\"required\":[\"command\"]}")),
    ], extra)


def base_profile(kind, model_id):
    """The package profile function for `kind`: the provider's profile, OpenAI's for anything else."""
    if kind == "deepseek":
        return Profiles.deepseek_model_profile(model_id)
    elif kind == "moonshotai":
        return Profiles.moonshotai_provider_profile(model_id)
    elif kind == "zai":
        return Profiles.zai_provider_profile(model_id)
    else:
        return Profiles.openai_model_profile(model_id)


def profile_of(model):
    """The profile for `model`: the package's provider profile, overridden per entry where the data says so."""
    entry = entry_of(model)
    base = base_profile(entry.wire.kind, entry.id)
    strict = base if entry.wire.strict else mapSet(base, "openai_supports_strict_tool_definition", JsonBool(False))
    thinking = strict if not entry.wire.reasoning else mapSet(strict, "supports_thinking", JsonBool(True))
    sent = thinking if not entry.wire.sendBack else mapSet(mapSet(thinking, "openai_chat_thinking_field", JsonString("reasoning_content")), "openai_chat_send_back_thinking_parts", JsonString("field"))
    return sent if entry.wire.maxTokens != "max_tokens" else mapSet(sent, "openai_chat_supports_max_completion_tokens", JsonBool(False))


def url(model):
    """Where `model`'s requests go: its provider's Chat Completions endpoint, empty for an unknown provider."""
    match find_provider(provider_id_of(model)):
        case Success(value=provider):
            return provider.base + "/chat/completions"
        case Error(message=_m):
            return ""


def effort_setting(effort):
    """The `thinking` setting value a level spells: off disables thinking, anything else names the wire level."""
    return JsonBool(False) if effort == "off" else JsonString(effort)


def entry_tools(acc, entry):
    """One server's listed tools as prefixed model definitions; an entry that cannot be used was already reported when it was listed."""
    name = J.text_at(entry, "name", "")
    return foldList(J.list_at(entry, "tools"), acc, lambda tools, tool_json: tool_step(tools, name, tool_json))


def tool_step(acc, name, tool_json):
    match Mcp.tool_of(tool_json):
        case Success(value=tool):
            return listAppend(acc, Mcp.tool_definition_of(name, tool))
        case Error():
            return acc


def definitions_of(servers_json):
    """Every connected server's tools as model definitions, named `<server>_<tool>`."""
    return foldList(J.list_at(servers_json, "servers"), [], entry_tools)


def completions(entry, profile, history, settings, tools):
    return J.encode(ModelsOpenai.completions_body(entry.id, profile, messages_of(history), settings, Models.with_tools(tools)))


def request_body_with_tools(history, model, effort, extra_tools):
    """The Chat Completions request with the built-in and given tools."""
    settings = Settings.model_settings() if effort == "" else mapSet(Settings.model_settings(), "thinking", effort_setting(effort))
    return completions(entry_of(model), profile_of(model), history, settings, definitions(extra_tools))


def request_body_with_mcp(history, model, effort, servers_json):
    """The Chat Completions request with the built-in tools and every connected MCP server's."""
    settings = Settings.model_settings() if effort == "" else mapSet(Settings.model_settings(), "thinking", effort_setting(effort))
    return completions(entry_of(model), profile_of(model), history, settings, definitions(definitions_of(servers_json)))


def request_body(history, model, effort):
    """A request with no MCP servers, kept as the engine's small pure surface."""
    return request_body_with_tools(history, model, effort, [])


def call_of(part):
    return ToolCall(callId=M.tool_call_id_of(part), callName=M.tool_name_of(part), callArgs=M.args_as_json_str(part))


def accept(history, model, status, body):
    """Takes one HTTP answer: the failure as words, or the response added and the calls it asks for."""
    entry = entry_of(model)
    provider = provider_by_id(provider_id_of(model))
    profile = profile_of(model)
    if status >= 400:
        return Reply(replyOk=False, replyProblem=E.message_of(E.status_error(status, entry.id, body)), replyHistory=history, replyCalls=[])
    else:
        match ModelsOpenai.process_response(entry.id, provider.id, profile, body):
            case E.Ok(value=response):
                calls = M.tool_calls(response)
                if len(calls) == 0 and trim(M.text(response)) == "":
                    return Reply(replyOk=False, replyProblem="The model returned no answer.", replyHistory=history, replyCalls=[])
                else:
                    return Reply(replyOk=True, replyProblem="", replyHistory=M.dump_messages(listAppend(messages_of(history), M.of_response(response))), replyCalls=mapList(calls, call_of))
            case E.Failed(error=error):
                return Reply(replyOk=False, replyProblem=E.message_of(error), replyHistory=history, replyCalls=[])


def arg(args_text, name):
    return J.text_at(unwrap_or(jsonDecode(args_text), JsonNull()), name, "")


def posix(script, values):
    """A POSIX shell running `script` with `values` as `$0`, `$1`...: arguments, never spliced into the script."""
    return Command(executable="/bin/sh", arguments=listConcat(["-c", script], values))


def windows(script, values):
    """PowerShell running `script` with `values` as `$args`: arguments, never spliced into the script."""
    return Command(executable="C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe", arguments=listConcat(["-NoProfile", "-NonInteractive", "-Command", script], values))


def posix_tool(workspace, name, args_text):
    if name == "list_files":
        return posix("cd \"$0\" && find . -type f -not -path './.git/*' -not -path '*/node_modules/*' -not -path '*/__pycache__/*' | sort | head -500", [workspace])
    elif name == "read_file":
        return posix("cd \"$0\" && cat -- \"$1\" 2>&1", [workspace, arg(args_text, "path")])
    elif name == "write_file":
        return posix("cd \"$0\" && mkdir -p \"$(dirname -- \"$1\")\" && printf '%s' \"$2\" > \"$1\" && printf 'wrote %s (%s bytes)\\n' \"$1\" \"$(wc -c < \"$1\" | tr -d ' ')\"", [workspace, arg(args_text, "path"), arg(args_text, "content")])
    elif name == "run_command":
        return posix("cd \"$0\" && /bin/sh -c \"$1\" 2>&1", [workspace, arg(args_text, "command")])
    else:
        return posix("echo \"Unknown tool: $0\"; exit 127", [name])


def windows_tool(workspace, name, args_text):
    if name == "list_files":
        return windows("Set-Location -LiteralPath $args[0]; Get-ChildItem -Recurse -File -Force | Where-Object { $_.FullName -notmatch '\\\\(\\.git|node_modules|__pycache__)\\\\' } | Select-Object -First 500 | ForEach-Object { Resolve-Path -Relative $_.FullName }", [workspace])
    elif name == "read_file":
        return windows("Set-Location -LiteralPath $args[0]; Get-Content -LiteralPath $args[1] -Raw -ErrorAction Stop", [workspace, arg(args_text, "path")])
    elif name == "write_file":
        return windows("Set-Location -LiteralPath $args[0]; $folder = Split-Path -Parent $args[1]; if ($folder) { New-Item -ItemType Directory -Force -Path $folder | Out-Null }; [IO.File]::WriteAllText((Join-Path (Get-Location) $args[1]), $args[2]); \"wrote $($args[1])\"", [workspace, arg(args_text, "path"), arg(args_text, "content")])
    elif name == "run_command":
        return windows("Set-Location -LiteralPath $args[0]; & cmd.exe /d /c $args[1] 2>&1; exit $LASTEXITCODE", [workspace, arg(args_text, "command")])
    else:
        return windows("\"Unknown tool: $($args[0])\"; exit 127", [name])


def tool_command(platform, workspace, name, args_text):
    """The process a tool call becomes on `platform`."""
    return windows_tool(workspace, name, args_text) if platform == "windows" else posix_tool(workspace, name, args_text)


def tool_summary(name, args_text):
    """What a call does, in a line: the file or the command."""
    if name == "run_command":
        return arg(args_text, "command")
    elif name == "list_files":
        return "project files"
    else:
        return arg(args_text, "path")


def whole_characters(text, at):
    """`at`, moved back off any UTF-8 continuation byte, so a cut never splits a character."""
    byte = unwrap_or(byteAt(text, at), 0)
    return whole_characters(text, at - 1) if at > 0 and byte >= 128 and byte < 192 else at


def cut_text(text, limit):
    """At most `limit` bytes of `text`, in whole characters."""
    return take(text, whole_characters(text, limit))


def tool_result_text(name, status, output, problem):
    """What the model sees: the output, cut to size, with the exit status for a command."""
    cut = cut_text(output, OUTPUT_LIMIT) + "\n[output cut at " + toString(OUTPUT_LIMIT) + " bytes]" if length(output) > OUTPUT_LIMIT else output
    if problem != "":
        return "The tool could not run: " + problem
    elif name == "run_command":
        return "exit " + toString(status) + "\n" + cut
    elif status != 0:
        return "failed with status " + toString(status) + "\n" + cut
    else:
        return cut


def totals_step(acc, response):
    return Totals(totalRequests=acc.totalRequests + 1, totalTools=acc.totalTools + len(M.tool_calls(response)), totalInput=acc.totalInput + response.usage.input_tokens, totalOutput=acc.totalOutput + response.usage.output_tokens, lastInput=response.usage.input_tokens)


def usage_of(history):
    """Requests, tool calls and tokens across a conversation."""
    return foldList(M.responses(messages_of(history)), Totals(totalRequests=0, totalTools=0, totalInput=0, totalOutput=0, lastInput=0), totals_step)


def written_id(part):
    """The call a part reports as having written its file."""
    match part:
        case M.ToolReturnPart(content=content, tool_call_id=tool_call_id):
            return tool_call_id if startsWith(content, "wrote ") else ""
        case _:
            return ""


def write_paths(written, response):
    writes = filterList(M.tool_calls(response), lambda part: M.tool_name_of(part) == "write_file" and listContains(written, M.tool_call_id_of(part)))
    return mapList(writes, lambda part: arg(M.args_as_json_str(part), "path"))


def distinct_step(acc, item):
    return acc if listContains(acc, item) else listAppend(acc, item)


def files_changed(history):
    """Every file the agent wrote in this conversation, once, in the order first written. A write that failed or never ran changed nothing."""
    messages = messages_of(history)
    written = foldList(M.requests(messages), [], lambda acc, request: listConcat(acc, mapList(request.parts, written_id)))
    return foldList(foldList(M.responses(messages), [], lambda acc, response: listConcat(acc, write_paths(written, response))), [], distinct_step)


def last_calls(history):
    """The tool calls of the last response."""
    responses = M.responses(messages_of(history))
    match listGet(responses, len(responses) - 1):
        case Success(value=response):
            return mapList(M.tool_calls(response), call_of)
        case Error(message=_m):
            return []


def answered(history, call_id):
    """Whether a request in the history already answers the call."""
    parts = foldList(M.requests(messages_of(history)), [], lambda acc, request: listConcat(acc, request.parts))
    return len(filterList(parts, lambda part: return_id(part) == call_id)) > 0


def return_id(part):
    match part:
        case M.ToolReturnPart(tool_call_id=tool_call_id):
            return tool_call_id
        case M.RetryPromptPart(tool_call_id=tool_call_id):
            return tool_call_id
        case _:
            return ""


def pending(history, results):
    """The calls of the last response with no answer yet, neither in the history nor among `results`."""
    return filterList(last_calls(history), lambda call: not answered(history, call.callId) and len(filterList(results, lambda r: r.resultId == call.callId)) == 0)
