---
layout: page
title: "websocketServerListen (Function)"
description: "Starts the WebSocket server and returns 0 or a negative runtime error."
---

**Signature:** `websocketServerListen(serverID: int) -> int`

**Description:** Starts the WebSocket server and returns 0 or a negative runtime error.

## Parameters

- **serverID** (int): Server identifier from websocketCreateServer

**Returns:** int

## Example

```osprey
let status = websocketServerListen(serverId)
print(status)
```

```osprey-ml
status = websocketServerListen serverId
print status
```
