# osprey: flavor=python
# osprey: namespace=agent
# osprey: module=McpClient
"""The agent's MCP boundary: standard configuration documents, validated form
values and the sans-IO protocol helpers the application state machine uses.

Only strings, numbers, booleans and Json cross into the Osprey-flavored half
of the application: MCP's union values (`MCPServerConfig`, `Incoming`) stay
inside this module, serialized as JSON text, because a Python-flavor union
stored in an Osprey record miscompiles today. No application environment is
read here, so `${VAR}` can never expand to an application secret. Secrets are
serialized only into the private `mcp.json` document this module builds.
"""
from pydantic_ai import JsonUtil as J
from pydantic_ai import Mcp

__all__ = [
    "parse_config", "config_json", "config_value", "form_problem", "form_server", "form_config", "valid_url",
    "Save", "add_global", "paste_global", "remove_global_text", "Merged", "ServerView", "merge_view",
    "ReadStep", "read_step", "message_json", "find_response", "initialize_outcome", "tools_page", "call_text_of",
    "ping_replies", "header_value", "http_messages", "start_brief", "arguments_of", "Wire", "initialize_line",
    "initialized_line", "list_tools_line", "call_line", "protocol_version", "bare_tool",
]


def parse_config(text):
    """Parses the standard `mcpServers` shape with no host environment."""
    empty: dict[str, str] = {}
    return Mcp.parse_config(text, empty)


def json_map(values):
    """A string map as a JSON string map, in key order."""
    empty: dict[str, Json] = {}
    return foldList(mapKeys(values), empty, lambda acc, key: mapSet(acc, key, JsonString(unwrap_or(mapGet(values, key), ""))))


def config_value(config):
    """One serializable server entry, including its private values."""
    match config:
        case Mcp.StdioServer(command=command, args=args, env=env, cwd=cwd):
            members = [J.member("command", JsonString(command)), J.member("args", JsonArray(mapList(args, lambda text: JsonString(text)))), J.member("env", J.of_map(json_map(env)))]
            return J.obj(members if cwd == "" else listAppend(members, J.member("cwd", JsonString(cwd))))
        case Mcp.HttpServer(url=url, headers=headers):
            return J.obj([J.member("url", JsonString(url)), J.member("headers", J.of_map(json_map(headers)))])


def config_json(servers):
    """The standard private `mcp.json` document."""
    return J.encode(J.obj([J.member("mcpServers", J.obj(mapList(servers, lambda server: J.member(Mcp.name_of(server), config_value(server)))))]))


class Save:
    """A document that changed, or the words saying why it could not."""
    text: str
    problem: str


def saved(text):
    return Save(text=text, problem="")


def refused(problem):
    return Save(text="", problem=problem)


def servers_member(text):
    """The `mcpServers` object of a document, or an empty object when unreadable."""
    match J.decode(text):
        case Success(value=value):
            return J.object_at(value, "mcpServers")
        case Error():
            return J.obj([])


def with_entry(text, name, entry):
    """A document with one more server entry under `name`."""
    members = listAppend(J.members_of(servers_member(text)), J.member(name, entry))
    return J.encode(J.obj([J.member("mcpServers", J.obj(members))]))


def names_of(text):
    """Every server name a document configures."""
    return mapList(J.members_of(servers_member(text)), lambda member: J.member_name(member))


def add_global(global_text, name, entry_json):
    """`mcp.json` with one more server, or a naming problem."""
    match J.decode(entry_json):
        case Success(value=entry):
            taken = names_of(global_text)
            if listContains(taken, name):
                return refused(f"An MCP server named '{name}' already exists.")
            else:
                return saved(with_entry(global_text, name, entry))
        case Error(message=message):
            return refused(f"The server entry is not valid JSON: {message}")


def paste_problem(problems, clash):
    """Why a pasted block cannot be added, in words; empty when it can. A name repeated inside one JSON object never reaches here, because JSON decoding rejects duplicate members."""
    if len(problems) > 0:
        return join(problems, "\n")
    elif clash != "":
        return f"An MCP server named '{clash}' already exists."
    else:
        return ""


def paste_global(global_text, pasted):
    """`mcp.json` with every server a pasted block configures, or one problem naming the first fault. Atomic: nothing is added when anything is wrong."""
    loaded = parse_config(pasted)
    existing = names_of(global_text)
    pasted_names = mapList(loaded.servers, lambda server: Mcp.name_of(server))
    clash = unwrap_or(listGet(filterList(pasted_names, lambda name: listContains(existing, name)), 0), "")
    problem = paste_problem(loaded.problems, clash)
    added = foldList(loaded.servers, global_text, lambda acc, server: with_entry(acc, Mcp.name_of(server), config_value(server)))
    return Save(text=(added if problem == "" else ""), problem=problem)


def remove_global_text(global_text, name):
    """`mcp.json` without one server."""
    members = filterList(J.members_of(servers_member(global_text)), lambda member: J.member_name(member) != name)
    return J.encode(J.obj([J.member("mcpServers", J.obj(members))]))


class ServerView:
    """One merged server as the application stores it: primitives only."""
    viewName: str
    viewOrigin: str
    viewKind: str
    viewEndpoint: str
    viewConfig: str
    viewEnabled: bool


def view_of(server, origin):
    match server:
        case Mcp.StdioServer(command=command, args=args):
            words = join(listPrepend(args, command), " ")
            return ServerView(viewName=Mcp.name_of(server), viewOrigin=origin, viewKind="command", viewEndpoint=words, viewConfig=J.encode(config_value(server)), viewEnabled=origin != "project")
        case Mcp.HttpServer(url=url):
            return ServerView(viewName=Mcp.name_of(server), viewOrigin=origin, viewKind="url", viewEndpoint=url, viewConfig=J.encode(config_value(server)), viewEnabled=origin != "project")


class Merged:
    """Both files' servers, a project entry winning a name clash, and each file's problems."""
    mergedServers: list[ServerView]
    mergedProblems: list[str]


def entry_problem(server):
    """Why a server that parsed cannot run here, empty when it can."""
    match server:
        case Mcp.StdioServer(name=name, command=command, cwd=cwd):
            if trim(command) == "":
                return f"MCP server config '{name}' needs a command"
            elif cwd != "":
                return f"MCP server config '{name}' sets cwd, which this app does not support"
            else:
                return ""
        case Mcp.HttpServer(name=name, url=url):
            return "" if valid_url(url) else f"MCP server config '{name}' needs an HTTPS URL, or an HTTP URL on this machine"


def file_problems(label, load):
    """One file's problems in words, those parsing found and those of the entries it could not run."""
    unusable = filterList(mapList(load.servers, entry_problem), lambda words: words != "")
    return mapList(listConcat(load.problems, unusable), lambda problem: f"{label}: {problem}")


def merge_view(global_text, project_text):
    """The servers both documents configure, as primitives, with their problems in words."""
    globe = parse_config(global_text)
    project = parse_config(project_text)
    merged = Mcp.merge_configs(filterList(globe.servers, lambda server: entry_problem(server) == ""), filterList(project.servers, lambda server: entry_problem(server) == ""))
    views = mapList(merged, lambda server: view_of(server, "project" if listContains(mapList(project.servers, lambda other: Mcp.name_of(other)), Mcp.name_of(server)) else "global"))
    words = listConcat(file_problems("mcp.json", globe), file_problems(".mcp.json", project))
    return Merged(mergedServers=views, mergedProblems=words)


def nonempty_lines(text):
    """Nonempty trimmed lines, used for one argument per line."""
    return filterList(mapList(unwrap_or(split(text, "\n"), []), trim), lambda line: line != "")


def pairs_of(text, label):
    """`NAME=value` lines as a string map, or a plain validation problem."""
    return foldList(nonempty_lines(text), Success(value={}), lambda acc, line: pair_step(acc, line, label))


def pair_step(acc, line, label):
    match acc:
        case Error():
            return acc
        case Success(value=values):
            match indexOf(line, "="):
                case Success(value=at):
                    name = trim(take(line, at))
                    return Success(value=mapSet(values, name, drop(line, at + 1))) if name != "" else Error(message=f"{label} lines must look like NAME=value.")
                case Error():
                    return Error(message=f"{label} lines must look like NAME=value.")


def valid_url(url):
    """HTTPS anywhere, or plain HTTP only to a loopback test/local server."""
    if contains(url, " ") or contains(url, "\n") or contains(url, "\t"):
        return False
    elif startsWith(url, "https://"):
        return length(drop(url, 8)) > 0
    else:
        hosts = ["http://localhost", "http://127.0.0.1", "http://[::1]"]
        return foldList(hosts, False, lambda valid, host: valid or loopback_url(url, host))


def loopback_url(url, host):
    """The prefix must end at the host, not at `localhost.attacker`."""
    rest = drop(url, length(host))
    return startsWith(url, host) and (rest == "" or startsWith(rest, ":") or startsWith(rest, "/"))


def form_problem(name, kind, command, url, environment, headers):
    """The first problem in an Add server form, empty when it is valid."""
    if trim(name) == "":
        return "Enter a server name."
    elif kind == "command" and trim(command) == "":
        return "Enter a command."
    elif kind == "url" and not valid_url(trim(url)):
        return "Enter an HTTPS URL, or an HTTP URL on this machine."
    else:
        match pairs_of(environment, "Environment"):
            case Error(message=problem):
                return problem
            case Success():
                match pairs_of(headers, "Header"):
                    case Success():
                        return ""
                    case Error(message=problem):
                        return problem


def form_config(name, kind, command, arguments, url, environment, headers):
    """A validated form as the server entry `mcp.json` holds, or the words saying why not."""
    problem = form_problem(name, kind, command, url, environment, headers)
    if problem != "":
        return refused(problem)
    elif kind == "url":
        match pairs_of(headers, "Header"):
            case Success(value=values):
                return saved(J.encode(J.obj([J.member("url", JsonString(trim(url))), J.member("headers", J.of_map(json_map(values)))])))
            case Error(message=words):
                return refused(words)
    else:
        match pairs_of(environment, "Environment"):
            case Success(value=values):
                return saved(J.encode(J.obj([J.member("command", JsonString(trim(command))), J.member("args", JsonArray(mapList(nonempty_lines(arguments), lambda text: JsonString(text)))), J.member("env", J.of_map(json_map(values)))])))
            case Error(message=words):
                return refused(words)


def form_server(name, kind, command, arguments, url, environment, headers):
    """A validated form as one typed server config, kept for the host-free suite."""
    entry = form_config(name, kind, command, arguments, url, environment, headers)
    if entry.problem != "":
        return Error(message=entry.problem)
    else:
        single = J.encode(J.obj([J.member("mcpServers", J.obj([J.member(trim(name), unwrap_or(J.decode(entry.text), JsonNull()))]))]))
        servers = parse_config(single).servers
        return Error(message="the form did not parse") if len(servers) == 0 else Success(value=servers[0])


def start_brief(entry_json):
    """How to reach one server, as JSON: a child process with its environment, or a URL with its headers."""
    match J.decode(entry_json):
        case Success(value=entry):
            match entry:
                case JsonObject():
                    return stdio_brief(entry) if J.has(entry, "command") else http_brief(entry)
                case _:
                    return "{\"kind\":\"unsupported\"}"
        case Error():
            return "{\"kind\":\"unsupported\"}"


def pair_items(values):
    return mapList(mapKeys(values), lambda key: J.obj([J.member("name", JsonString(key)), J.member("value", JsonString(unwrap_or(mapGet(values, key), "")))]))


def stdio_brief(entry):
    command = J.text_at(entry, "command", "")
    if command == "":
        return "{\"kind\":\"unsupported\",\"problem\":\"The server entry needs a command.\"}"
    else:
        env = pair_items(text_map(entry, "env"))
        return J.encode(J.obj([J.member("kind", JsonString("stdio")), J.member("command", JsonString(command)), J.member("args", JsonArray(J.list_at(entry, "args"))), J.member("env", JsonArray(env))]))


def http_brief(entry):
    url = J.text_at(entry, "url", "")
    if url == "":
        return "{\"kind\":\"unsupported\",\"problem\":\"The server entry needs a URL.\"}"
    else:
        return J.encode(J.obj([J.member("kind", JsonString("http")), J.member("url", JsonString(url)), J.member("headers", JsonArray(pair_items(text_map(entry, "headers"))))]))


def text_map(value, name):
    """`value[name]` as a string map; a null or absent field is an empty map."""
    return foldList(J.members_of(J.object_at(value, name)), {}, lambda acc, member: text_map_step(acc, member))


def text_map_step(acc, member):
    match J.member_value(member):
        case JsonString(text):
            return mapSet(acc, J.member_name(member), text)
        case _:
            return acc


def read_step(buffer, chunk):
    """The complete lines a chunk of standard output ends, and the buffer the next chunk starts from."""
    lines = Mcp.lines_of(Mcp.LineBuffer(pending=buffer), chunk)
    return ReadStep(lines=lines.lines, buffer=lines.buffer.pending)


class ReadStep:
    """Stdio framing over one read: the lines it completed and the remainder."""
    lines: list[str]
    buffer: str


def message_json(line):
    """One line from a server as tagged JSON text, so no protocol union crosses the flavor boundary."""
    return J.encode(tag_of(Mcp.read_message(line)))


def tag_of(incoming):
    match incoming:
        case Mcp.ResponseOk(id=ident, result=result):
            return J.obj([J.member("kind", JsonString("ok")), J.member("id", ident), J.member("result", result)])
        case Mcp.ResponseError(id=ident, error=error):
            return J.obj([J.member("kind", JsonString("error")), J.member("id", ident), J.member("message", JsonString(error.message)), J.member("code", JsonInt(error.code))])
        case Mcp.ServerRequest(id=ident, method=method, params=params):
            return J.obj([J.member("kind", JsonString("request")), J.member("id", ident), J.member("method", JsonString(method)), J.member("params", params)])
        case Mcp.ServerNotification(method=method):
            return J.obj([J.member("kind", JsonString("notification")), J.member("method", JsonString(method))])
        case Mcp.Malformed(reason=reason):
            return J.obj([J.member("kind", JsonString("malformed")), J.member("reason", JsonString(reason))])


def decode_messages(texts):
    """Each message text as its decoded tagged document; an undecodable one reads as an empty object."""
    return mapList(texts, lambda text: unwrap_or(J.decode(text), JsonNull()))


def id_text(message):
    """A message's JSON-RPC id as JSON text, so numbers and strings compare alike."""
    return J.encode(J.object_at(message, "id"))


def find_response(texts, wanted):
    """The response message whose id matches `wanted` as JSON text, or `{}` when there is none."""
    found = filterList(decode_messages(texts), lambda message: (J.text_at(message, "kind", "") == "ok" or J.text_at(message, "kind", "") == "error") and id_text(message) == wanted)
    return J.encode(unwrap_or(listGet(found, 0), J.obj([])))


def initialize_outcome(text):
    """The `initialize` reply as `{"ok", "problem"}`: our protocol version, or the server's refusal in words."""
    message = unwrap_or(J.decode(text), JsonNull())
    match J.text_at(message, "kind", ""):
        case "error":
            return J.encode(J.obj([J.member("ok", JsonBool(False)), J.member("problem", JsonString(error_words(message)))]))
        case _:
            match Mcp.initialize_result(J.object_at(message, "result")):
                case Mcp.Ok(session=session):
                    return J.encode(J.obj([J.member("ok", JsonBool(True)), J.member("problem", JsonString(""))]))
                case Mcp.Failed(error=error):
                    return J.encode(J.obj([J.member("ok", JsonBool(False)), J.member("problem", JsonString(Mcp.message_of(error)))]))


def error_words(message):
    return f"{J.text_at(message, 'message', '')} (code: {J.int_at(message, 'code', 0)})"


def tools_page(text):
    """A `tools/list` reply as `{"tools", "cursor", "problems"}`."""
    message = unwrap_or(J.decode(text), JsonNull())
    match J.text_at(message, "kind", ""):
        case "error":
            return J.encode(J.obj([J.member("tools", JsonArray([])), J.member("cursor", JsonString("")), J.member("problems", JsonArray([JsonString(error_words(message))]))]))
        case _:
            listing = Mcp.list_tools_result(J.object_at(message, "result"))
            tools = JsonArray(foldList(listing.tools, [], lambda acc, tool: listAppend(acc, J.obj([J.member("name", JsonString(tool.name)), J.member("description", JsonString(tool.description)), J.member("inputSchema", tool.input_schema)]))))
            problems = JsonArray(mapList(listing.problems, lambda problem: JsonString(problem)))
            return J.encode(J.obj([J.member("tools", tools), J.member("cursor", JsonString(listing.next_cursor)), J.member("problems", problems)]))


def call_text_of(text):
    """A `tools/call` reply as the tool's text; an `isError` reply is that text, an error response is its words."""
    message = unwrap_or(J.decode(text), JsonNull())
    match J.text_at(message, "kind", ""):
        case "error":
            return error_words(message)
        case _:
            match Mcp.call_result("", J.object_at(message, "result")):
                case Success(value=words):
                    return words
                case Error(message=words):
                    return words


def ping_replies(texts):
    """The encoded `ping` responses a server's own pings ask for, one per line, empty when it sent none."""
    pings = filterList(decode_messages(texts), lambda message: J.text_at(message, "kind", "") == "request" and J.text_at(message, "method", "") == "ping")
    return join(mapList(pings, lambda message: Mcp.encode_line(Mcp.ping_response(J.object_at(message, "id")))), "\n")


def response_text_of(message_text):
    """Tool text from one tagged message text, for the host-free suite."""
    return call_text_of(message_text)


def header_value(result, wanted):
    """One HTTP response header, case-insensitively, empty when absent."""
    headers = J.list_at(result, "headers")
    found = filterList(headers, lambda header: toLowerCase(J.text_at(header, "name", "")) == toLowerCase(wanted))
    return J.text_at(unwrap_or(listGet(found, 0), JsonNull()), "value", "")


def http_messages(result, body):
    """The JSON-RPC messages in one JSON or SSE HTTP response, each as tagged JSON text."""
    return mapList(Mcp.reply_messages(header_value(result, "content-type"), body), lambda incoming: J.encode(tag_of(incoming)))


def arguments_of(text):
    """A tool call's JSON object as the map `tools/call` requires."""
    match J.decode(text):
        case Success(value=value):
            match value:
                case JsonObject():
                    return Success(value=J.to_map(value))
                case _:
                    return Error(message="Tool arguments must be a JSON object.")
        case Error(message=problem):
            return Error(message=f"Tool arguments are not valid JSON: {problem}")


class Wire:
    """One encoded JSON-RPC line to send, or the words saying why it could not be built."""
    text: str
    problem: str


def initialize_line(ident):
    """The `initialize` request as one stdio line."""
    return Mcp.encode_line(Mcp.initialize_request(JsonInt(ident), "Osprey Agent", "1"))


def initialized_line():
    """The `notifications/initialized` line."""
    return Mcp.encode_line(Mcp.initialized_notification())


def list_tools_line(ident, cursor):
    """The `tools/list` request as one stdio line, paging by cursor."""
    return Mcp.encode_line(Mcp.list_tools_request(JsonInt(ident), cursor))


def call_line(ident, tool, arguments_json):
    """The `tools/call` request as one stdio line; arguments that are not a JSON object are a problem in words."""
    match arguments_of(arguments_json):
        case Success(value=arguments):
            return Wire(text=Mcp.encode_line(Mcp.call_tool_request(JsonInt(ident), tool, arguments)), problem="")
        case Error(message=problem):
            return Wire(text="", problem=problem)


def protocol_version():
    """The MCP protocol version this client speaks."""
    return Mcp.PROTOCOL_VERSION


def bare_tool(server_name, call_name):
    """The tool name a `<server>_<tool>` call names."""
    return Mcp.original_tool_name(server_name, call_name)
