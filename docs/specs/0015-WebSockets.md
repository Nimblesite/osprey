# WebSockets

## Runtime surface [BUILTIN-WEBSOCKET]

The native runtime exposes a text-frame WebSocket transport. Handles and
operation results are `int`; negative values report failure.

## Client

```osprey
websocketConnect(url: string) -> int
websocketSend(wsID: int, message: string) -> int
websocketClose(wsID: int) -> int
```

`websocketConnect` accepts a `ws://` URL and returns a connection handle.
`websocketSend` returns `0` on success. The language surface does not expose a
receive callback or incoming-frame iterator.

## Server

```osprey
websocketCreateServer(port: int, address: string, path: string) -> int
websocketServerListen(serverID: int) -> int
websocketServerBroadcast(serverID: int, message: string) -> int
websocketKeepAlive() -> Unit
```

The server accepts upgrade requests, sends a welcome text frame, and echoes
received text payloads. `websocketServerBroadcast` returns the number of
connections written. `websocketKeepAlive` blocks until `SIGINT` or `SIGTERM`.
The `path` argument is stored with the server but is not currently used to
filter upgrade requests.

## Framing limits

The runtime handles single text frames only. It does not implement fragmented,
binary, close, ping, or pong frames, and outbound payloads are limited to 4096
bytes. Client frames are not masked, and the client transport rejects `wss://`.
