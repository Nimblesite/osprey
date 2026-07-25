---
layout: page
title: "httpCloseClient (Function)"
description: "Closes the HTTP client and returns the runtime status."
---

**Signature:** `httpCloseClient(clientID: int) -> int`

**Description:** Closes the HTTP client and returns the runtime status.

## Parameters

- **clientID** (int): Client identifier to close

**Returns:** int

## Example

```osprey
let result = httpCloseClient(clientId)
print("Client closed")
```

```osprey-ml
result = httpCloseClient clientId
print "Client closed"
```
