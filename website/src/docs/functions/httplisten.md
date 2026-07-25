---
layout: page
title: "httpListen (Function)"
description: "Starts the HTTP server with a request handler and returns 0 or a negative runtime error."
---

**Signature:** `httpListen(serverID: int, handler: any) -> int`

**Description:** Starts the HTTP server with a request handler and returns 0 or a negative runtime error.

## Parameters

- **serverID** (int): Server identifier from httpCreateServer
- **handler** (any): Request handler function

**Returns:** int

## Example

```osprey
let result = httpListen(serverId, requestHandler)
print("Server listening")
```

```osprey-ml
result = httpListen (serverId, requestHandler)
print "Server listening"
```
