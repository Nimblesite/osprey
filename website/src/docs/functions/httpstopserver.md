---
layout: page
title: "httpStopServer (Function)"
description: "Stops the HTTP server and returns the runtime status."
---

**Signature:** `httpStopServer(serverID: int) -> int`

**Description:** Stops the HTTP server and returns the runtime status.

## Parameters

- **serverID** (int): Server identifier to stop

**Returns:** int

## Example

```osprey
let result = httpStopServer(serverId)
print("Server stopped")
```

```osprey-ml
result = httpStopServer serverId
print "Server stopped"
```
