# osprey: flavor=python
# osprey: namespace=pydantic_ai
# osprey: module=Agent
"""pydantic_ai.agent: the tool-calling run loop.

Port of pydantic_ai/agent/__init__.py and _agent_graph.py (2.46.0), text
output only: `Agent(model, system_prompt=)` builds the agent, `tool_plain`
registers a tool, `run_sync` sends the user prompt, calls every tool the
model asks for (a tool's `Error` becomes a `RetryPromptPart`, as
`ModelRetry` does) and repeats until the model answers with text alone. The
run stops with `UsageLimitExceeded` after `REQUEST_LIMIT` requests, CPython's
`UsageLimits.request_limit` default. Dependencies, structured output,
streaming runs, history processors and instrumentation are not ported.
"""
from pydantic_ai import Exceptions as E
from pydantic_ai import Messages as M
from pydantic_ai import Models
from pydantic_ai import Tools
from pydantic_ai import Usage

__all__ = ["Agent", "AgentRunResult", "agent", "tool_plain", "run_sync", "REQUEST_LIMIT", "all_messages", "new_messages"]

REQUEST_LIMIT = 50
"""`UsageLimits.request_limit`: the most model requests one run may make."""


class Agent:
    """An agent: its model, system prompt and tools."""
    model: Models.Model
    system_prompt: str
    tools: list[Tools.Tool]


class AgentRunResult:
    """`AgentRunResult`: the text output, the whole history, the usage and where this run's messages start."""
    output: str
    messages: list[M.ModelMessage]
    usage: Usage.RunUsage
    new_message_index: int


def agent(model, system_prompt):
    """`Agent(model, system_prompt=system_prompt)` with no tools."""
    return Agent(model=model, system_prompt=system_prompt, tools=[])


def tool_plain(a, t):
    """`agent.tool_plain(func)`: the agent with `t` registered."""
    return Agent(model=a.model, system_prompt=a.system_prompt, tools=listAppend(a.tools, t))


def all_messages(result) -> list[M.ModelMessage]:
    """`AgentRunResult.all_messages()`."""
    return result.messages


def new_messages(result) -> list[M.ModelMessage]:
    """`AgentRunResult.new_messages()`: the messages this run added."""
    indexes = filterList(toList(map(range(len(result.messages)), lambda i: i)), lambda i: i >= result.new_message_index)
    return mapList(indexes, lambda i: unwrap_or(listGet(result.messages, i), M.of_request(M.model_request([]))))


def first_request(a, user_prompt, message_history) -> M.ModelRequest:
    """The first request of a run: the system prompt (only when the history is empty) and the user prompt."""
    system = [] if a.system_prompt == "" or len(message_history) > 0 else [M.SystemPromptPart(content=a.system_prompt)]
    return M.ModelRequest(parts=listAppend(system, M.UserPromptPart(content=user_prompt)), instructions="")


def run_sync(a, user_prompt, message_history) -> E.Outcome[AgentRunResult]:
    """`agent.run_sync(user_prompt, message_history=message_history)`."""
    messages = listAppend(message_history, M.of_request(first_request(a, user_prompt, message_history)))
    return step(a, messages, Usage.run_usage(), len(message_history))


def parameters_of(a) -> Models.ModelRequestParameters:
    return Models.with_tools(mapList(a.tools, Tools.tool_definition_of))


def step(a, messages, usage, start) -> E.Outcome[AgentRunResult]:
    """One model request, then the tool calls it asked for, until a text answer."""
    if usage.requests >= REQUEST_LIMIT:
        return E.Failed(error=E.UsageLimitExceeded(message=f"The next request would exceed the request_limit of {REQUEST_LIMIT}"))
    else:
        match a.model.request(messages, {}, parameters_of(a)):
            case E.Ok(value=response):
                return continue_run(a, listAppend(messages, M.of_response(response)), Usage.incr(usage, response.usage), start, response)
            case E.Failed(error=error):
                return E.Failed(error=error)


def continue_run(a, messages, usage, start, response) -> E.Outcome[AgentRunResult]:
    calls = M.tool_calls(response)
    if len(calls) == 0:
        return E.Ok(value=AgentRunResult(output=M.text(response), messages=messages, usage=usage, new_message_index=start))
    else:
        returns = mapList(calls, lambda call: call_tool(a, call))
        counted = Usage.RunUsage(
            requests=usage.requests, tool_calls=usage.tool_calls + len(calls), input_tokens=usage.input_tokens,
            cache_write_tokens=usage.cache_write_tokens, cache_read_tokens=usage.cache_read_tokens, output_tokens=usage.output_tokens,
            input_audio_tokens=usage.input_audio_tokens, cache_audio_read_tokens=usage.cache_audio_read_tokens,
            output_audio_tokens=usage.output_audio_tokens, details=usage.details,
        )
        return step(a, listAppend(messages, M.of_request(M.model_request(returns))), counted, start)


def call_tool(a, call) -> M.ModelRequestPart:
    """Runs the tool a call names: its text as a `ToolReturnPart`, its `Error` as a `RetryPromptPart`, an unknown name as a retry too."""
    name = M.tool_name_of(call)
    return call_named(a, call, name, 0)


def call_named(a, call, name, i) -> M.ModelRequestPart:
    """Finds the tool by index so the stored function keeps its effect provenance, then calls it."""
    match listGet(a.tools, i):
        case Success(value=t):
            if t.name == name:
                match t.function(M.args_as_dict(call)):
                    case Success(value=content):
                        return M.ToolReturnPart(tool_name=name, content=content, tool_call_id=M.tool_call_id_of(call))
                    case Error(message=message):
                        return M.RetryPromptPart(content=message, tool_name=name, tool_call_id=M.tool_call_id_of(call))
            else:
                return call_named(a, call, name, i + 1)
        case Error(message=_m):
            return M.RetryPromptPart(content=f"Unknown tool name: '{name}'. Available tools: {join(mapList(a.tools, lambda t: t.name), ", ")}", tool_name=name, tool_call_id=M.tool_call_id_of(call))
