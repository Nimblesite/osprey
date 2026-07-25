---
layout: page
title: "websocketCreateServer (Function)"
description: "Creates a WebSocket server and returns its handle, or a negative runtime error."
---

**Signature:** `websocketCreateServer(port: int, address: string, path: string) -> int`

**Description:** Creates a WebSocket server and returns its handle, or a negative runtime error.

## Parameters

- **port** (int): Port number to bind to (1-65535)
- **address** (string): IP address to bind to (e.g., "127.0.0.1", "0.0.0.0")
- **path** (string): WebSocket endpoint path (e.g., "/chat", "/live")

**Returns:** int

## Example

```osprey
let serverId = websocketCreateServer(8080, "127.0.0.1", "/chat")
print(serverId)
```

```osprey-ml
serverId = websocketCreateServer 8080 "127.0.0.1" "/chat"
print serverId
```
