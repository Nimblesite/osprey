# osprey: flavor=python
# osprey: namespace=pydantic_ai
# osprey: module=Mcp
"""pydantic_ai.mcp as a sans-IO client: the MCP wire protocol as pure code.

Port of pydantic_ai/mcp.py (2.46.0), tools only. Upstream hands its wire
protocol to FastMCP and the MCP SDK; here JSON-RPC over stdio line framing and
streamable HTTP is ours, written from the MCP specification (protocol version
`PROTOCOL_VERSION`), so every function is pure and the application owns the
IO. Differences beside the transports: an `MCPError`'s `data` prints as JSON
rather than a Python dict, `load_mcp_toolsets` becomes `parse_config`, which
reports each broken entry beside the servers that parsed instead of raising on
the whole file, and image, audio and blob content becomes a description, never
bytes, because a tool returns text here. Resources, prompts, sampling and
elicitation are not ported.
"""
from pydantic_ai import JsonUtil as J
from pydantic_ai import Messages as M
from pydantic_ai import Tools

__all__ = [
    "PROTOCOL_VERSION", "MCPError", "message_of", "Implementation", "ServerCapabilities", "capabilities_of",
    "Session", "InitializeResult", "initialize_result", "initialize_request", "initialized_notification", "list_tools_request",
    "call_tool_request", "ping_request", "ping_response", "encode_line", "Incoming", "read_message",
    "LineBuffer", "Lines", "lines_of", "request_headers", "reply_messages", "Tool", "tool_of", "ToolList",
    "list_tools_result", "call_result", "tool_definition_of", "original_tool_name", "MCPServerConfig",
    "ConfigLoad", "parse_config", "merge_configs", "name_of",
]

PROTOCOL_VERSION = "2025-06-18"
"""The protocol version this client asks for and accepts."""


class MCPError:
    """An error response from an MCP server (pydantic_ai.mcp.MCPError; `data` is JSON here, `JsonNull` when none)."""
    message: str
    code: int
    data: Json


def message_of(error) -> str:
    """`str(MCPError)`: the message with its code, and the data when the server sent any."""
    if J.is_null(error.data):
        return f"{error.message} (code: {error.code})"
    else:
        return f"{error.message} (code: {error.code}, data: {J.encode(error.data)})"


class Implementation:
    """`mcp_types.Implementation`: the name, title and version one side stamps on itself."""
    name: str
    title: str
    version: str


class ServerCapabilities:
    """What a server supports, from `initialize`'s reply (pydantic_ai.mcp.ServerCapabilities)."""
    experimental: list[str]
    logging: bool
    prompts: bool
    prompts_list_changed: bool
    resources: bool
    resources_list_changed: bool
    tools: bool
    tools_list_changed: bool
    completions: bool


def present(value, name):
    """Whether a capability was sent at all: upstream reads each as `is not None`."""
    return not J.is_null(J.object_at(value, name))


def list_changed(value, name):
    """A capability object's `listChanged` flag, `False` when the capability or flag is absent."""
    return J.bool_at(J.object_at(value, name), "listChanged", False)


def capabilities_of(value) -> ServerCapabilities:
    """`ServerCapabilities.from_mcp_sdk`: a capability counts when its entry is present."""
    return ServerCapabilities(
        experimental=listSort(mapKeys(J.to_map(J.object_at(value, "experimental")))),
        logging=present(value, "logging"),
        prompts=present(value, "prompts"),
        prompts_list_changed=list_changed(value, "prompts"),
        resources=present(value, "resources"),
        resources_list_changed=list_changed(value, "resources"),
        tools=present(value, "tools"),
        tools_list_changed=list_changed(value, "tools"),
        completions=present(value, "completions"),
    )


class Session:
    """What `initialize` answered: the server's stamp, its capabilities, its instructions and its version."""
    server_info: Implementation
    capabilities: ServerCapabilities
    instructions: str
    protocol_version: str


@union
class InitializeResult:
    """`initialize_result`: the session, or the client's protocol error. Not upstream's shape — the
    port's `Result` error arm carries a string, so a structured `MCPError` failure is its own union."""
    class Ok:
        """The `initialize` reply as a `Session`."""
        session: Session
    class Failed:
        """The server asked for a protocol version this client cannot speak."""
        error: MCPError


def initialize_result(json) -> InitializeResult:
    """The `initialize` reply; a protocol version other than ours is an `MCPError`, as a client that
    cannot speak the server's version disconnects."""
    version = J.text_at(json, "protocolVersion", "")
    if version != PROTOCOL_VERSION:
        return Failed(error=MCPError(message=f"Server requested protocol version {version}, which this client does not support", code=-32602, data=JsonNull()))
    else:
        return Ok(session=Session(
            server_info=implementation_of(J.object_at(json, "serverInfo")),
            capabilities=capabilities_of(J.object_at(json, "capabilities")),
            instructions=J.text_at(json, "instructions", ""),
            protocol_version=version,
        ))


def implementation_of(value) -> Implementation:
    """The server's `serverInfo` stamp; every field falls back to empty when absent."""
    return Implementation(name=J.text_at(value, "name", ""), title=J.text_at(value, "title", ""), version=J.text_at(value, "version", ""))


def request(id, method, params):
    """One JSON-RPC request or notification: version, id, method and params."""
    return J.obj([J.member("jsonrpc", JsonString("2.0")), J.member("id", id), J.member("method", JsonString(method)), J.member("params", params)])


def initialize_request(id, client_name, client_version) -> Json:
    """`initialize`: our protocol version, no client capabilities, our stamp."""
    return request(id, "initialize", J.obj([
        J.member("protocolVersion", JsonString(PROTOCOL_VERSION)),
        J.member("capabilities", J.obj([])),
        J.member("clientInfo", J.obj([J.member("name", JsonString(client_name)), J.member("version", JsonString(client_version))])),
    ]))


def initialized_notification() -> Json:
    """`notifications/initialized`: the client's go-ahead after `initialize` answered."""
    return J.obj([J.member("jsonrpc", JsonString("2.0")), J.member("method", JsonString("notifications/initialized"))])


def list_tools_request(id, cursor) -> Json:
    """`tools/list`, carrying the previous page's `nextCursor` when paging."""
    params = J.obj([]) if cursor == "" else J.obj([J.member("cursor", JsonString(cursor))])
    return request(id, "tools/list", params)


def call_tool_request(id, name, arguments) -> Json:
    """`tools/call`: the tool's name and its arguments object."""
    return request(id, "tools/call", J.obj([J.member("name", JsonString(name)), J.member("arguments", J.of_map(arguments))]))


def ping_request(id) -> Json:
    """`ping`: the liveness probe a client may send."""
    return request(id, "ping", J.obj([]))


def ping_response(id) -> Json:
    """The empty result that answers a server's `ping`."""
    return J.obj([J.member("jsonrpc", JsonString("2.0")), J.member("id", id), J.member("result", J.obj([]))])


def encode_line(message) -> str:
    """One stdio frame: the JSON text of a message and its newline, none embedded."""
    return J.encode(message) + "\n"


@union
class Incoming:
    """One message read from a server, typed (never a crash on bad input)."""
    class ResponseOk:
        """A successful response to one of our requests."""
        id: Json
        result: Json
    class ResponseError:
        """An error response to one of our requests."""
        id: Json
        error: MCPError
    class ServerRequest:
        """A request the server sent us."""
        id: Json
        method: str
        params: Json
    class ServerNotification:
        """A notification the server sent us."""
        method: str
        params: Json
    class Malformed:
        """Text that is not a JSON-RPC message; `reason` says why."""
        reason: str


def read_message(text) -> Incoming:
    """One JSON-RPC message from text: a typed `Incoming` whatever the text says."""
    match J.decode(trim(text)):
        case Success(value=value):
            return read_value(value)
        case Error(message=message):
            return Malformed(reason=f"not JSON: {message}")


def read_value(value):
    match value:
        case JsonObject():
            return read_object(value)
        case _:
            return Malformed(reason="a message must be a JSON object")


def read_object(value):
    match J.object_at(value, "method"):
        case JsonString(method):
            return read_call(value, method)
        case JsonNull():
            return read_response(value)
        case _:
            return Malformed(reason="method must be a string")


def read_call(value, method):
    """A call from the server: a request when it carries an id, else a notification."""
    params = J.object_at(value, "params")
    match id_of(value):
        case Success(value=id):
            return ServerRequest(id=id, method=method, params=params)
        case Error():
            return ServerNotification(method=method, params=params)


def read_response(value):
    if J.has(value, "error"):
        return read_error(value, J.object_at(value, "error"))
    elif J.has(value, "result"):
        return respond(value, J.object_at(value, "result"))
    else:
        return Malformed(reason="a message needs a method, a result or an error")


def respond(value, result):
    match id_of(value):
        case Success(value=id):
            return ResponseOk(id=id, result=result)
        case Error(message=reason):
            return Malformed(reason=reason)


def read_error(value, error):
    match id_of(value):
        case Success(value=id):
            return error_response(id, error)
        case Error(message=reason):
            return Malformed(reason=reason)


def error_response(id, error):
    match J.object_at(error, "code"):
        case JsonInt(code):
            return ResponseError(id=id, error=MCPError(message=J.text_at(error, "message", ""), code=code, data=J.object_at(error, "data")))
        case _:
            return Malformed(reason="an error response needs an integer code")


def id_of(value) -> Result[Json, str]:
    """A JSON-RPC id: a string or a number, as JSON-RPC 2.0 allows."""
    id = J.object_at(value, "id")
    match id:
        case JsonInt():
            return Success(value=id)
        case JsonString():
            return Success(value=id)
        case _:
            return Error(message="id must be a string or a number")


class LineBuffer:
    """Stdio framing state: the text received since the last complete line."""
    pending: str


class Lines:
    """The lines a chunk completed, and the buffer the next chunk starts from."""
    lines: list[str]
    buffer: LineBuffer


def lines_of(buffer, chunk) -> Lines:
    """The complete lines `chunk` ends, keeping the remainder in the buffer across any split."""
    match split(buffer.pending + chunk, "\n"):
        case Success(value=pieces):
            return Lines(lines=mapList(first_items(pieces, len(pieces) - 1), without_cr), buffer=LineBuffer(pending=unwrap_or(listGet(pieces, len(pieces) - 1), "")))
        case Error():
            return Lines(lines=[], buffer=LineBuffer(pending=buffer.pending + chunk))


def first_items(items, n) -> list:
    """The first `n` items of a list."""
    return foldList(items, [], lambda acc, item: acc if len(acc) >= n else listAppend(acc, item))


def without_cr(line):
    """A line's text with a trailing CR, as CRLF framing ends lines, dropped."""
    return take(line, length(line) - 1) if endsWith(line, "\r") else line


def request_headers(session_id, extra) -> str:
    """The headers a streamable-HTTP request carries: both accept types, our protocol version,
    the session once the server assigned one, then `extra` (authentication, content type)."""
    session = "" if session_id == "" else f"\r\nMcp-Session-Id: {session_id}"
    extra_headers = "" if extra == "" else f"\r\n{extra}"
    return f"Accept: application/json, text/event-stream\r\nMCP-Protocol-Version: {PROTOCOL_VERSION}{session}{extra_headers}"


def reply_messages(content_type, body) -> list[Incoming]:
    """The messages one HTTP reply carries: an event stream holds many, JSON one, an empty body none."""
    if contains(content_type, "text/event-stream"):
        return sse_messages(body)
    elif trim(body) == "":
        return []
    else:
        return [read_message(body)]


class SseFold:
    """SSE scan state: the data lines of the event being read and the messages read."""
    data: list[str]
    messages: list[Incoming]


def sse_messages(body):
    folded = foldList(line_pieces(body), SseFold(data=[], messages=[]), sse_step)
    return close_sse(folded).messages


def sse_step(acc, line):
    """One SSE line: a blank line dispatches, a comment or field but `data` is kept, `data` joins."""
    if line == "":
        return close_sse(acc)
    elif startsWith(line, ":"):
        return acc
    elif startsWith(line, "data:"):
        return SseFold(data=listAppend(acc.data, drop_after_colon(drop(line, length("data:")))), messages=acc.messages)
    else:
        return acc


def close_sse(acc) -> SseFold:
    """The event being read, dispatched: a stream's trailing data is delivered without a blank line."""
    if len(acc.data) == 0:
        return SseFold(data=[], messages=acc.messages)
    else:
        return SseFold(data=[], messages=listAppend(acc.messages, read_message(join(acc.data, "\n"))))


def drop_after_colon(text):
    """A `data:` payload: one leading space after the colon is not part of it (SSE)."""
    return drop(text, 1) if startsWith(text, " ") else text


def line_pieces(text):
    """The lines of a text, CRLF endings dropped, a trailing line without newline included."""
    match split(text, "\n"):
        case Success(value=pieces):
            return mapList(pieces, without_cr)
        case Error():
            return [text]


class Tool:
    """`mcp_types.Tool`: a tool's name, description and input schema, as the server listed it."""
    name: str
    description: str
    input_schema: Json


def tool_of(json) -> Result[Tool, str]:
    """One `tools/list` entry; `Error` names why an entry cannot be used."""
    name = J.text_at(json, "name", "")
    schema = J.object_at(json, "inputSchema")
    if name == "":
        return Error(message="a tool needs a name")
    else:
        match schema:
            case JsonObject():
                return Success(value=Tool(name=name, description=J.text_at(json, "description", ""), input_schema=schema))
            case _:
                return Error(message=f"tool '{name}' needs an inputSchema object")


class ToolList:
    """A `tools/list` result: the tools, the next page's cursor, and a problem per bad entry."""
    tools: list[Tool]
    next_cursor: str
    problems: list[str]


def list_tools_result(json) -> ToolList:
    """The tools a `tools/list` result lists; a bad entry is skipped and reported, never fatal."""
    return foldList(J.list_at(json, "tools"), ToolList(tools=[], next_cursor=J.text_at(json, "nextCursor", ""), problems=[]), tool_entry)


def tool_entry(listing, entry) -> ToolList:
    match tool_of(entry):
        case Success(value=tool):
            return ToolList(tools=listAppend(listing.tools, tool), next_cursor=listing.next_cursor, problems=listing.problems)
        case Error(message=message):
            return ToolList(tools=listing.tools, next_cursor=listing.next_cursor, problems=listAppend(listing.problems, message))


def call_result(name, json) -> Result[str, str]:
    """A `tools/call` result as the text a tool returns, or the `ModelRetry` message a failed one
    raises (upstream's default `tool_error_behavior='retry'`)."""
    structured = J.object_at(json, "structuredContent")
    parts = J.list_at(json, "content")
    failed = J.bool_at(json, "isError", False)
    if not failed:
        return Success(value=mapped_result(structured, parts, failed))
    elif J.is_null(structured) and len(parts) == 0:
        return Error(message=f"Tool '{name}' returned an error")
    else:
        return Error(message=mapped_result(structured, parts, failed))


def mapped_result(structured, parts, failed):
    """`_map_mcp_call_tool_result`: structured content wins when every part is text or the call
    failed, a lone `result` key unwrapped; otherwise the content parts map and join."""
    if not J.is_null(structured) and (failed or all_text(parts)):
        return J.encode(unwrap_result_key(structured))
    else:
        return join(mapList(parts, call_part_text), "\n")


def unwrap_result_key(structured):
    """The SDK wraps a primitive in a lone `result` key; the raw value is what the tool returned."""
    members = J.members_of(structured)
    first = unwrap_or(listGet(members, 0), JsonMember("", JsonNull()))
    if len(members) == 1 and J.member_name(first) == "result":
        return J.member_value(first)
    else:
        return structured


def all_text(parts):
    """Whether every content block is a text block (an empty list is all text, as `all` is)."""
    return foldList(parts, True, lambda acc, part: acc and J.text_at(part, "type", "") == "text")


def call_part_text(part):
    """One content block as text (`_map_mcp_tool_result`; bytes stay a description, never sent)."""
    kind = J.text_at(part, "type", "")
    if kind == "text":
        return text_block(J.text_at(part, "text", ""))
    elif kind == "image":
        return described("image", J.text_at(part, "mimeType", ""), J.text_at(part, "data", ""))
    elif kind == "audio":
        return described("audio", J.text_at(part, "mimeType", ""), J.text_at(part, "data", ""))
    elif kind == "resource_link":
        return J.text_at(part, "uri", "")
    elif kind == "resource":
        return resource_text(J.object_at(part, "resource"))
    else:
        return f"[unsupported content type: {kind}]"


def text_block(text):
    """A text block: re-encoded when it spells a JSON value, as upstream parses `[`/`{` text back."""
    if startsWith(text, "[") or startsWith(text, "{"):
        match J.decode(text):
            case Success(value=value):
                return J.encode(value)
            case Error():
                return text
    else:
        return text


def described(kind, mime_type, base64):
    """An image, audio or blob block as a description: a tool returns text, not bytes, here."""
    return f"[{kind}: {mime_type}, {byte_size(base64)} bytes]"


def byte_size(base64):
    """The byte count base64 text carries: three per four characters, less its padding."""
    if endsWith(base64, "=="):
        return 3 * (length(base64) // 4) - 2
    elif endsWith(base64, "="):
        return 3 * (length(base64) // 4) - 1
    else:
        return 3 * (length(base64) // 4)


def resource_text(resource):
    """An embedded resource (`EmbeddedResource`): its text, or a description of its blob."""
    match J.object_at(resource, "text"):
        case JsonString(text):
            return text
        case _:
            return described("resource", or_default(J.text_at(resource, "mimeType", ""), "application/octet-stream"), J.text_at(resource, "blob", ""))


def or_default(text, fallback):
    return fallback if text == "" else text


def tool_definition_of(prefix, tool) -> Tools.ToolDefinition:
    """The definition the model sees, named `f'{prefix}_{name}'` as upstream's `PrefixedToolset` names it."""
    return Tools.tool_definition(f"{prefix}_{tool.name}", tool.description, tool.input_schema)


def original_tool_name(prefix, name) -> str:
    """`str.removeprefix(prefix + '_')`: the tool name a prefixed call names."""
    return drop(name, length(prefix) + 1) if startsWith(name, f"{prefix}_") else name


@union
class MCPServerConfig:
    """One `mcpServers` entry: how to reach one server (`load_mcp_toolsets`'s shapes)."""
    class StdioServer:
        """A server started as a child process (`StdioTransport`)."""
        name: str
        command: str
        args: list[str]
        env: dict[str, str]
        cwd: str
    class HttpServer:
        """A server reached over streamable HTTP."""
        name: str
        url: str
        headers: dict[str, str]


class ConfigLoad:
    """A config file's servers and the problems found on the way, one per broken entry."""
    servers: list[MCPServerConfig]
    problems: list[str]


def parse_config(text, environment) -> ConfigLoad:
    """`load_mcp_toolsets`'s `mcpServers` shape: every server that parses, and one problem per
    broken entry, so a window can show each one (upstream raises on the whole file)."""
    match J.decode(text):
        case Success(value=value):
            return servers_of(value, environment)
        case Error(message=message):
            return ConfigLoad(servers=[], problems=[f"not valid JSON: {message}"])


def servers_of(value, environment) -> ConfigLoad:
    match J.object_at(value, "mcpServers"):
        case JsonObject():
            members = J.members_of(J.object_at(value, "mcpServers"))
            return foldList(members, ConfigLoad(servers=[], problems=[]), lambda acc, member: entry_step(acc, environment, member))
        case _:
            return ConfigLoad(servers=[], problems=["the configuration must have an mcpServers object"])


def entry_step(acc, environment, member):
    name = J.member_name(member)
    match J.member_value(member):
        case JsonObject():
            return expand_entry(acc, environment, name, J.member_value(member))
        case _:
            return problem(acc, f"MCP server config '{name}' must be an object")


def expand_entry(acc, environment, name, value):
    match expand_value(value, environment):
        case Success(value=expanded):
            return server_of(acc, name, expanded)
        case Error(message=message):
            return problem(acc, f"MCP server config '{name}': {message}")


def server_of(acc, name, value) -> ConfigLoad:
    """One server entry: `command` wins over `url`, as upstream checks `'command' in server` first; a
    JSON null counts as omission, so an entry carrying neither transport is the error naming the server."""
    if is_string(value, "command"):
        server = StdioServer(name=name, command=J.text_at(value, "command", ""), args=text_list_at(value, "args"), env=text_map_at(value, "env"), cwd=J.text_at(value, "cwd", ""))
        return ConfigLoad(servers=listAppend(acc.servers, server), problems=acc.problems)
    elif is_string(value, "url"):
        server = HttpServer(name=name, url=J.text_at(value, "url", ""), headers=text_map_at(value, "headers"))
        return ConfigLoad(servers=listAppend(acc.servers, server), problems=acc.problems)
    else:
        return problem(acc, f"MCP server config '{name}' must have either 'command' or 'url'")


def is_string(value, name):
    """Whether a member is present as a string; an absent or null member is omission, not a string."""
    match jsonField(value, name):
        case Success(value=field):
            match field:
                case JsonString(_text):
                    return True
                case _:
                    return False
        case Error(message=_m):
            return False


def problem(acc, text) -> ConfigLoad:
    return ConfigLoad(servers=acc.servers, problems=listAppend(acc.problems, text))


def text_list_at(value, name):
    """`value[name]` as the string elements of an array; a null or absent field is no elements."""
    return foldList(J.list_at(value, name), [], text_list_step)


def text_list_step(acc, item):
    match item:
        case JsonString(text):
            return listAppend(acc, text)
        case _:
            return acc


def text_map_at(value, name):
    """`value[name]` as a string map; a null or absent field is an empty map."""
    return foldList(J.members_of(J.object_at(value, name)), {}, text_map_step)


def text_map_step(acc, member):
    match J.member_value(member):
        case JsonString(text):
            return mapSet(acc, J.member_name(member), text)
        case _:
            return acc


def merge_configs(global_servers, project_servers) -> list[MCPServerConfig]:
    """The global servers then the project ones, a project entry winning a name clash."""
    taken = mapList(project_servers, name_of)
    return listConcat(filterList(global_servers, lambda server: not listContains(taken, name_of(server))), project_servers)


def name_of(config) -> str:
    match config:
        case StdioServer(name=name):
            return name
        case HttpServer(name=name):
            return name


@union
class Reference:
    """One `${...}` in a config string, as upstream's expansion pattern reads it."""
    class Kept:
        """A shape the pattern does not match, kept as written."""
        inner: str
    class Variable:
        """`${VAR}`."""
        name: str
    class WithDefault:
        """`${VAR:-default}`."""
        name: str
        default: str


def expand_value(value, environment):
    """`${VAR}` and `${VAR:-default}` expanded over every string, recursively (upstream
    `_expand_env_vars`); `Error` names the first undefined variable with no default."""
    match value:
        case JsonString(text):
            return expand_string(text, environment)
        case JsonObject(members):
            return expand_members(members, environment)
        case JsonArray(items):
            return expand_items(items, environment)
        case _:
            return Success(value=value)


def expand_string(text, environment) -> Result[Json, str]:
    match expand_text(text, environment):
        case Success(value=expanded):
            return Success(value=JsonString(expanded))
        case Error(message=message):
            return Error(message=message)


def expand_members(members, environment):
    match M.collect(members, lambda member: expand_member(member, environment)):
        case Success(value=expanded):
            return Success(value=JsonObject(expanded))
        case Error(message=message):
            return Error(message=message)


def expand_member(member, environment) -> Result[JsonMember, str]:
    match expand_value(J.member_value(member), environment):
        case Success(value=value):
            return Success(value=JsonMember(J.member_name(member), value))
        case Error(message=message):
            return Error(message=message)


def expand_items(items, environment):
    match M.collect(items, lambda item: expand_value(item, environment)):
        case Success(value=expanded):
            return Success(value=JsonArray(expanded))
        case Error(message=message):
            return Error(message=message)


def expand_text(text, environment):
    match indexOf(text, "${"):
        case Error():
            return Success(value=text)
        case Success(value=start):
            return expand_at(text, start, environment)


def expand_at(text, start, environment):
    rest = drop(text, start + 2)
    match indexOf(rest, "}"):
        case Error():
            return Success(value=text)
        case Success(value=end):
            return expand_rest(text, start, take(rest, end), drop(rest, end + 1), environment)


def expand_rest(text, start, inner, tail, environment) -> Result[str, str]:
    match expand_reference(inner, environment):
        case Success(value=replacement):
            match expand_text(tail, environment):
                case Success(value=expanded):
                    return Success(value=take(text, start) + replacement + expanded)
                case Error(message=message):
                    return Error(message=message)
        case Error(message=message):
            return Error(message=message)


def expand_reference(inner, environment):
    match reference_of(inner):
        case Kept(inner=kept):
            return Success(value="${" + kept + "}")
        case Variable(name=name):
            return required(environment, name)
        case WithDefault(name=name, default=default):
            return Success(value=unwrap_or(mapGet(environment, name), default))


def reference_of(inner):
    match indexOf(inner, ":"):
        case Error():
            return Variable(name=inner) if inner != "" else Kept(inner=inner)
        case Success(value=at):
            return defaulted(inner, at)


def defaulted(inner, at):
    if at == 0 or not startsWith(drop(inner, at), ":-"):
        return Kept(inner=inner)
    else:
        return WithDefault(name=take(inner, at), default=drop(inner, at + 2))


def required(environment, name) -> Result[str, str]:
    match mapGet(environment, name):
        case Success(value=value):
            return Success(value=value)
        case Error():
            return Error(message=f"Environment variable ${{{name}}} is not defined")
