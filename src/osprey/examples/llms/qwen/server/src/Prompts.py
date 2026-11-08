# osprey: flavor=python
# osprey: namespace=qwen_server
# osprey: module=Prompts
"""Render the GGUF chat template pinned in tests/chat_template.jinja [LLM-TEMPLATE].

The server keeps thinking within the active turn, drops older reasoning and
omits the generation prompt's trailing <think> line ([LLM-API-OLLAMA]).
"""

from qwen_server import Lists
from qwen_server import Wire

__all__ = ["render", "render_single", "turn", "tojson", "OPEN"]

OPEN = "<|im_start|>"
"""The frame every served message opens with."""
CLOSE = "<|im_end|>"
"""The frame every served message closes with."""
ASSISTANT = f"{OPEN}assistant\n"
"""The assistant prefix the prompt leaves for the model to continue."""
TOOLS_HEADER = "# Tools\n\nYou have access to the following functions:\n\n<tools>"
"""The template's tool-list heading, before the first tool's JSON."""
FORMAT = """

If you choose to call a function ONLY reply in the following format with NO suffix:

<tool_call>
<function=example_function_name>
<parameter=example_parameter_1>
value_1
</parameter>
<parameter=example_parameter_2>
This is the value for the second parameter
that can span
multiple lines
</parameter>
</function>
</tool_call>

<IMPORTANT>
Reminder:
- Function calls MUST follow the specified format: an inner <function=...></function> block must be nested within <tool_call></tool_call> XML tags
- Required parameters MUST be specified
- You may provide optional reasoning for your function call in natural language BEFORE the function call, but NOT after
- If there is no function call available, answer the question like normal with your current knowledge and do not tell the user about function calls
</IMPORTANT>"""
"""The template's call-format instructions, verbatim, after the closing `</tools>`."""
DIGITS = "0123456789abcdef"
"""The digits a code point's escape is spelled with, lowercase as jinja2 spells them."""
HTML_SAFE = [["<", "\\u003c"], [">", "\\u003e"], ["&", "\\u0026"], ["'", "\\u0027"]]
"""The characters jinja2's `tojson` escapes for HTML, and each one's escape."""


def render(messages, tools):
    """Render the chat or return the template's first refusal."""
    systems = leading_systems(messages, 0)
    start = Lists.count(systems)
    fault = history_fault(messages, start)
    if fault != "":
        return Error(message=fault)
    else:
        return Success(value=prompt(messages, tools, merged_text(systems), start))


def prompt(messages, tools, system, start):
    """The merged system turn, remaining messages and assistant prefix."""
    listed = Wire.items(tools)
    head = requested_system_turn(system) if Lists.count(listed) == 0 else tools_turn(listed, system)
    return head + turns(messages, start) + ASSISTANT


def render_single(prompt, system):
    """`/api/generate`'s single user turn, with an optional system turn before it."""
    return requested_system_turn(system) + turn(user_turn(prompt)) + ASSISTANT


def user_turn(prompt):
    """One `user` message carrying `prompt`."""
    return JsonObject([Wire.text_member("role", "user"), Wire.text_member("content", prompt)])


def requested_system_turn(system):
    """The system turn the request named, or nothing when it named none."""
    return "" if system == "" else turn(JsonObject([Wire.text_member("role", "system"), Wire.text_member("content", system)]))


def turn(message):
    """One rendered message, framed as the template frames it; a message without a role is a
    `user` turn. The content is trimmed: an answer that begins with newlines (as one does
    after `</think>`) would otherwise merge with the newline that ends the `assistant` prefix
    into one BPE token, so a history carrying that answer would no longer begin with the
    prompt's tokens and the cache checkpoint taken after the prompt could not be reused
    ([LLM-CACHE-CHECKPOINT])."""
    role = Wire.text(message, "role")
    content = trim(Wire.text(message, "content"))
    spelled = "user" if role == "" else role
    return f"{OPEN}{spelled}\n{content}{CLOSE}\n"


def turns(messages, start):
    count = Lists.count(messages)
    latest = fold(range(start, count), satSub(count, 1), lambda last, at: at if is_query(messages, at) else last)
    return fold(range(start, count), "", lambda rendered, at: rendered + turn_at(messages, at, latest))


def is_query(messages, at):
    match listGet(messages, at):
        case Success(value=message):
            content = trim(Wire.text(message, "content"))
            return Wire.text(message, "role") == "user" and not (startsWith(content, "<tool_response>") and endsWith(content, "</tool_response>"))
        case Error():
            return False


def turn_at(messages, at, latest):
    match listGet(messages, at):
        case Success(value=message):
            return message_turn(messages, at, message, at > latest)
        case Error():
            return ""


def message_turn(messages, at, message, active):
    role = Wire.text(message, "role")
    if role == "tool":
        return tool_turn(messages, at, message)
    elif role == "assistant":
        return assistant_turn(message, active)
    else:
        return turn(message)


# --- the tools system turn -------------------------------------------------


def tools_turn(tools, system):
    """Write the tool declarations and optional merged system text."""
    listed = tool_list(tools)
    tail = "" if system == "" else f"\n\n{system}"
    return f"{OPEN}system\n{TOOLS_HEADER}{listed}\n</tools>{FORMAT}{tail}{CLOSE}\n"


def tool_list(tools):
    """Each tool's `tojson`, on its own line under `<tools>`."""
    return foldList(tools, "", lambda rendered, tool: rendered + "\n" + tojson(tool))


def leading_systems(messages, at):
    """The leading `system`/`developer` messages from `at`, and none once another role appears."""
    match listGet(messages, at):
        case Success(value=message):
            return listConcat([message], leading_systems(messages, satAdd(at, 1))) if systemic(message) else []
        case Error():
            return []


def systemic(message):
    """Whether the message is a `system` or a `developer` message."""
    role = Wire.text(message, "role")
    return role == "system" or role == "developer"


def merged_text(systems):
    """The systems' trimmed contents joined by a newline, an empty one contributing nothing."""
    return foldList(systems, "", lambda merged, message: merged_message(merged, message))


def merged_message(merged, message):
    """The merged text with one more system message's trimmed content."""
    content = trim(Wire.text(message, "content"))
    if content == "":
        return merged
    elif merged == "":
        return content
    else:
        return f"{merged}\n{content}"


def calls_of(message):
    """The message's `tool_calls`, or none."""
    return Wire.items(Wire.field(message, "tool_calls"))


def assistant_turn(message, active):
    content = trim(Wire.text(message, "content"))
    reasoning = trim(Wire.text(message, "reasoning_content"))
    thinking = f"<think>\n{reasoning}\n</think>\n\n" if active else ""
    return f"{OPEN}assistant\n{thinking}{content}{call_blocks(calls_of(message), content)}{CLOSE}\n"


def call_blocks(calls, content):
    """Every call's `<tool_call>` block, the first following the turn's text."""
    return fold(range(Lists.count(calls)), "", lambda rendered, at: rendered + call_block(calls, at, content))


def call_block(calls, at, content):
    """One call's block: its frame and function name, its parameters, and its close."""
    match listGet(calls, at):
        case Success(value=call):
            params = parameters(arguments_of(call))
            return f"{call_head(at, content)}{call_name(call)}>\n{params}</function>\n</tool_call>"
        case Error():
            return ""


def call_head(at, content):
    """The frame before a call's name: a blank line after the turn's text, and one newline
    between calls."""
    if at > 0:
        return "\n<tool_call>\n<function="
    elif content != "":
        return "\n\n<tool_call>\n<function="
    else:
        return "<tool_call>\n<function="


def call_target(call):
    """The call's function object: the OpenAI shape's `function` member, or the flat call itself."""
    match Wire.field(call, "function"):
        case JsonNull():
            return call
        case function:
            return function


def call_name(call):
    """The name the call declares, written after `<function=`."""
    return Wire.text(call_target(call), "name")


def arguments_of(call):
    """The call's arguments: the object whose members are its parameters, or a string the
    template rejects."""
    return Wire.field(call_target(call), "arguments")


def parameters(arguments):
    """Every `<parameter>` block of a call's argument object, in the object's own order."""
    match arguments:
        case JsonObject(members):
            return foldList(members, "", lambda rendered, member: rendered + parameter(member))
        case _:
            return ""


def parameter(member):
    """One `<parameter=NAME>` block: a string verbatim, any other value as `tojson` writes it."""
    match member:
        case JsonMember(name, value):
            return f"<parameter={name}>\n{parameter_value(value)}\n</parameter>\n"


def parameter_value(value):
    """One parameter's value: a string as itself, anything else as `tojson` writes it."""
    match value:
        case JsonString(text):
            return text
        case other:
            return tojson(other)


# --- tool responses -------------------------------------------------------


def tool_turn(messages, at, message):
    """A `tool` message: its response inside the user turn that opens at the first of a run of
    consecutive tool messages, so the run shares one turn."""
    content = trim(Wire.text(message, "content"))
    head = "" if role_at(messages, satSub(at, 1)) == "tool" else f"{OPEN}user"
    close = "" if role_at(messages, satAdd(at, 1)) == "tool" else f"{CLOSE}\n"
    return f"{head}\n<tool_response>\n{content}\n</tool_response>{close}"


def role_at(messages, at):
    """The role of message `at`, and nothing outside the list."""
    match listGet(messages, at):
        case Success(value=message):
            return Wire.text(message, "role")
        case Error():
            return ""


# --- the refusals the template makes --------------------------------------


def history_fault(messages, start):
    """Find the template's first refusal after the leading system messages."""
    return fold(range(start, Lists.count(messages)), "", lambda fault, i: fault if fault != "" else fault_at(messages, i))


def fault_at(messages, index):
    match listGet(messages, index):
        case Success(value=message):
            return message_fault(message)
        case Error():
            return ""


def message_fault(message):
    if systemic(message):
        return "System message must be at the beginning."
    else:
        return calls_fault(calls_of(message)) if Wire.text(message, "role") == "assistant" else ""


def calls_fault(calls):
    """The refusal the first call the template rejects raises, or nothing."""
    return foldList(calls, "", lambda fault, call: fault if fault != "" else call_fault(call))


def call_fault(call):
    """The refusal one call raises: a missing function name, or arguments the template cannot
    write."""
    name = Wire.field(call_target(call), "name")
    match name:
        case JsonString(spelled):
            return arguments_fault(spelled, arguments_of(call))
        case _:
            return "Tool call is missing a function name."


def arguments_fault(name, arguments):
    """The refusal for one call's arguments, and nothing when the template renders them: an
    object is written parameter by parameter, a string is a fault an agent must parse first."""
    match arguments:
        case JsonString(text):
            return "" if trim(text) == "" else f"Tool call arguments for function \"{name}\" were passed as a JSON string. Parse them into an object before calling apply_chat_template."
        case JsonNull():
            return ""
        case JsonObject():
            return ""
        case _:
            return f"Tool call arguments for function \"{name}\" must be an object/mapping or a JSON string."


# --- jinja2's `tojson` ----------------------------------------------------


def tojson(value):
    """`value` as jinja2's `tojson` filter writes it: `json.dumps(value, sort_keys=True)` with
    the default `", "`/`": "` separators and `ensure_ascii=True`, then `<`, `>`, `&` and `'`
    replaced by their `\\uXXXX` escapes. Keys sort at every object level."""
    match value:
        case JsonString(text):
            return quote(text)
        case JsonArray(values):
            return f"[{join(mapList(values, lambda item: tojson(item)), ', ')}]"
        case JsonObject(members):
            return f"{{{join(mapList(sorted_members(members), lambda member: member_json(member)), ', ')}}}"
        case other:
            return scalar_json(other)


def scalar_json(value):
    """A null, a boolean or a number, in JSON's spelling."""
    match value:
        case JsonBool(flag):
            return "true" if flag else "false"
        case JsonInt(number):
            return toString(number)
        case JsonFloat(number):
            return toString(number)
        case _:
            return "null"


def sorted_members(members):
    """`members` in name order, as `sort_keys` writes them."""
    return listSortBy(members, lambda left, right: -1 if member_name(left) < member_name(right) else (1 if member_name(left) > member_name(right) else 0))


def member_name(member):
    """One member's name."""
    match member:
        case JsonMember(name, _value):
            return name


def member_json(member):
    """One member as `tojson` writes it: its name, `: `, its value."""
    match member:
        case JsonMember(name, value):
            return f"{quote(name)}: {tojson(value)}"


def quote(text):
    """`text` as `tojson` writes a string: quoted, ASCII only, and HTML-safe."""
    return html_safe(f"\"{escaped(text, 0)}\"")


class Escaping:
    """The JSON text of a string so far, and the continuation bytes of its last code point
    still to pass over."""
    text: str
    skip: int


def escaped(text, at):
    """The JSON text of `text`'s characters from byte `at` onward: a fold over the bytes, not
    a frame per character, since a tool's description can run to kilobytes and a frame per
    character overflowed the stack at about a thousand."""
    return fold(range(at, len(text)), Escaping(text="", skip=0), lambda state, index: escaped_at(text, index, state)).text


def escaped_at(text, at, state):
    """One byte's step: pass over a continuation byte, or write the code point that starts here."""
    if state.skip > 0:
        return Escaping(text=state.text, skip=satSub(state.skip, 1))
    else:
        match codePointAt(text, at):
            case Success(value=code_point):
                return Escaping(text=state.text + character(text, at, code_point), skip=satSub(width(code_point), 1))
            case Error():
                return Escaping(text=state.text + "\\ufffd", skip=0)


def width(code_point):
    """How many bytes of the text one code point occupies."""
    match codePointWidth(code_point):
        case Success(value=bytes_):
            return bytes_
        case Error():
            return 1


def character(text, at, code_point):
    """One character as JSON writes it: its own bytes where JSON leaves it as itself, and the
    escape it needs where it does not."""
    if code_point >= 32 and code_point <= 126 and code_point != 34 and code_point != 92:
        return take(drop(text, at), 1)
    else:
        return escape(code_point)


def escape(code_point):
    """The escape `code_point` needs: one of the short ones, or `\\uXXXX`."""
    if code_point == 34:
        return "\\\""
    elif code_point == 92:
        return "\\\\"
    elif code_point == 8:
        return "\\b"
    elif code_point == 9:
        return "\\t"
    elif code_point == 10:
        return "\\n"
    elif code_point == 12:
        return "\\f"
    elif code_point == 13:
        return "\\r"
    else:
        return unicode_escape(code_point)


def unicode_escape(code_point):
    """`\\uXXXX` for a code point in the BMP, and the surrogate pair JSON writes above it."""
    if code_point > 65535:
        offset = satSub(code_point, 65536)
        low = satSub(offset, satMul(intDiv(offset, 1024), 1024))
        return f"\\u{hex4(satAdd(55296, intDiv(offset, 1024)))}\\u{hex4(satAdd(56320, low))}"
    else:
        return f"\\u{hex4(code_point)}"


def hex4(value):
    """A code point's four lowercase hexadecimal digits."""
    return hex_digit(value, 4096) + hex_digit(value, 256) + hex_digit(value, 16) + hex_digit(value, 1)


def hex_digit(value, place):
    """`value`'s hexadecimal digit in `place`, one of 4096, 256, 16 and 1."""
    return take(drop(DIGITS, low_digit(intDiv(value, place))), 1)


def low_digit(number):
    """`number`'s low hexadecimal digit, 0 to 15."""
    return satSub(number, satMul(intDiv(number, 16), 16))


def html_safe(literal):
    """`literal` with the four characters jinja2's `tojson` escapes for HTML."""
    return foldList(HTML_SAFE, literal, lambda text, escape_pair: replaced(text, escape_pair))


def replaced(text, escape_pair):
    """`text` with the pair's character written as its escape."""
    return unwrap_or(replace(text, pair_at(escape_pair, 0), pair_at(escape_pair, 1)), text)


def pair_at(escape_pair, at):
    """One element of an HTML-safe pair: the character, or the escape that replaces it."""
    match listGet(escape_pair, at):
        case Success(value=element):
            return element
        case Error():
            return ""
