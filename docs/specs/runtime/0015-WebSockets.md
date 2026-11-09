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

A server's connection list holds its live connections and nothing else. The
welcome frame comes first, and a client that receives it is ready for broadcasts.
A connection leaves the list before its descriptor closes, so `websocketServerBroadcast`
counts and writes to live connections only, and no number of connections over a
server's life uses up the 1,000 it can hold at once. A broadcast to a peer whose
socket has stayed full for a second doesn't count that peer, and ends its
connection, as it has missed a frame.

Stopping a server ends its connections and waits for the threads serving them
before it returns. A connection that hasn't finished its handshake ends within a
second.

## Framing [BUILTIN-WEBSOCKET-FRAMING]

Frames follow RFC 6455. The parser reads all three payload length forms: one
byte, 16 bits and 64 bits. It reports each frame's FIN bit and opcode, and it
takes frames from a byte stream, so a frame split across TCP segments, or
several frames in one segment, arrive intact. It refuses reserved bits, unknown
opcodes, fragmented control frames, control payloads over 125 bytes, lengths
not in their shortest form, and 64-bit lengths with the top bit set.

The server reads each connection as follows:

- **Masking:** client frames must be masked. An unmasked frame is closed with
  status 1002 (protocol error).
- **Fragments:** a text message sent in fragments is delivered once, whole.
  Continuing nothing, or starting a message while another is in fragments,
  closes with 1002.
- **Ping and pong:** a ping is answered with a pong carrying the same payload,
  even between the fragments of a message. An unsolicited pong gets no answer.
- **Close:** a close frame is answered with the same status code, then the
  connection ends. A one-byte close payload, or a status code a peer may not
  send, closes with 1002.
- **Binary:** binary messages are not taken and close with 1003.
- **Size limit:** a frame, or a message assembled from fragments, over 16384
  bytes closes with 1009.
- **Out of memory:** an allocation that fails while a frame is read closes the
  connection with 1011.

Text payloads are not checked for valid UTF-8.

Outbound payloads are limited to 4096 bytes, so the 64-bit length form is never
sent. Server frames are unmasked. The client masks every frame with a fresh
random key, but it never reads frames and it rejects `wss://`.
