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
    """Parses the standard `mcpServers` shape with no host environment.

    # Examples
    ```osprey
    from agent import McpClient

    @handler(Arith)
    class wrapped:
        def overflow(_op, _l, _r, value):
            return value

    with wrapped:
        ok = McpClient.parse_config('{"mcpServers":{"fs":{"command":"npx","args":["fs"]}}}')
        bad = McpClient.parse_config('{"mcpServers":{"empty":{}}}')
        junk = McpClient.parse_config("not json")
        print(f"{len(ok.servers)} {ok.problems} | {len(bad.servers)} {bad.problems} | {junk.problems}")
    ```
    ```output
    1 [] | 0 ["MCP server config 'empty' must have either 'command' or 'url'"] | ["not valid JSON: jsonDecode: byte 0: unexpected character"]
    ```
    """
    empty: dict[str, str] = {}
    return Mcp.parse_config(text, empty)


def json_map(values):
    """A string map as a JSON string map, in key order."""
    empty: dict[str, Json] = {}
    return foldList(mapKeys(values), empty, lambda acc, key: mapSet(acc, key, JsonString(unwrap_or(mapGet(values, key), ""))))


def config_value(config):
    """One serializable server entry, including its private values.

    # Examples
    ```osprey
    from agent import McpClient
    from pydantic_ai import JsonUtil as J
    from pydantic_ai import Mcp

    @handler(Arith)
    class wrapped:
        def overflow(_op, _l, _r, value):
            return value

    with wrapped:
        load = McpClient.parse_config('{"mcpServers":{"fs":{"command":"npx","args":["-y","fs"],"env":{"A":"1"}}}}')
        blank = Mcp.StdioServer(name="", command="", args=[], env={}, cwd="")
        print(J.encode(McpClient.config_value(unwrap_or(load.servers[0], blank))))
    ```
    ```output
    {"command":"npx","args":["-y","fs"],"env":{"A":"1"}}
    ```
    """
    match config:
        case Mcp.StdioServer(command=command, args=args, env=env, cwd=cwd):
            members = [J.member("command", JsonString(command)), J.member("args", JsonArray(mapList(args, lambda text: JsonString(text)))), J.member("env", J.of_map(json_map(env)))]
            return J.obj(members if cwd == "" else listAppend(members, J.member("cwd", JsonString(cwd))))
        case Mcp.HttpServer(url=url, headers=headers):
            return J.obj([J.member("url", JsonString(url)), J.member("headers", J.of_map(json_map(headers)))])


def config_json(servers):
    """The standard private `mcp.json` document.

    # Examples
    ```osprey
    from agent import McpClient

    @handler(Arith)
    class wrapped:
        def overflow(_op, _l, _r, value):
            return value

    with wrapped:
        load = McpClient.parse_config('{"mcpServers":{"fs":{"command":"npx","args":["fs"]}}}')
        print(McpClient.config_json(load.servers))
    ```
    ```output
    {"mcpServers":{"fs":{"command":"npx","args":["fs"],"env":{}}}}
    ```
    """
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
    """`mcp.json` with one more server, or a naming problem.

    # Examples
    ```osprey
    from agent import McpClient

    doc = '{"mcpServers":{"fs":{"command":"npx","args":["fs"],"env":{}}}}'
    entry = '{"command":"git","args":[]}'
    added = McpClient.add_global(doc, "git", entry)
    print(f"[{McpClient.add_global(doc, "fs", entry).problem}] [{added.problem}] {added.text}")
    ```
    ```output
    [An MCP server named 'fs' already exists.] [] {"mcpServers":{"fs":{"command":"npx","args":["fs"],"env":{}},"git":{"command":"git","args":[]}}}
    ```
    """
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
    """`mcp.json` with every server a pasted block configures, or one problem naming the first fault. Atomic: nothing is added when anything is wrong.

    # Examples
    ```osprey
    from agent import McpClient

    @handler(Arith)
    class wrapped:
        def overflow(_op, _l, _r, value):
            return value

    with wrapped:
        doc = '{"mcpServers":{"fs":{"command":"npx","args":["fs"],"env":{}}}}'
        pasted = '{"mcpServers":{"git":{"command":"git"},"fs":{"command":"other"}}}'
        clash = McpClient.paste_global(doc, pasted)
        both = McpClient.paste_global("{}", pasted)
        print(f"[{clash.problem}] [{both.problem}] {both.text}")
    ```
    ```output
    [An MCP server named 'fs' already exists.] [] {"mcpServers":{"git":{"command":"git","args":[],"env":{}},"fs":{"command":"other","args":[],"env":{}}}}
    ```
    """
    loaded = parse_config(pasted)
    existing = names_of(global_text)
    pasted_names = mapList(loaded.servers, lambda server: Mcp.name_of(server))
    clash = unwrap_or(listGet(filterList(pasted_names, lambda name: listContains(existing, name)), 0), "")
    problem = paste_problem(loaded.problems, clash)
    added = foldList(loaded.servers, global_text, lambda acc, server: with_entry(acc, Mcp.name_of(server), config_value(server)))
    return Save(text=(added if problem == "" else ""), problem=problem)


def remove_global_text(global_text, name):
    """`mcp.json` without one server.

    # Examples
    ```osprey
    from agent import McpClient

    doc = '{"mcpServers":{"fs":{"command":"npx","args":["fs"],"env":{}},"git":{"command":"git","args":[],"env":{}}}}'
    print(f"{McpClient.remove_global_text(doc, "fs")} {McpClient.remove_global_text(doc, "nope")}")
    ```
    ```output
    {"mcpServers":{"git":{"command":"git","args":[],"env":{}}}} {"mcpServers":{"fs":{"command":"npx","args":["fs"],"env":{}},"git":{"command":"git","args":[],"env":{}}}}
    ```
    """
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
    """The servers both documents configure, as primitives, with their problems in words.

    # Examples
    ```osprey
    from agent import McpClient

    @handler(Arith)
    class wrapped:
        def overflow(_op, _l, _r, value):
            return value

    with wrapped:
        doc = '{"mcpServers":{"fs":{"command":"npx","args":["fs"]},"web":{"url":"http://evil.example"}}}'
        merged = McpClient.merge_view(doc, '{"mcpServers":{"fs":{"command":"other"}}}')
        named = mapList(merged.mergedServers, lambda s: s.viewName + ":" + s.viewOrigin + ":" + s.viewEndpoint)
        print(f"{named} {merged.mergedProblems}")
    ```
    ```output
    ["fs:project:other"] ["mcp.json: MCP server config 'web' needs an HTTPS URL, or an HTTP URL on this machine"]
    ```
    """
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
    """HTTPS anywhere, or plain HTTP only to a loopback test/local server.

    # Examples
    ```osprey
    from agent import McpClient

    secure = McpClient.valid_url("https://api.example.com/mcp")
    local = McpClient.valid_url("http://localhost:8080")
    spoof = McpClient.valid_url("http://localhost.attacker")
    bare = McpClient.valid_url("https://")
    print(f"{secure} {local} {spoof} {bare}")
    ```
    ```output
    true true false false
    ```
    """
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
    """The first problem in an Add server form, empty when it is valid.

    # Examples
    ```osprey
    from agent import McpClient

    @handler(Arith)
    class wrapped:
        def overflow(_op, _l, _r, value):
            return value

    with wrapped:
        unnamed = McpClient.form_problem("", "command", "npx", "", "", "")
        remote = McpClient.form_problem("fs", "url", "", "http://evil.example", "", "")
        env = McpClient.form_problem("fs", "command", "npx", "", "TOKEN", "")
        fine = McpClient.form_problem("fs", "command", "npx", "", "TOKEN=x", "")
        print(f"[{unnamed}] [{remote}] [{env}] [{fine}]")
    ```
    ```output
    [Enter a server name.] [Enter an HTTPS URL, or an HTTP URL on this machine.] [Environment lines must look like NAME=value.] []
    ```
    """
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
    """A validated form as the server entry `mcp.json` holds, or the words saying why not.

    # Examples
    ```osprey
    from agent import McpClient

    @handler(Arith)
    class wrapped:
        def overflow(_op, _l, _r, value):
            return value

    with wrapped:
        entry = McpClient.form_config("fs", "command", "npx", "-y\\nfs", "", "A=1", "")
        refused = McpClient.form_config("fs", "command", "npx", "", "", "A", "")
        print(f"{entry.text} [{entry.problem}] [{refused.problem}]")
    ```
    ```output
    {"command":"npx","args":["-y","fs"],"env":{"A":"1"}} [] [Environment lines must look like NAME=value.]
    ```
    """
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
    """A validated form as one typed server config, kept for the host-free suite.

    # Examples
    ```osprey
    from agent import McpClient

    @handler(Arith)
    class wrapped:
        def overflow(_op, _l, _r, value):
            return value

    with wrapped:
        match McpClient.form_server("fs", "url", "", "", "https://api.example.com", "", "X-Token: abc"):
            case Success():
                print("parsed")
            case Error(message=problem):
                print(problem)
    ```
    ```output
    Header lines must look like NAME=value.
    ```
    """
    entry = form_config(name, kind, command, arguments, url, environment, headers)
    if entry.problem != "":
        return Error(message=entry.problem)
    else:
        single = J.encode(J.obj([J.member("mcpServers", J.obj([J.member(trim(name), unwrap_or(J.decode(entry.text), JsonNull()))]))]))
        match parse_config(single).servers[0]:
            case Success(value=server):
                return Success(value=server)
            case Error():
                return Error(message="the form did not parse")


def start_brief(entry_json):
    """How to reach one server, as JSON: a child process with its environment, or a URL with its headers.

    # Examples
    ```osprey
    from agent import McpClient

    stdio = McpClient.start_brief('{"command":"npx","args":["fs"],"env":{"A":"1"}}')
    http = McpClient.start_brief('{"url":"https://x","headers":{"A":"1"}}')
    print(f"{stdio} {http} {McpClient.start_brief('{}')}")
    ```
    ```output
    {"kind":"stdio","command":"npx","args":["fs"],"env":[{"name":"A","value":"1"}]} {"kind":"http","url":"https://x","headers":[{"name":"A","value":"1"}]} {"kind":"unsupported","problem":"The server entry needs a URL."}
    ```
    """
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
    """The complete lines a chunk of standard output ends, and the buffer the next chunk starts from.

    # Examples
    ```osprey
    from agent import McpClient

    @handler(Arith)
    class wrapped:
        def overflow(_op, _l, _r, value):
            return value

    with wrapped:
        step = McpClient.read_step("", "{\\"a\\":1}\\n{\\"b\\"")
        print(f"{step.lines} [{step.buffer}]")
    ```
    ```output
    ["{\\"a\\":1}"] [{"b"]
    ```
    """
    lines = Mcp.lines_of(Mcp.LineBuffer(pending=buffer), chunk)
    return ReadStep(lines=lines.lines, buffer=lines.buffer.pending)


class ReadStep:
    """Stdio framing over one read: the lines it completed and the remainder."""
    lines: list[str]
    buffer: str


def message_json(line):
    """One line from a server as tagged JSON text, so no protocol union crosses the flavor boundary.

    # Examples
    ```osprey
    from agent import McpClient

    print(f"{McpClient.message_json('{"id":1,"result":{"x":2}}')} {McpClient.message_json('garbage')}")
    ```
    ```output
    {"kind":"ok","id":1,"result":{"x":2}} {"kind":"malformed","reason":"not JSON: jsonDecode: byte 0: unexpected character"}
    ```
    """
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
    """The response message whose id matches `wanted` as JSON text, or `{}` when there is none.

    # Examples
    ```osprey
    from agent import McpClient

    texts = [McpClient.message_json('{"id":1,"result":{"x":2}}'), McpClient.message_json('{"id":2,"result":{}}')]
    print(f"{McpClient.find_response(texts, "1")} {McpClient.find_response(texts, "9")}")
    ```
    ```output
    {"kind":"ok","id":1,"result":{"x":2}} {}
    ```
    """
    found = filterList(decode_messages(texts), lambda message: (J.text_at(message, "kind", "") == "ok" or J.text_at(message, "kind", "") == "error") and id_text(message) == wanted)
    return J.encode(unwrap_or(listGet(found, 0), J.obj([])))


def initialize_outcome(text):
    """The `initialize` reply as `{"ok", "problem"}`: our protocol version, or the server's refusal in words.

    # Examples
    ```osprey
    from agent import McpClient

    agreed = McpClient.message_json('{"id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{},"serverInfo":{"name":"fs","version":"1"}}}')
    refused = McpClient.message_json('{"id":1,"error":{"code":-32602,"message":"bad"}}')
    print(f"{McpClient.initialize_outcome(agreed)} {McpClient.initialize_outcome(refused)}")
    ```
    ```output
    {"ok":true,"problem":""} {"ok":false,"problem":"bad (code: -32602)"}
    ```
    """
    message = unwrap_or(J.decode(text), JsonNull())
    match J.text_at(message, "kind", ""):
        case "error":
            return J.encode(J.obj([J.member("ok", JsonBool(False)), J.member("problem", JsonString(error_words(message)))]))
        case _:
            match Mcp.initialize_result(J.object_at(message, "result")):
                case Mcp.Ok(session=_):
                    return J.encode(J.obj([J.member("ok", JsonBool(True)), J.member("problem", JsonString(""))]))
                case Mcp.Failed(error=error):
                    return J.encode(J.obj([J.member("ok", JsonBool(False)), J.member("problem", JsonString(Mcp.message_of(error)))]))


def error_words(message):
    return f"{J.text_at(message, 'message', '')} (code: {J.int_at(message, 'code', 0)})"


def tools_page(text):
    """A `tools/list` reply as `{"tools", "cursor", "problems"}`.

    # Examples
    ```osprey
    from agent import McpClient

    listed = McpClient.message_json('{"id":1,"result":{"tools":[{"name":"read","description":"Reads","inputSchema":{"type":"object"}}],"nextCursor":"c2"}}')
    failed = McpClient.message_json('{"id":1,"error":{"code":1,"message":"nope"}}')
    print(f"{McpClient.tools_page(listed)} {McpClient.tools_page(failed)}")
    ```
    ```output
    {"tools":[{"name":"read","description":"Reads","inputSchema":{"type":"object"}}],"cursor":"c2","problems":[]} {"tools":[],"cursor":"","problems":["nope (code: 1)"]}
    ```
    """
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
    """A `tools/call` reply as the tool's text; an `isError` reply is that text, an error response is its words.

    # Examples
    ```osprey
    from agent import McpClient

    @handler(Arith)
    class wrapped:
        def overflow(_op, _l, _r, value):
            return value

    with wrapped:
        text = McpClient.message_json('{"id":1,"result":{"content":[{"type":"text","text":"hi"}]}}')
        fault = McpClient.message_json('{"id":1,"result":{"isError":true,"content":[{"type":"text","text":"boom"}]}}')
        missing = McpClient.message_json('{"id":1,"error":{"code":1,"message":"nope"}}')
        print(f"[{McpClient.call_text_of(text)}] [{McpClient.call_text_of(fault)}] [{McpClient.call_text_of(missing)}]")
    ```
    ```output
    [hi] [boom] [nope (code: 1)]
    ```
    """
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
    """The encoded `ping` responses a server's own pings ask for, one per line, empty when it sent none.

    # Examples
    ```osprey
    from agent import McpClient

    lines = [McpClient.message_json('{"id":7,"method":"ping"}'), McpClient.message_json('{"id":1,"result":{}}')]
    print(f"[{McpClient.ping_replies(lines)}]")
    ```
    ```output
    [{"jsonrpc":"2.0","id":7,"result":{}}
    ]
    ```
    """
    pings = filterList(decode_messages(texts), lambda message: J.text_at(message, "kind", "") == "request" and J.text_at(message, "method", "") == "ping")
    return join(mapList(pings, lambda message: Mcp.encode_line(Mcp.ping_response(J.object_at(message, "id")))), "\n")


def header_value(result, wanted):
    """One HTTP response header, case-insensitively, empty when absent.

    # Examples
    ```osprey
    from agent import McpClient
    from pydantic_ai import JsonUtil as J

    type = J.obj([J.member("name", JsonString("Content-Type")), J.member("value", JsonString("text/event-stream"))])
    result = J.obj([J.member("headers", JsonArray([type]))])
    print(f"[{McpClient.header_value(result, "content-type")}] [{McpClient.header_value(result, "X-Token")}]")
    ```
    ```output
    [text/event-stream] []
    ```
    """
    headers = J.list_at(result, "headers")
    found = filterList(headers, lambda header: toLowerCase(J.text_at(header, "name", "")) == toLowerCase(wanted))
    return J.text_at(unwrap_or(listGet(found, 0), JsonNull()), "value", "")


def http_messages(result, body):
    """The JSON-RPC messages in one JSON or SSE HTTP response, each as tagged JSON text.

    # Examples
    ```osprey
    from agent import McpClient
    from pydantic_ai import JsonUtil as J

    @handler(Arith)
    class wrapped:
        def overflow(_op, _l, _r, value):
            return value

    with wrapped:
        whole = J.obj([J.member("headers", JsonArray([J.obj([J.member("name", JsonString("Content-Type")), J.member("value", JsonString("application/json"))])]))])
        stream = J.obj([J.member("headers", JsonArray([J.obj([J.member("name", JsonString("Content-Type")), J.member("value", JsonString("text/event-stream"))])]))])
        print(f"{McpClient.http_messages(whole, '{"id":1,"result":{}}')} {McpClient.http_messages(stream, "event: message\\ndata: {\\"id\\":2,\\"result\\":{}}\\n\\n")}")
    ```
    ```output
    ["{\\"kind\\":\\"ok\\",\\"id\\":1,\\"result\\":{}}"] ["{\\"kind\\":\\"ok\\",\\"id\\":2,\\"result\\":{}}"]
    ```
    """
    return mapList(Mcp.reply_messages(header_value(result, "content-type"), body), lambda incoming: J.encode(tag_of(incoming)))


def arguments_of(text):
    """A tool call's JSON object as the map `tools/call` requires.

    # Examples
    ```osprey
    from agent import McpClient

    match McpClient.arguments_of('{"path":"a"}'):
        case Success(value=values):
            print(mapKeys(values))
        case Error():
            print("refused")
    match McpClient.arguments_of('[]'):
        case Success():
            print("accepted")
        case Error(message=problem):
            print(problem)
    ```
    ```output
    ["path"]
    Tool arguments must be a JSON object.
    ```
    """
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
    """The `initialize` request as one stdio line.

    # Examples
    ```osprey
    from agent import McpClient

    print(McpClient.initialize_line(1))
    ```
    ```output
    {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"Osprey Agent","version":"1"}}}

    ```
    """
    return Mcp.encode_line(Mcp.initialize_request(JsonInt(ident), "Osprey Agent", "1"))


def initialized_line():
    """The `notifications/initialized` line.

    # Examples
    ```osprey
    from agent import McpClient

    print(McpClient.initialized_line())
    ```
    ```output
    {"jsonrpc":"2.0","method":"notifications/initialized"}

    ```
    """
    return Mcp.encode_line(Mcp.initialized_notification())


def list_tools_line(ident, cursor):
    """The `tools/list` request as one stdio line, paging by cursor.

    # Examples
    ```osprey
    from agent import McpClient

    print(f"{McpClient.list_tools_line(2, "")}{McpClient.list_tools_line(3, "c2")}")
    ```
    ```output
    {"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
    {"jsonrpc":"2.0","id":3,"method":"tools/list","params":{"cursor":"c2"}}

    ```
    """
    return Mcp.encode_line(Mcp.list_tools_request(JsonInt(ident), cursor))


def call_line(ident, tool, arguments_json):
    """The `tools/call` request as one stdio line; arguments that are not a JSON object are a problem in words.

    # Examples
    ```osprey
    from agent import McpClient

    call = McpClient.call_line(4, "fs_read", '{"path":"a"}')
    bad = McpClient.call_line(4, "fs_read", "[]")
    print(f"{call.text}[{call.problem}] [{bad.problem}]")
    ```
    ```output
    {"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"fs_read","arguments":{"path":"a"}}}
    [] [Tool arguments must be a JSON object.]
    ```
    """
    match arguments_of(arguments_json):
        case Success(value=arguments):
            return Wire(text=Mcp.encode_line(Mcp.call_tool_request(JsonInt(ident), tool, arguments)), problem="")
        case Error(message=problem):
            return Wire(text="", problem=problem)


def protocol_version():
    """The MCP protocol version this client speaks.

    # Examples
    ```osprey
    from agent import McpClient

    print(McpClient.protocol_version())
    ```
    ```output
    2025-06-18
    ```
    """
    return Mcp.PROTOCOL_VERSION


def bare_tool(server_name, call_name):
    """The tool name a `<server>_<tool>` call names.

    # Examples
    ```osprey
    from agent import McpClient

    @handler(Arith)
    class wrapped:
        def overflow(_op, _l, _r, value):
            return value

    with wrapped:
        print(f"{McpClient.bare_tool('fs', 'fs_read')} {McpClient.bare_tool('fs', 'other')}")
    ```
    ```output
    read other
    ```
    """
    return Mcp.original_tool_name(server_name, call_name)
