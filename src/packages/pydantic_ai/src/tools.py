# osprey: flavor=python
# osprey: namespace=pydantic_ai
# osprey: module=Tools
"""pydantic_ai.tools: the definition a model sees and the function an agent runs.

Port of pydantic_ai/tools.py (2.46.0). CPython derives a tool's name,
description and JSON schema from the function's signature; Osprey has no
runtime introspection, so a `Tool` spells them beside its `function`, which
receives the call's arguments as a JSON object and answers the return value
as text (Success) or a `ModelRetry` message (Error).
"""
from typing import Callable

__all__ = ["ToolDefinition", "Tool", "tool_definition", "tool_definition_of", "tool"]


class ToolDefinition:
    """What a model is told about one tool."""
    name: str
    description: str
    parameters_json_schema: Json
    strict: bool


class Tool:
    """A tool an agent can call: its definition and the function behind it."""
    name: str
    description: str
    parameters_json_schema: Json
    function: Callable[[dict[str, Json]], Result[str, str]]


def tool_definition(name, description, parameters_json_schema):
    """`ToolDefinition(name=, description=, parameters_json_schema=)` with the default `strict=False`."""
    return ToolDefinition(name=name, description=description, parameters_json_schema=parameters_json_schema, strict=False)


def tool(name, description, parameters_json_schema, function):
    """`Tool(function, name=, description=)` with the schema spelled explicitly."""
    return Tool(name=name, description=description, parameters_json_schema=parameters_json_schema, function=function)


def tool_definition_of(t):
    """`Tool.tool_def`: the definition the model sees."""
    return tool_definition(t.name, t.description, t.parameters_json_schema)
