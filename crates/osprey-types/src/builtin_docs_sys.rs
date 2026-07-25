//! Built-in documentation data (system, network & runtime). Generated companion to
//! `builtins.rs`: every entry's prose pairs with the type scheme of the
//! same name. Edit prose here; edit types in `builtins.rs`. The parity
//! test in `builtin_docs.rs` guarantees the two stay in lockstep.
//!
//! Param order and count MUST match the builtin's real arity.

use crate::builtin_docs::BuiltinDoc;
use crate::builtin_docs_lang::builtin_doc;

/// `files` built-in documentation. Prose only — types come from the
/// authoritative scheme in `builtins.rs`, joined by name.
pub(crate) static FILES: &[BuiltinDoc] = &[
    builtin_doc!(
        "readFile",
        "Reads the entire contents of a file as a string.",
        ["filename" => "Path to the file to read"],
        "let content = readFile(\"input.txt\")\nprint(\"File read\")",
    ),
    builtin_doc!(
        "writeFile",
        "Writes content to a file. Creates the file if it doesn't exist. Returns number of bytes written.",
        ["filename" => "Path to the file to write", "content" => "Content to write to the file"],
        "let result = writeFile(\"output.txt\", \"Hello, World!\")\nprint(\"File written\")",
    ),
];

/// HTTP built-in documentation [BUILTIN-HTTP] [HTTP-RESPONSE-HANDLE]. Prose
/// only — types come from the authoritative scheme in `builtins.rs`.
pub(crate) static HTTP: &[BuiltinDoc] = &[
    builtin_doc!(
        "httpCreateClient",
        "Creates an HTTP client and returns its handle, or a negative runtime error.",
        ["base_url" => "Base URL for requests (e.g., \"http://api.example.com\")", "timeout" => "Request timeout in milliseconds"],
        "let clientId = httpCreateClient(\"http://httpbin.org\", 5000)\nprint(\"Client created\")",
    ),
    builtin_doc!(
        "httpCloseClient",
        "Closes the HTTP client and returns the runtime status.",
        ["clientID" => "Client identifier to close"],
        "let result = httpCloseClient(clientId)\nprint(\"Client closed\")",
    ),
    builtin_doc!(
        "httpGet",
        "Makes an HTTP GET request and returns its status code, or a negative transport error.",
        ["clientID" => "Client identifier from httpCreateClient", "path" => "Request path (e.g., \"/api/users\")", "headers" => "Additional headers (e.g., \"Authorization: Bearer token\")"],
        "let status = httpGet(clientId, \"/get\", \"\")\nprint(\"GET request status: ${status}\")",
    ),
    builtin_doc!(
        "httpGetResponse",
        "Sends an HTTP GET request and returns a response handle for inspecting the status, headers, and body.",
        ["clientID" => "Client identifier from httpCreateClient", "path" => "Request path, e.g. \"/api/users\"", "headers" => "Additional request headers, or \"\" for none"],
        "match httpGetResponse(client, \"/users\", \"\") {\n  Success { value } => print(\"status: ${httpResponseStatus(value)}\")\n  Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "httpResponseBody",
        "Returns the body of a response handle as a string.",
        ["responseID" => "Handle returned by httpGetResponse"],
        "match httpResponseBody(response) {\n  Success { value } => print(value)\n  Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "httpResponseFree",
        "Releases a response handle; an invalid handle or double free returns Error.",
        ["responseID" => "Handle returned by httpGetResponse"],
        "match httpResponseFree(response) {\n  Success { value } => print(\"released\")\n  Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "httpResponseStatus",
        "Returns the HTTP status code of a response handle.",
        ["responseID" => "Handle returned by httpGetResponse"],
        "let code = httpResponseStatus(response)  // 200",
    ),
    builtin_doc!(
        "httpResponseHeader",
        "Returns the value of the named header from a response handle.",
        ["responseID" => "Handle returned by httpGetResponse", "name" => "Header name, e.g. \"Content-Type\""],
        "match httpResponseHeader(response, \"Content-Type\") {\n  Success { value } => print(value)\n  Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "httpPost",
        "Makes an HTTP POST request and returns its status code, or a negative transport error.",
        ["clientID" => "Client identifier from httpCreateClient", "path" => "Request path", "body" => "Request body data", "headers" => "Additional headers"],
        "let status = httpPost(clientId, \"/post\", \"{\\\"key\\\":\\\"value\\\"}\", \"Content-Type: application/json\")\nprint(\"POST status: ${status}\")",
    ),
    builtin_doc!(
        "httpPut",
        "Makes an HTTP PUT request and returns its status code, or a negative transport error.",
        ["clientID" => "Client identifier from httpCreateClient", "path" => "Request path", "body" => "Request body data", "headers" => "Additional headers"],
        "let status = httpPut(clientId, \"/put\", \"{\\\"updated\\\":\\\"data\\\"}\", \"Content-Type: application/json\")\nprint(\"PUT status: ${status}\")",
    ),
    builtin_doc!(
        "httpDelete",
        "Makes an HTTP DELETE request and returns its status code, or a negative transport error.",
        ["clientID" => "Client identifier from httpCreateClient", "path" => "Request path", "headers" => "Additional headers"],
        "let status = httpDelete(clientId, \"/delete\", \"\")\nprint(\"DELETE status: ${status}\")",
    ),
    builtin_doc!(
        "httpCreateServer",
        "Creates an HTTP server bound to the specified port and address.",
        ["port" => "Port number to bind to (1-65535)", "address" => "IP address to bind to (e.g., \"127.0.0.1\", \"0.0.0.0\")"],
        "let serverId = httpCreateServer(8080, \"127.0.0.1\")\nprint(\"Server created with ID: ${serverId}\")",
    ),
    builtin_doc!(
        "httpListen",
        "Starts the HTTP server with a request handler and returns 0 or a negative runtime error.",
        ["serverID" => "Server identifier from httpCreateServer", "handler" => "Request handler function"],
        "let result = httpListen(serverId, requestHandler)\nprint(\"Server listening\")",
    ),
    builtin_doc!(
        "httpStopServer",
        "Stops the HTTP server and returns the runtime status.",
        ["serverID" => "Server identifier to stop"],
        "let result = httpStopServer(serverId)\nprint(\"Server stopped\")",
    ),
];

/// `json` built-in documentation. Prose only — types come from the
/// authoritative scheme in `builtins.rs`, joined by name.
pub(crate) static JSON: &[BuiltinDoc] = &[
    builtin_doc!(
        "jsonParse",
        "Parses a JSON string and returns an opaque document handle for querying, or an error on malformed input.",
        ["text" => "The JSON text to parse"],
        "match jsonParse(\"{\\\"name\\\": \\\"osprey\\\"}\") {\n  Success { value } => print(\"parsed\")\n  Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "jsonGet",
        "Returns the string value at the given path within a parsed JSON document.",
        ["document" => "Handle returned by jsonParse", "path" => "Dotted path to the value, e.g. \"user.name\""],
        "match jsonGet(doc, \"name\") {\n  Success { value } => print(value)\n  Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "jsonLength",
        "Returns the number of elements in the JSON array at the given path.",
        ["document" => "Handle returned by jsonParse", "path" => "Dotted path to the array"],
        "let n = jsonLength(doc, \"items\")",
    ),
    builtin_doc!(
        "jsonFree",
        "Releases a parsed JSON document handle obtained from jsonParse.",
        ["document" => "Handle returned by jsonParse"],
        "jsonFree(doc)",
    ),
];

/// `concurrency` built-in documentation. Prose only — types come from the
/// authoritative scheme in `builtins.rs`, joined by name.
pub(crate) static CONCURRENCY: &[BuiltinDoc] = &[
    builtin_doc!(
        "await",
        "Waits for a fiber to finish and returns its result, suspending the current fiber until then.",
        ["fiber" => "The fiber to await"],
        "let result = await(worker)",
    ),
    builtin_doc!(
        "fiberDone",
        "Returns 1 if the given fiber has finished, 0 otherwise.",
        ["fiber" => "The fiber to test"],
        "let finished = fiberDone(worker)  // 0 or 1",
    ),
    builtin_doc!(
        "yield",
        "Yields control from the current fiber, letting other ready fibers run.",
        [],
        "yield",
    ),
    builtin_doc!(
        "fiber_yield",
        "Yields control to the fiber scheduler with an optional value.",
        ["value" => "The value to yield"],
        "let result = fiber_yield(42)",
    ),
    builtin_doc!(
        "Channel",
        "Creates a new channel with the specified capacity.",
        ["capacity" => "The capacity of the channel"],
        "let ch = Channel(10)",
    ),
    builtin_doc!(
        "send",
        "Sends a value to a channel. Returns 1 for success, 0 for failure.",
        ["channel" => "The channel to send to", "value" => "The value to send"],
        "let success = send(ch, 42)",
    ),
    builtin_doc!(
        "recv",
        "Receives a value from a channel.",
        ["channel" => "The channel to receive from"],
        "let value = recv(ch)",
    ),
];

/// WebSocket built-in documentation [BUILTIN-WEBSOCKET]. Prose only — types
/// come from the authoritative scheme in `builtins.rs`, joined by name.
pub(crate) static WEBSOCKET: &[BuiltinDoc] = &[
    builtin_doc!(
        "websocketCreateServer",
        "Creates a WebSocket server and returns its handle, or a negative runtime error.",
        ["port" => "Port number to bind to (1-65535)", "address" => "IP address to bind to (e.g., \"127.0.0.1\", \"0.0.0.0\")", "path" => "WebSocket endpoint path (e.g., \"/chat\", \"/live\")"],
        "let serverId = websocketCreateServer(8080, \"127.0.0.1\", \"/chat\")\nprint(serverId)",
    ),
    builtin_doc!(
        "websocketServerListen",
        "Starts the WebSocket server and returns 0 or a negative runtime error.",
        ["serverID" => "Server identifier from websocketCreateServer"],
        "let status = websocketServerListen(serverId)\nprint(status)",
    ),
    builtin_doc!(
        "websocketServerBroadcast",
        "Broadcasts one text frame and returns the number of connections written.",
        ["serverID" => "Server identifier", "message" => "Message to broadcast to all clients"],
        "let sent = websocketServerBroadcast(serverId, \"Welcome!\")\nprint(sent)",
    ),
    builtin_doc!(
        "websocketKeepAlive",
        "Blocks until SIGINT or SIGTERM so server threads remain alive.",
        [],
        "websocketKeepAlive()  // Blocks until Ctrl+C",
    ),
    builtin_doc!(
        "websocketConnect",
        "Connects to a WebSocket server at the given URL and returns a connection id.",
        ["url" => "WebSocket URL, e.g. \"ws://localhost:8080/chat\""],
        "let conn = websocketConnect(\"ws://localhost:8080/chat\")",
    ),
    builtin_doc!(
        "websocketSend",
        "Sends one text frame and returns 0 or a negative runtime error.",
        ["wsID" => "WebSocket identifier from websocketConnect", "message" => "Message to send"],
        "let status = websocketSend(wsId, \"Hello, WebSocket!\")\nprint(status)",
    ),
    builtin_doc!(
        "websocketClose",
        "Closes the WebSocket connection and returns the runtime status.",
        ["wsID" => "WebSocket identifier to close"],
        "let status = websocketClose(wsId)\nprint(status)",
    ),
];

/// `terminal` built-in documentation. Prose only — types come from the
/// authoritative scheme in `builtins.rs`, joined by name.
pub(crate) static TERMINAL: &[BuiltinDoc] = &[
    builtin_doc!(
        "termReadKey",
        "Reads a single keypress from the terminal and returns it as a string.",
        [],
        "match termReadKey() {\n  Success { value } => print(\"key: ${value}\")\n  Error { message } => print(message)\n}",
    ),
    builtin_doc!(
        "termRawMode",
        "Enables (1) or disables (0) raw terminal input mode, so keypresses arrive unbuffered.",
        ["enabled" => "1 to enable raw mode, 0 to restore cooked mode"],
        "termRawMode(1)",
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
        "termClear()",
    ),
    builtin_doc!(
        "termMoveCursor",
        "Moves the terminal cursor to the given row and column.",
        ["row" => "Target row (1-based)", "col" => "Target column (1-based)"],
        "termMoveCursor(1, 1)",
    ),
    builtin_doc!(
        "termHideCursor",
        "Hides the terminal cursor.",
        [],
        "termHideCursor()",
    ),
    builtin_doc!(
        "termShowCursor",
        "Shows the terminal cursor.",
        [],
        "termShowCursor()",
    ),
    builtin_doc!(
        "spawnProcess",
        "Spawns an external async process with MANDATORY callback for stdout/stderr capture. The callback function receives (processID: int, eventType: int, data: string) and is called for stdout (1), stderr (2), and exit (3) events. Returns a handle for the running process. CALLBACK IS REQUIRED - NO FUNCTION OVERLOADING!",
        ["command" => "The command to execute", "callback" => "MANDATORY callback function for process events (processID, eventType, data)"],
        "fn processEventHandler(processID: int, eventType: int, data: string) -> Unit = {\n    match eventType {\n        1 => print(\"STDOUT: ${data}\")\n        2 => print(\"STDERR: ${data}\")\n        3 => print(\"EXIT: ${data}\")\n        _ => print(\"Unknown event\")\n    }\n}\nlet result = spawnProcess(\"echo hello\", processEventHandler)\nmatch result {\n    Success { value } => {\n        let exitCode = awaitProcess(value)\n        cleanupProcess(value)\n    }\n    Error { message } => print(\"Failed\")\n}",
    ),
    builtin_doc!(
        "awaitProcess",
        "Waits for a spawned process to complete and returns its exit code. Blocks until the process finishes.",
        ["handle" => "Process ID from spawnProcess"],
        "let exitCode = awaitProcess(processHandle)\nprint(\"Process exited with code: ${toString(exitCode)}\")",
    ),
    builtin_doc!(
        "cleanupProcess",
        "Cleans up resources associated with a completed process. Should be called after awaitProcess.",
        ["handle" => "Process ID from spawnProcess"],
        "cleanupProcess(processHandle)  // Free process resources",
    ),
];
