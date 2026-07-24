# WebSockets

Bidirectional WebSocket communication over RFC 6455. Every operation that can fail returns `Result`; see [Error Handling](0013-ErrorHandling.md).

> **Flavor layer — shared core (AST and above).** WebSocket semantics are flavor-blind after lowering to `osprey_ast::Program`; no later phase inspects the source flavor. Examples use Default named-argument calls; see [ML Flavor Syntax](0024-MLFlavorSyntax.md) and [Language Flavors](0023-LanguageFlavors.md).

## Status

Function signatures below are the specified interface. The current C runtime returns raw `int64_t` for several of these functions; the type system expects `Result<T, string>` and the bridge is being aligned. WebSocket server `listen` currently fails to bind in some environments.

## Types

```osprey
type WebSocketID = int
type ServerID    = int

type WebSocketMessage = {
    type:      string,
    data:      string,
    timestamp: int
}

type WebSocketConnection = {
    id:          WebSocketID,
    url:         string,
    isConnected: bool
}
```

## Client Functions

```osprey
websocketConnect(
    url: string,
    messageHandler: fn(string) -> Result<unit, string>
) -> Result<WebSocketID, string>

websocketSend(wsID: WebSocketID, message: string) -> Result<unit, string>
websocketClose(wsID: WebSocketID)                  -> Result<unit, string>
```

`messageHandler` is invoked once per incoming frame with the frame payload.

```osprey
fn handleMessage(msg) -> Result<unit, string> = {
    print("received: ${msg}")
    Success { value: () }
}

match websocketConnect(url: "ws://localhost:8080/chat", messageHandler: handleMessage) {
    Success { value: wsID } => {
        websocketSend(wsID: wsID, message: "hello")
        websocketClose(wsID: wsID)
    }
    Error { message } => print("connect failed: ${message}")
}
```

## Server Functions

```osprey
websocketCreateServer(
    port: int, address: string, path: string
) -> Result<ServerID, string>

websocketServerListen(serverID: ServerID)                         -> Result<unit, string>
websocketServerSend(serverID: ServerID, wsID: WebSocketID,
                    message: string)                              -> Result<unit, string>
websocketServerBroadcast(serverID: ServerID, message: string)     -> Result<unit, string>
websocketStopServer(serverID: ServerID)                           -> Result<unit, string>
```

## Server Example

```osprey
match websocketCreateServer(port: 8080, address: "127.0.0.1", path: "/chat") {
    Success { value: serverID } => match websocketServerListen(serverID: serverID) {
        Success { value: _ } => {
            websocketServerBroadcast(serverID: serverID, message: "Welcome!")
            sleep(10000)
            websocketStopServer(serverID: serverID)
        }
        Error { message } => print("listen failed: ${message}")
    }
    Error { message } => print("create failed: ${message}")
}
```
