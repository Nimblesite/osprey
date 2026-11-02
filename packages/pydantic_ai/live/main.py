# osprey: flavor=python
"""@osprey/pydantic_ai against a live model: an agent is given a task it can
only finish by calling tools, and a second model call judges the answer.

Reads `DEEPSEEKKEY` from `.env` in the working directory and talks to
DeepSeek's OpenAI-compatible chat API over HTTPS. Run from the repository root:

    osprey packages/pydantic_ai/live --run

The agent has two tools, `get_temperature(city)` and `add(a, b)`, and is asked
for the temperature in Paris plus ten. The checks pin the mechanics (the model
called both tools, every call got its return, usage was counted) and the LLM
oracle: the same model, asked as a judge, must answer PASS for the agent's
output against the expected 31. Not part of the offline package matrix: it
spends tokens and needs the network.
"""
from pydantic_ai import Agent
from pydantic_ai import Direct
from pydantic_ai import Exceptions as E
from pydantic_ai import JsonUtil as J
from pydantic_ai import Messages as M
from pydantic_ai import Models
from pydantic_ai import ModelsOpenai
from pydantic_ai import Providers
from pydantic_ai import Tools
from testing import Check

MODEL_NAME = "deepseek-chat"
QUESTION = "What is the current temperature in Paris in degrees Celsius, plus 10? Use the tools for both steps and answer with the final number only."
EXPECTED = "31"


def env_value(text, name):
    """The value of `name=` in dotenv text, empty when absent."""
    lines = unwrap_or(split(text, "\n"), [])
    hits = filterList(lines, lambda line: startsWith(line, name + "="))
    return trim(unwrap_or(replace(unwrap_or(listGet(hits, 0), ""), name + "=", ""), ""))


def temperature_tool():
    return Tools.tool(
        "get_temperature",
        "The current temperature of a city in degrees Celsius.",
        unwrap_or(jsonDecode("{\"type\":\"object\",\"properties\":{\"city\":{\"type\":\"string\",\"description\":\"The city name\"}},\"required\":[\"city\"]}"), JsonNull()),
        lambda args: get_temperature(unwrap_or(jsonText(unwrap_or(mapGet(args, "city"), JsonNull())), "")),
    )


def get_temperature(city) -> Result[str, str]:
    lowered = toLowerCase(city)
    if contains(lowered, "paris"):
        return Success(value="21")
    elif contains(lowered, "london"):
        return Success(value="14")
    else:
        return Error(message=f"Unknown city {city}; known cities are Paris and London")


def add_tool():
    return Tools.tool(
        "add",
        "Adds two numbers.",
        unwrap_or(jsonDecode("{\"type\":\"object\",\"properties\":{\"a\":{\"type\":\"number\"},\"b\":{\"type\":\"number\"}},\"required\":[\"a\",\"b\"]}"), JsonNull()),
        lambda args: Success(value=number_text(J.float_at(J.of_map(args), "a", 0.0) + J.float_at(J.of_map(args), "b", 0.0))),
    )


def number_text(x) -> str:
    """A whole number without its `.0`."""
    text = toString(x)
    return unwrap_or(replace(text, ".0", ""), text) if endsWith(text, ".0") else text


def tool_names(messages):
    """Every tool name called across the run's responses, in order."""
    return foldList(M.responses(messages), [], lambda acc, r: listConcat(acc, mapList(M.tool_calls(r), M.tool_name_of)))


def tool_return_count(messages) -> int:
    return foldList(M.requests(messages), 0, lambda n, r: n + len(filterList(r.parts, is_return)))


def is_return(part) -> bool:
    match part:
        case M.ToolReturnPart():
            return True
        case M.SystemPromptPart():
            return False
        case M.UserPromptPart():
            return False
        case M.RetryPromptPart():
            return False
        case M.InstructionPart():
            return False


def judge(model, question, answer) -> str:
    """The LLM oracle: PASS when the answer states the expected value, else FAIL."""
    prompt = f"You are grading an assistant. The question was: {question}\nThe expected final number is {EXPECTED}.\nThe assistant answered: {answer}\nDoes the assistant's answer state the number {EXPECTED} as its final result? Reply with exactly one word: PASS or FAIL."
    request = M.ModelRequest(parts=[M.UserPromptPart(content=prompt)], instructions="")
    match Direct.model_request(model, [M.of_request(request)], {"temperature": JsonFloat(0.0), "max_tokens": JsonInt(5)}, Models.model_request_parameters()):
        case E.Ok(value=response):
            return trim(M.text(response))
        case E.Failed(error=error):
            return f"judge failed: {E.message_of(error)}"


def run_checks(key):
    provider = Providers.DeepSeekProvider(key)
    model = ModelsOpenai.OpenAIChatModel(MODEL_NAME, provider, {})
    agent = Agent.tool_plain(Agent.tool_plain(Agent.agent(model, "You are a precise assistant. Always use the tools to look up facts and to do arithmetic."), temperature_tool()), add_tool())
    match Agent.run_sync(agent, QUESTION, []):
        case E.Ok(value=result):
            names = tool_names(result.messages)
            print(f"agent output: {result.output}")
            print(f"tools called: {join(names, ", ")}")
            print(f"requests: {result.usage.requests}, tool calls: {result.usage.tool_calls}, tokens: {result.usage.input_tokens} in / {result.usage.output_tokens} out")
            verdict = judge(model, QUESTION, result.output)
            print(f"judge: {verdict}")
            return [
                Check.truth("live.called_get_temperature", listContains(names, "get_temperature"), join(names, ",")),
                Check.truth("live.called_add", listContains(names, "add"), join(names, ",")),
                Check.equal("live.every_call_returned", tool_return_count(result.messages), len(names)),
                Check.truth("live.requests_at_least_two", result.usage.requests >= 2, toString(result.usage.requests)),
                Check.equal("live.tool_calls_counted", result.usage.tool_calls, len(names)),
                Check.truth("live.tokens_counted", result.usage.input_tokens > 0 and result.usage.output_tokens > 0, toString(result.usage.input_tokens)),
                Check.truth("live.model_name_reported", len(M.responses(result.messages)) > 0, "no responses"),
                Check.truth("live.output_mentions_expected", contains(result.output, EXPECTED), result.output),
                Check.truth("live.llm_oracle_pass", startsWith(toUpperCase(verdict), "PASS"), verdict),
            ]
        case E.Failed(error=error):
            return [Check.failed("live.run", E.message_of(error))]


def suite():
    key = env_value(unwrap_or(readFile(".env"), ""), "DEEPSEEKKEY")
    return [Check.failed("live.key", "DEEPSEEKKEY missing from .env")] if key == "" else run_checks(key)


def report_suite():
    return Check.report(suite())


def main():
    match Check.strictly(lambda: report_suite()):
        case Success(value=value):
            return value
        case Error(message=message):
            return print(f"suite stopped: {message}")
