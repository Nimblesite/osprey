# WebSockets

## Runtime surface [BUILTIN-WEBSOCKET]

The native runtime exposes a text-frame WebSocket transport. Handles and
operation results are `int`; negative values report failure.

## Client [BUILTIN-WEBSOCKET-CLIENT]

```osprey
websocketConnect(url: string) -> int
websocketSend(wsID: int, message: string) -> int
websocketClose(wsID: int) -> int
```

`websocketConnect` accepts a `ws://` URL and returns a connection handle. It
rejects any other scheme, including `wss://`, and it rejects a URL whose host
or port cannot be parsed. Connection, name resolution and the upgrade exchange
each report their own negative code, and every failure releases the socket
before returning. `websocketSend` returns `0` on success and rejects a handle
that names no live connection. `websocketClose` releases the handle; closing a
handle that is already closed is a no-op rather than an error. The language
surface does not expose a receive callback or incoming-frame iterator, so the
client never reads frames after the upgrade response.

## Server [BUILTIN-WEBSOCKET-SERVER]

```osprey
websocketCreateServer(port: int, address: string, path: string) -> int
websocketServerListen(serverID: int) -> int
websocketServerBroadcast(serverID: int, message: string) -> int
websocketKeepAlive() -> Unit
```

The server accepts upgrade requests, sends a welcome text frame, and echoes
received text payloads. A connection that sends nothing, that is not an upgrade
request, or that omits `Sec-WebSocket-Key` is closed without a response.
`websocketServerBroadcast` returns the number of connections written. The
`path` argument is stored with the server but is not used to filter upgrade
requests.

## Handshake [BUILTIN-WEBSOCKET-HANDSHAKE]

The server answers a valid upgrade with `101 Switching Protocols` and the
RFC 6455 accept token: base64 of `SHA-1(key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11")`.
A key is accepted only when it is exactly 24 base64 characters. Any refusal, and
any digest failure, yields no response rather than a partial one. The client
generates its own key as base64 of 16 random bytes.

## Lifecycle [BUILTIN-WEBSOCKET-LIFECYCLE]

`websocketKeepAlive` blocks until `SIGINT` or `SIGTERM`, then stops every
server the process created and returns.

## Framing limits [BUILTIN-WEBSOCKET-FRAMING]

The runtime handles single text frames only. It does not implement fragmented,
binary, close, ping, or pong frames, and outbound payloads are limited to 4096
bytes. Outbound frames use the one-byte length form below 126 bytes and the
16-bit extended form above it; the 64-bit form is never emitted and is rejected
on input. Inbound masked frames are unmasked with the frame's masking key.
Client frames sent by this runtime are not masked, and the client transport
rejects `wss://`.
