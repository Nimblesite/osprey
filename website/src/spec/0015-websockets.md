---
layout: page
title: "WebSockets"
description: "Osprey Language Specification: WebSockets"
date: 2026-07-12
tags: ["specification", "reference", "documentation"]
author: "Christian Findlay"
permalink: "/spec/0015-websockets/"
---

# WebSockets

Bidirectional WebSocket communication over RFC 6455. Every operation that can fail returns `Result`; see [Error Handling](/spec/0013-errorhandling/).

> **Flavor layer — shared core (AST and above).**  WebSocket streaming is a runtime concern: the functions here are ordinary names, and a call like `websocketSend(wsID: wsID, message: "hello")` lowers to `Expr::Call { function, arguments, named_arguments }` with the result threaded through `Expr::Match`. From the canonical AST (`osprey_ast::Program`) onward — type inference, effect checking, IR lowering, codegen, and the C runtime — nothing inspects which flavor produced the program; WebSocket semantics are flavor-blind. Only the surface spelling of the call differs (the named-argument form shown here is the Default `.osp` surface; the ML `.ospml` whitespace-application counterpart is described in [ML Flavor Syntax](/spec/0024-mlflavorsyntax/)). See [Language Flavors](/spec/0023-languageflavors/).

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

No `main` wrapper is needed — a program is a bare top-level script, and `main` is
synthesised from the trailing statements (identically in both flavors):

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