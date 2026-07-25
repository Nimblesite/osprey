---
layout: page
title: "httpDelete (Function)"
description: "Makes an HTTP DELETE request and returns its status code, or a negative transport error."
---

**Signature:** `httpDelete(clientID: int, path: string, headers: string) -> int`

**Description:** Makes an HTTP DELETE request and returns its status code, or a negative transport error.

## Parameters

- **clientID** (int): Client identifier from httpCreateClient
- **path** (string): Request path
- **headers** (string): Additional headers

**Returns:** int

## Example

```osprey
let status = httpDelete(clientId, "/delete", "")
print("DELETE status: ${status}")
```

```osprey-ml
status = httpDelete (clientId, "/delete", "")
print "DELETE status: ${status}"
```
