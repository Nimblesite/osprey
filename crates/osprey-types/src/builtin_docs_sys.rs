//! Built-in documentation data (system, network & runtime). Generated companion to
//! `builtins.rs`: every entry's prose pairs with the type scheme of the
//! same name. Edit prose here; edit types in `builtins.rs`. The parity
//! test in `builtin_docs.rs` guarantees the two stay in lockstep.
//!
//! Param order and count MUST match the builtin's real arity.

use crate::builtin_docs::BuiltinDoc;
use crate::builtin_docs_lang::builtin_doc;

const JSON_DECODE_EXAMPLE: &str = "fn encode(value) = match jsonEncode(value) {\n    Success { value } => value\n    Error { message } => message\n}\nmatch jsonDecode(\"[1]\") {\n    Success { value } => print(encode(value))\n    Error { message } => print(message)\n}";

const JSON_ENCODE_EXAMPLE: &str = "match jsonEncode(JsonArray([JsonInt(1)])) {\n    Success { value } => print(value)\n    Error { message } => print(message)\n}";

const JSON_DOCUMENT_EXAMPLE: &str = "fn inspect(document) = {\n    let summary = \"name=${jsonGet(document, \"name\")}; items=${jsonLength(document, \"items\")}\"\n    let released = jsonFree(document)\n    \"${summary}; released=${released}\"\n}\nmatch jsonParse(\"{\\\"name\\\":\\\"osprey\\\",\\\"items\\\":[1,2]}\") {\n    Success { value } => print(inspect(value))\n    Error { message } => print(message)\n}";

const PROCESS_EXAMPLE: &str = "fn onEvent(_pid, kind, data) = print(\"${kind}: ${trim(data)}\")\nmatch spawnProcess(\"echo hello\", onEvent) {\n    Success { value } => {\n        let exitCode = awaitProcess(value)\n        cleanupProcess(value)\n        print(\"exit: ${exitCode}\")\n    }\n    Error { message } => print(\"Process failed: ${message}\")\n}";

const HTTP_CLIENT_EXAMPLE: &str = "let client = httpCreateClient(\"http://127.0.0.1:18898\", 1000)\nprint(if client > 0 { \"closed: ${httpCloseClient(client)}\" } else { \"client error: ${client}\" })";

const RAW_KEY_EXAMPLE: &str = "// Run in an interactive terminal; raw mode is restored before printing.\ntermRawMode(1)\nlet key = termReadKey()\ntermRawMode(0)\nprint(key)";

const CURSOR_EXAMPLE: &str = "let hidden = termHideCursor()\nlet moved = termMoveCursor(1, 1)\nlet shown = termShowCursor()\nprint(\"hide=${hidden}; move=${moved}; show=${shown}\")";

const WEBSOCKET_EXAMPLE: &str = "fn communicate(server, connection) = {\n    let sent = websocketSend(connection, \"Hello\")\n    let broadcast = websocketServerBroadcast(server, \"Welcome\")\n    let closed = websocketClose(connection)\n    \"sent=${sent}; broadcast=${broadcast}; closed=${closed}\"\n}\nfn exchange(server) = {\n    let connection = websocketConnect(\"ws://127.0.0.1:18899/chat\")\n    if connection > 0 { communicate(server, connection) } else { \"connection error: ${connection}\" }\n}\nlet server = websocketCreateServer(18899, \"127.0.0.1\", \"/chat\")\nlet listening = if server > 0 { websocketServerListen(server) } else { server }\nprint(if listening == 0 { exchange(server) } else { \"server error: ${listening}\" })";

macro_rules! http_example {
    ($action:literal) => { http_example!("", $action) };
    ($helpers:literal, $action:literal) => {
        concat!($helpers, "fn reply(_method, _path, _headers, _body) = HttpResponse {\n    status: 200, headers: \"X-Demo: hi\\r\\n\", contentType: \"text/plain\",\n    streamFd: -1, isComplete: true, partialBody: \"hello\"\n}\nfn runRequest(client) = {\n    let result = ", $action, "\n    let closed = httpCloseClient(client)\n    \"${result}; client close=${closed}\"\n}\nfn request() = {\n    let client = httpCreateClient(\"http://127.0.0.1:18898\", 1000)\n    if client > 0 { runRequest(client) } else { \"client error: ${client}\" }\n}\nfn serve(server) = {\n    let listening = httpListen(server, reply)\n    let result = if listening == 0 { request() } else { \"listen error: ${listening}\" }\n    let stopped = httpStopServer(server)\n    \"${result}; server stop=${stopped}\"\n}\nlet server = httpCreateServer(18898, \"127.0.0.1\")\nprint(if server > 0 { serve(server) } else { \"server error: ${server}\" })")
    };
}
macro_rules! http_response_example {
    () => { http_example!("fn inspect(response) = {\n    let summary = \"status=${httpResponseStatus(response)}; body=${httpResponseBody(response)}; header=${httpResponseHeader(response, \"X-Demo\")}\"\n    let released = httpResponseFree(response)\n    \"${summary}; released=${released}\"\n}\n", "match httpGetResponse(client, \"/\", \"\") {\n        Success { value } => inspect(value)\n        Error { message } => message\n    }") };
}

/// `files` built-in documentation. Prose only — types come from the
/// authoritative scheme in `builtins.rs`, joined by name.
pub(crate) static FILES: &[BuiltinDoc] = &[
    builtin_doc!(
        "readFile",
        "Reads a text file into a Result<string, Error>. The NUL-terminated string ABI is not a binary-buffer interface.",
        ["filename" => "Path to the file to read"],
        "match readFile(\"input.txt\") {\n    Success { value } => print(value)\n    Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "writeFile",
        "Writes text, creating the file if needed. Returns Result<int, Error> containing the number of bytes written.",
        ["filename" => "Path to the file to write", "content" => "Content to write to the file"],
        "match writeFile(\"output.txt\", \"Hello, World!\") {\n    Success { value } => print(\"${value} bytes written\")\n    Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "loadTensor",
        "Reads one named F64, F32, F16 or BF16 tensor out of a safetensors file as a List<float>, every element widened exactly. Nothing touches the GPU surface.",
        ["path" => "Path to the .safetensors file", "name" => "Tensor name in its header, e.g. \"wte.weight\""],
        "match loadTensor(\"model.safetensors\", \"wte.weight\") {\n  Success { value } => print(\"${listLength(value)} floats\")\n  Error { message } => print(message)\n}",
    ),
];

/// HTTP built-in documentation [BUILTIN-HTTP] [HTTP-RESPONSE-HANDLE]. Prose
/// only — types come from the authoritative scheme in `builtins.rs`.
pub(crate) static HTTP: &[BuiltinDoc] = &[
    builtin_doc!(
        "httpCreateClient",
        "Creates an HTTP client and returns its handle, or a negative runtime error.",
        ["base_url" => "Base URL for requests (e.g., \"http://api.example.com\")", "timeout" => "Request timeout in milliseconds"],
        HTTP_CLIENT_EXAMPLE,
    ),
    builtin_doc!(
        "httpCloseClient",
        "Closes the HTTP client and returns the runtime status.",
        ["clientID" => "Client identifier to close"],
        HTTP_CLIENT_EXAMPLE,
    ),
    builtin_doc!(
        "httpGet",
        "Makes an HTTP GET request and returns its status code, or a negative transport error.",
        ["clientID" => "Client identifier from httpCreateClient", "path" => "Request path (e.g., \"/api/users\")", "headers" => "Additional headers (e.g., \"Authorization: Bearer token\")"],
        http_example!("toString(httpGet(client, \"/\", \"\"))"),
    ),
    builtin_doc!(
        "httpGetResponse",
        "Sends an HTTP GET request and returns a response handle for inspecting the status, headers, and body.",
        ["clientID" => "Client identifier from httpCreateClient", "path" => "Request path, e.g. \"/api/users\"", "headers" => "Additional request headers, or \"\" for none"],
        http_response_example!(),
    ),
    builtin_doc!(
        "httpResponseBody",
        "Returns the body of a response handle as a string.",
        ["responseID" => "Handle returned by httpGetResponse"],
        http_response_example!(),
    ),
    builtin_doc!(
        "httpResponseFree",
        "Releases a response handle; an invalid handle or double free returns Error.",
        ["responseID" => "Handle returned by httpGetResponse"],
        http_response_example!(),
    ),
    builtin_doc!(
        "httpResponseStatus",
        "Returns the HTTP status code of a response handle.",
        ["responseID" => "Handle returned by httpGetResponse"],
        http_response_example!(),
    ),
    builtin_doc!(
        "httpResponseHeader",
        "Returns the value of the named header from a response handle.",
        ["responseID" => "Handle returned by httpGetResponse", "name" => "Header name, e.g. \"Content-Type\""],
        http_response_example!(),
    ),
    builtin_doc!(
        "httpPost",
        "Makes an HTTP POST request and returns its status code, or a negative transport error.",
        ["clientID" => "Client identifier from httpCreateClient", "path" => "Request path", "body" => "Request body data", "headers" => "Additional headers"],
        http_example!("toString(httpPost(client, \"/\", \"hello\", \"Content-Type: text/plain\"))"),
    ),
    builtin_doc!(
        "httpPut",
        "Makes an HTTP PUT request and returns its status code, or a negative transport error.",
        ["clientID" => "Client identifier from httpCreateClient", "path" => "Request path", "body" => "Request body data", "headers" => "Additional headers"],
        http_example!("toString(httpPut(client, \"/\", \"updated\", \"Content-Type: text/plain\"))"),
    ),
    builtin_doc!(
        "httpDelete",
        "Makes an HTTP DELETE request and returns its status code, or a negative transport error.",
        ["clientID" => "Client identifier from httpCreateClient", "path" => "Request path", "headers" => "Additional headers"],
        http_example!("toString(httpDelete(client, \"/\", \"\"))"),
    ),
    builtin_doc!(
        "httpCreateServer",
        "Creates an HTTP server bound to the specified port and address.",
        ["port" => "Port number to bind to (1-65535)", "address" => "IP address to bind to (e.g., \"127.0.0.1\", \"0.0.0.0\")"],
        http_example!("toString(httpGet(client, \"/\", \"\"))"),
    ),
    builtin_doc!(
        "httpListen",
        "Starts the HTTP server with a request handler and returns 0 or a negative runtime error.",
        ["serverID" => "Server identifier from httpCreateServer", "handler" => "Request handler function"],
        http_example!("toString(httpGet(client, \"/\", \"\"))"),
    ),
    builtin_doc!(
        "httpStopServer",
        "Stops the HTTP server and returns the runtime status.",
        ["serverID" => "Server identifier to stop"],
        http_example!("toString(httpGet(client, \"/\", \"\"))"),
    ),
];

/// `json` built-in documentation. Prose only — types come from the
/// authoritative scheme in `builtins.rs`, joined by name.
pub(crate) static JSON: &[BuiltinDoc] = &[
    builtin_doc!(
        "jsonParse",
        "Parses a JSON string and returns an opaque document handle for querying, or an error on malformed input.",
        ["text" => "The JSON text to parse"],
        JSON_DOCUMENT_EXAMPLE,
    ),
    builtin_doc!(
        "jsonGet",
        "Returns a JSON scalar at the given path as a string. Arrays, objects, invalid paths, and invalid handles return Error.",
        ["document" => "Handle returned by jsonParse", "path" => "Dotted keys and bracketed indices, e.g. \"user.items[0].name\""],
        JSON_DOCUMENT_EXAMPLE,
    ),
    builtin_doc!(
        "jsonLength",
        "Returns an array length or object member count at the given path, or -1 for an invalid path, handle, or scalar.",
        ["document" => "Handle returned by jsonParse", "path" => "Path to an array or object; empty means the root"],
        JSON_DOCUMENT_EXAMPLE,
    ),
    builtin_doc!(
        "jsonFree",
        "Releases a parsed JSON document handle obtained from jsonParse.",
        ["document" => "Handle returned by jsonParse"],
        JSON_DOCUMENT_EXAMPLE,
    ),
    builtin_doc!(
        "__jsonDecodeOpen",
        "Internal primitive of jsonDecode: parses a document into slots for the typed walk. Call jsonDecode instead.",
        ["text" => "The JSON text to decode"],
        JSON_DECODE_EXAMPLE,
    ),
    builtin_doc!(
        "__jsonDecodeClose",
        "Internal primitive of jsonDecode: frees a document the walk has finished with. Call jsonDecode instead.",
        ["document" => "Handle from __jsonDecodeOpen"],
        JSON_DECODE_EXAMPLE,
    ),
    builtin_doc!(
        "__jsonNodeKind",
        "Internal primitive of jsonDecode: the kind code of one value. Call jsonDecode instead.",
        ["document" => "Handle from __jsonDecodeOpen", "node" => "Slot of a value; the root is slot 0"],
        JSON_DECODE_EXAMPLE,
    ),
    builtin_doc!(
        "__jsonNodeFirst",
        "Internal primitive of jsonDecode: the first slot of a container's children. Call jsonDecode instead.",
        ["document" => "Handle from __jsonDecodeOpen", "node" => "Slot of a container; the root is slot 0"],
        JSON_DECODE_EXAMPLE,
    ),
    builtin_doc!(
        "__jsonNodeEnd",
        "Internal primitive of jsonDecode: the slot after a container's last child. Call jsonDecode instead.",
        ["document" => "Handle from __jsonDecodeOpen", "node" => "Slot of a container; the root is slot 0"],
        JSON_DECODE_EXAMPLE,
    ),
    builtin_doc!(
        "__jsonNodeKey",
        "Internal primitive of jsonDecode: the member name of an object's child. Call jsonDecode instead.",
        ["document" => "Handle from __jsonDecodeOpen", "node" => "Slot of an object member; the root is slot 0"],
        JSON_DECODE_EXAMPLE,
    ),
    builtin_doc!(
        "__jsonNodeText",
        "Internal primitive of jsonDecode: a string's value. Call jsonDecode instead.",
        ["document" => "Handle from __jsonDecodeOpen", "node" => "Slot of a string; the root is slot 0"],
        JSON_DECODE_EXAMPLE,
    ),
    builtin_doc!(
        "__jsonNodeInt",
        "Internal primitive of jsonDecode: an integer's value. Call jsonDecode instead.",
        ["document" => "Handle from __jsonDecodeOpen", "node" => "Slot of an integer; the root is slot 0"],
        JSON_DECODE_EXAMPLE,
    ),
    builtin_doc!(
        "__jsonNodeFloat",
        "Internal primitive of jsonDecode: a float's value. Call jsonDecode instead.",
        ["document" => "Handle from __jsonDecodeOpen", "node" => "Slot of a float; the root is slot 0"],
        JSON_DECODE_EXAMPLE,
    ),
    builtin_doc!(
        "__jsonWriterOpen",
        "Internal primitive of jsonEncode: opens a single-buffer writer. Call jsonEncode instead.",
        [],
        JSON_ENCODE_EXAMPLE,
    ),
    builtin_doc!(
        "__jsonWriteRaw",
        "Internal primitive of jsonEncode: appends a structural token. Call jsonEncode instead.",
        ["writer" => "Handle from __jsonWriterOpen", "token" => "Literal JSON text"],
        JSON_ENCODE_EXAMPLE,
    ),
    builtin_doc!(
        "__jsonWriteString",
        "Internal primitive of jsonEncode: appends one quoted, escaped string. Call jsonEncode instead.",
        ["writer" => "Handle from __jsonWriterOpen", "text" => "The string to quote"],
        JSON_ENCODE_EXAMPLE,
    ),
    builtin_doc!(
        "__jsonWriteInt",
        "Internal primitive of jsonEncode: appends one integer. Call jsonEncode instead.",
        ["writer" => "Handle from __jsonWriterOpen", "number" => "The integer"],
        JSON_ENCODE_EXAMPLE,
    ),
    builtin_doc!(
        "__jsonWriteFloat",
        "Internal primitive of jsonEncode: appends one finite float. Call jsonEncode instead.",
        ["writer" => "Handle from __jsonWriterOpen", "number" => "The float"],
        JSON_ENCODE_EXAMPLE,
    ),
    builtin_doc!(
        "__jsonWriterClose",
        "Internal primitive of jsonEncode: closes a writer and returns its document, or the first refusal. Call jsonEncode instead.",
        ["writer" => "Handle from __jsonWriterOpen"],
        JSON_ENCODE_EXAMPLE,
    ),
];

/// `concurrency` built-in documentation. Prose only — types come from the
/// authoritative scheme in `builtins.rs`, joined by name.
pub(crate) static CONCURRENCY: &[BuiltinDoc] = &[
    builtin_doc!(
        "await",
        "Waits for a fiber to finish and returns its result, suspending the current fiber until then.",
        ["fiber" => "The fiber to await"],
        "fn work() = \"done\"\nlet worker = spawn work()\nlet result = await(worker)\nprint(\"${result}; finished=${fiberDone(worker)}\")",
    ),
    builtin_doc!(
        "fiberDone",
        "Returns 1 if the given fiber has finished, 0 otherwise.",
        ["fiber" => "The fiber to test"],
        "fn work() = \"done\"\nlet worker = spawn work()\nlet result = await(worker)\nprint(\"${result}; finished=${fiberDone(worker)}\")",
    ),
    builtin_doc!(
        "yield",
        "Yields control from the current fiber, letting other ready fibers run.",
        [],
        "yield",
    ),
    builtin_doc!(
        "fiber_yield",
        "Yields control to the fiber scheduler and returns its integer value.",
        ["value" => "The value to yield"],
        "let result = fiber_yield(42)",
    ),
    builtin_doc!(
        "Channel",
        "Creates a buffered channel with a positive capacity.",
        ["capacity" => "Positive number of buffered values"],
        "fn receive(channel) = recv(channel)\nlet channel = Channel(1)\nlet worker = spawn receive(channel)\nsend(channel, 42)\nprint(await(worker))",
    ),
    builtin_doc!(
        "send",
        "Blocks while the channel is full, then sends a value and returns Unit.",
        ["channel" => "The channel to send to", "value" => "The value to send"],
        "fn receive(channel) = recv(channel)\nlet channel = Channel(1)\nlet worker = spawn receive(channel)\nsend(channel, 42)\nprint(await(worker))",
    ),
    builtin_doc!(
        "recv",
        "Blocks while the channel is empty, then receives its oldest value.",
        ["channel" => "The channel to receive from"],
        "fn receive(channel) = recv(channel)\nlet channel = Channel(1)\nlet worker = spawn receive(channel)\nsend(channel, 42)\nprint(await(worker))",
    ),
];

/// WebSocket built-in documentation [BUILTIN-WEBSOCKET]. Prose only — types
/// come from the authoritative scheme in `builtins.rs`, joined by name.
pub(crate) static WEBSOCKET: &[BuiltinDoc] = &[
    builtin_doc!(
        "websocketCreateServer",
        "Creates a WebSocket server and returns its handle, or a negative runtime error.",
        ["port" => "Port number to bind to (1-65535)", "address" => "IP address to bind to (e.g., \"127.0.0.1\", \"0.0.0.0\")", "path" => "WebSocket endpoint path (e.g., \"/chat\", \"/live\")"],
        WEBSOCKET_EXAMPLE,
    ),
    builtin_doc!(
        "websocketServerListen",
        "Starts the WebSocket server and returns 0 or a negative runtime error.",
        ["serverID" => "Server identifier from websocketCreateServer"],
        WEBSOCKET_EXAMPLE,
    ),
    builtin_doc!(
        "websocketServerBroadcast",
        "Broadcasts one text frame and returns the number of connections written.",
        ["serverID" => "Server identifier", "message" => "Message to broadcast to all clients"],
        WEBSOCKET_EXAMPLE,
    ),
    builtin_doc!(
        "websocketKeepAlive",
        "Blocks until SIGINT or SIGTERM so server threads remain alive.",
        [],
        "// A long-running server; Ctrl+C stops the keep-alive loop.\nlet server = websocketCreateServer(18899, \"127.0.0.1\", \"/chat\")\nlet listening = if server > 0 { websocketServerListen(server) } else { server }\nprint(\"listen status: ${listening}\")\nif listening == 0 { websocketKeepAlive() } else { print(\"server could not start\") }",
    ),
    builtin_doc!(
        "websocketConnect",
        "Connects to a WebSocket server at the given URL and returns a connection id.",
        ["url" => "WebSocket URL, e.g. \"ws://localhost:8080/chat\""],
        WEBSOCKET_EXAMPLE,
    ),
    builtin_doc!(
        "websocketSend",
        "Sends one text frame and returns 0 or a negative runtime error.",
        ["wsID" => "WebSocket identifier from websocketConnect", "message" => "Message to send"],
        WEBSOCKET_EXAMPLE,
    ),
    builtin_doc!(
        "websocketClose",
        "Closes the WebSocket connection and returns the runtime status.",
        ["wsID" => "WebSocket identifier to close"],
        WEBSOCKET_EXAMPLE,
    ),
];

/// `terminal` built-in documentation. Prose only — types come from the
/// authoritative scheme in `builtins.rs`, joined by name.
pub(crate) static TERMINAL: &[BuiltinDoc] = &[
    builtin_doc!(
        "termReadKey",
        "Reads a single keypress from the terminal and returns it as a string.",
        [],
        RAW_KEY_EXAMPLE,
    ),
    builtin_doc!(
        "termRawMode",
        "Enables (1) or disables (0) raw terminal input mode, so keypresses arrive unbuffered.",
        ["enabled" => "1 to enable raw mode, 0 to restore cooked mode"],
        RAW_KEY_EXAMPLE,
    ),
    builtin_doc!(
        "termCols",
        "Returns the terminal width in columns.",
        [],
        "let width = termCols()",
    ),
    builtin_doc!(
        "termRows",
        "Returns the terminal height in rows.",
        [],
        "let height = termRows()",
    ),
    builtin_doc!(
        "termClear",
        "Clears the terminal screen.",
        [],
        "let cleared = termClear()\nprint(\"clear status: ${cleared}\")",
    ),
    builtin_doc!(
        "termMoveCursor",
        "Moves the terminal cursor to the given row and column.",
        ["row" => "Target row (1-based)", "col" => "Target column (1-based)"],
        CURSOR_EXAMPLE,
    ),
    builtin_doc!(
        "termHideCursor",
        "Hides the terminal cursor.",
        [],
        CURSOR_EXAMPLE,
    ),
    builtin_doc!(
        "termShowCursor",
        "Shows the terminal cursor.",
        [],
        CURSOR_EXAMPLE,
    ),
    builtin_doc!(
        "spawnProcess",
        "Spawns a process and reports stdout, stderr and exit events through the required callback. Returns a handle for the running process.",
        ["command" => "The command to execute", "callback" => "Required callback receiving (processID, eventType, data); event types are stdout (1), stderr (2) and exit (3)"],
        PROCESS_EXAMPLE,
    ),
    builtin_doc!(
        "awaitProcess",
        "Waits for a spawned process to complete and returns its exit code. Blocks until the process finishes.",
        ["handle" => "Process ID from spawnProcess"],
        PROCESS_EXAMPLE,
    ),
    builtin_doc!(
        "cleanupProcess",
        "Cleans up resources associated with a completed process. Should be called after awaitProcess.",
        ["handle" => "Process ID from spawnProcess"],
        PROCESS_EXAMPLE,
    ),
];
