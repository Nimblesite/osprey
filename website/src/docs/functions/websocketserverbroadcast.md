---
layout: page
title: "websocketServerBroadcast (Function)"
description: "Broadcasts one text frame and returns the number of connections written."
---

**Signature:** `websocketServerBroadcast(serverID: int, message: string) -> int`

**Description:** Broadcasts one text frame and returns the number of connections written.

## Parameters

- **serverID** (int): Server identifier
- **message** (string): Message to broadcast to all clients

**Returns:** int

## Example

```osprey
let sent = websocketServerBroadcast(serverId, "Welcome!")
print(sent)
```

```osprey-ml
sent = websocketServerBroadcast serverId "Welcome!"
print sent
```
