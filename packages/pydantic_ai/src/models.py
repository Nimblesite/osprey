# osprey: flavor=python
# osprey: namespace=pydantic_ai
# osprey: module=Models
"""pydantic_ai.models: the request parameters and the `Model` every provider
implements.

Port of pydantic_ai/models/__init__.py (2.46.0). `Model` is an abstract class
in CPython; here it is a record whose `request` field is the implementation
(`OpenAIChatModel`, `TestModel`), so `model.request(messages, settings,
parameters)` is the port of `Model.request`. The `ModelRequestParameters`
subset carries the function tools, the text-output flag and the instruction
parts; output tools, native tools and structured output are not ported.
"""
from typing import Callable
from pydantic_ai import Exceptions
from pydantic_ai import Messages
from pydantic_ai import Profiles
from pydantic_ai import Tools

__all__ = ["ModelRequestParameters", "Model", "model_request_parameters", "with_tools", "model", "instructions_text"]


class ModelRequestParameters:
    """What the agent asks of one request: the tools it may call and the instructions to prepend."""
    function_tools: list[Tools.ToolDefinition]
    allow_text_output: bool
    instruction_parts: list[Messages.ModelRequestPart]


class Model:
    """A model: its name, the provider's system name, its profile and its request function."""
    model_name: str
    system: str
    profile: Profiles.ModelProfile
    request: Callable[[list[Messages.ModelMessage], dict[str, Json], ModelRequestParameters], Exceptions.Outcome[Messages.ModelResponse]]


def model_request_parameters():
    """`ModelRequestParameters()`: no tools, text output allowed, no instructions."""
    return ModelRequestParameters(function_tools=[], allow_text_output=True, instruction_parts=[])


def with_tools(function_tools):
    """`ModelRequestParameters(function_tools=function_tools)`."""
    return ModelRequestParameters(function_tools=function_tools, allow_text_output=True, instruction_parts=[])


def model(model_name, system, profile, request):
    """A `Model` record."""
    return Model(model_name=model_name, system=system, profile=profile, request=request)


def instruction_content(part):
    match part:
        case Messages.InstructionPart(content=content):
            return content
        case Messages.SystemPromptPart():
            return ""
        case Messages.UserPromptPart():
            return ""
        case Messages.ToolReturnPart():
            return ""
        case Messages.RetryPromptPart():
            return ""


def last_request_instructions(messages):
    """The `instructions` of the last `ModelRequest`, as `Model._get_instructions` reads them."""
    return foldList(messages, "", lambda acc, m: request_instructions(m, acc))


def request_instructions(message, acc):
    match message:
        case Messages.Request(request=request):
            return request.instructions
        case Messages.Response():
            return acc


def instructions_text(messages, parameters) -> str:
    """`Model._get_instructions`: the last request's instructions and every instruction part, joined by blank lines; empty when there are none."""
    parts = filterList(mapList(parameters.instruction_parts, instruction_content), lambda s: s != "")
    last = last_request_instructions(messages)
    all_parts = parts if last == "" else listPrepend(parts, last)
    return join(all_parts, "\n\n")
