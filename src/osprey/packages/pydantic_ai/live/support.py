# osprey: flavor=python
# osprey: namespace=pydantic_ai_live
# osprey: module=Support
"""What the live suites share: reading the key and asking the LLM judge."""
from pydantic_ai import Direct
from pydantic_ai import Exceptions as E
from pydantic_ai import Messages as M
from pydantic_ai import Models

__all__ = ["ask_judge", "env_value", "tool_names"]


def tool_names(messages):
    """Every tool name called across the run's responses, in order."""
    return foldList(M.responses(messages), [], lambda acc, r: listConcat(acc, mapList(M.tool_calls(r), M.tool_name_of)))


def env_value(text, name):
    """The value of `name=` in dotenv text, empty when absent."""
    lines = unwrap_or(split(text, "\n"), [])
    hits = filterList(lines, lambda line: startsWith(line, name + "="))
    return trim(unwrap_or(replace(unwrap_or(listGet(hits, 0), ""), name + "=", ""), ""))


def ask_judge(model, prompt) -> str:
    """The LLM oracle's one-word verdict on `prompt`."""
    request = M.ModelRequest(parts=[M.UserPromptPart(content=prompt)], instructions="")
    match Direct.model_request(model, [M.of_request(request)], {"temperature": JsonFloat(0.0), "max_tokens": JsonInt(5)}, Models.model_request_parameters()):
        case E.Ok(value=response):
            return trim(M.text(response))
        case E.Failed(error=error):
            return f"judge failed: {E.message_of(error)}"
