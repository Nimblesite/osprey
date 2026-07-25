---
layout: page
title: "httpGet (Function)"
description: "Makes an HTTP GET request and returns its status code, or a negative transport error."
---

**Signature:** `httpGet(clientID: int, path: string, headers: string) -> int`

**Description:** Makes an HTTP GET request and returns its status code, or a negative transport error.

## Parameters

- **clientID** (int): Client identifier from httpCreateClient
- **path** (string): Request path (e.g., "/api/users")
- **headers** (string): Additional headers (e.g., "Authorization: Bearer token")

**Returns:** int

## Example

```osprey
let status = httpGet(clientId, "/get", "")
print("GET request status: ${status}")
```

```osprey-ml
status = httpGet (clientId, "/get", "")
print "GET request status: ${status}"
```
