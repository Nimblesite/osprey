---
layout: page
title: "httpPut (Function)"
description: "Makes an HTTP PUT request and returns its status code, or a negative transport error."
---

**Signature:** `httpPut(clientID: int, path: string, body: string, headers: string) -> int`

**Description:** Makes an HTTP PUT request and returns its status code, or a negative transport error.

## Parameters

- **clientID** (int): Client identifier from httpCreateClient
- **path** (string): Request path
- **body** (string): Request body data
- **headers** (string): Additional headers

**Returns:** int

## Example

```osprey
let status = httpPut(clientId, "/put", "{\"updated\":\"data\"}", "Content-Type: application/json")
print("PUT status: ${status}")
```

```osprey-ml
status = httpPut (clientId, "/put", "{\"updated\":\"data\"}", "Content-Type: application/json")
print "PUT status: ${status}"
```
