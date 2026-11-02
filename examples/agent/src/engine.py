# osprey: flavor=python
# osprey: namespace=agent
# osprey: module=Engine
"""The coding agent's turn logic over @osprey/pydantic_ai, with every effect
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
from pydantic_ai import Models
from pydantic_ai import ModelsOpenai
from pydantic_ai import Profiles
from pydantic_ai import Tools

__all__ = [
    "ToolCall", "ToolResult", "Reply", "Command", "new_history", "add_user", "add_returns", "request_body",
    "accept", "tool_command", "tool_summary", "tool_result_text", "url", "profile_of", "usage_of", "Totals",
    "files_changed", "DEEPSEEK_URL", "pending", "last_calls",
]

DEEPSEEK_URL = "https://api.deepseek.com/chat/completions"
"""DeepSeek's OpenAI-compatible Chat Completions endpoint."""

OUTPUT_LIMIT = 40000
"""The most tool output, in bytes, a model is shown; the rest is cut with a note."""


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


def messages_of(history):
    """The typed messages of a stored history; a corrupt store reads as empty, and the next turn starts clean."""
    return unwrap_or(M.validate_messages(history), [])


def system_prompt(workspace):
    return "You are a senior software engineer working in the project at " + workspace + " through tools. Read before you edit, make the smallest correct change and run the tests after every change. Paths are relative to the project. Answer concisely in Markdown; when you finish a task, say what you changed and how you verified it."


def add_user(history, workspace, text):
    """Opens a turn: the person's words, with the system prompt when the conversation is new."""
    prior = messages_of(history)
    system = [M.SystemPromptPart(content=system_prompt(workspace))] if len(prior) == 0 else []
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


def definitions():
    """The tools the model may call."""
    return [
        Tools.tool_definition("list_files", "Lists every file in the project.", schema("{\"type\":\"object\",\"properties\":{},\"required\":[]}")),
        Tools.tool_definition("read_file", "Reads a text file.", schema("{\"type\":\"object\",\"properties\":{\"path\":{\"type\":\"string\",\"description\":\"Path relative to the project\"}},\"required\":[\"path\"]}")),
        Tools.tool_definition("write_file", "Replaces a whole text file, creating folders as needed.", schema("{\"type\":\"object\",\"properties\":{\"path\":{\"type\":\"string\",\"description\":\"Path relative to the project\"},\"content\":{\"type\":\"string\",\"description\":\"The whole new content\"}},\"required\":[\"path\",\"content\"]}")),
        Tools.tool_definition("run_command", "Runs a shell command in the project and returns its exit status and output.", schema("{\"type\":\"object\",\"properties\":{\"command\":{\"type\":\"string\",\"description\":\"The command line\"}},\"required\":[\"command\"]}")),
    ]


def profile_of(model_name):
    """The profile for a model: DeepSeek's quirks for its names, OpenAI's otherwise."""
    return Profiles.deepseek_model_profile(model_name) if startsWith(model_name, "deepseek") else Profiles.openai_model_profile(model_name)


def url(_model_name):
    """Where a model's requests go."""
    return DEEPSEEK_URL


def request_body(history, model_name):
    """The Chat Completions request for the next model step, as JSON text."""
    return J.encode(ModelsOpenai.completions_body(model_name, profile_of(model_name), messages_of(history), {}, Models.with_tools(definitions())))


def call_of(part):
    return ToolCall(callId=M.tool_call_id_of(part), callName=M.tool_name_of(part), callArgs=M.args_as_json_str(part))


def accept(history, model_name, status, body):
    """Takes one HTTP answer: the failure as words, or the response added and the calls it asks for."""
    if status >= 400:
        return Reply(replyOk=False, replyProblem=E.message_of(E.http_error(status, model_name, body)), replyHistory=history, replyCalls=[])
    else:
        match ModelsOpenai.process_response(model_name, "deepseek", profile_of(model_name), body):
            case E.Ok(value=response):
                return Reply(replyOk=True, replyProblem="", replyHistory=M.dump_messages(listAppend(messages_of(history), M.of_response(response))), replyCalls=mapList(M.tool_calls(response), call_of))
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
        return windows("Set-Location -LiteralPath $args[0]; Get-Content -LiteralPath $args[1] -Raw", [workspace, arg(args_text, "path")])
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


def tool_result_text(name, status, output, problem):
    """What the model sees: the output, cut to size, with the exit status for a command."""
    cut = take(output, OUTPUT_LIMIT) + "\n[output cut at " + toString(OUTPUT_LIMIT) + " bytes]" if length(output) > OUTPUT_LIMIT else output
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


def write_paths(response):
    writes = filterList(M.tool_calls(response), lambda part: M.tool_name_of(part) == "write_file")
    return mapList(writes, lambda part: arg(M.args_as_json_str(part), "path"))


def distinct_step(acc, item):
    return acc if listContains(acc, item) else listAppend(acc, item)


def files_changed(history):
    """Every file the agent wrote in this conversation, once, in the order first written."""
    return foldList(foldList(M.responses(messages_of(history)), [], lambda acc, response: listConcat(acc, write_paths(response))), [], distinct_step)


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
