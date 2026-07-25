---
layout: page
title: "httpPost (Function)"
description: "Makes an HTTP POST request and returns its status code, or a negative transport error."
---

**Signature:** `httpPost(clientID: int, path: string, body: string, headers: string) -> int`

**Description:** Makes an HTTP POST request and returns its status code, or a negative transport error.

## Parameters

- **clientID** (int): Client identifier from httpCreateClient
- **path** (string): Request path
- **body** (string): Request body data
- **headers** (string): Additional headers

**Returns:** int

## Example

```osprey
let status = httpPost(clientId, "/post", "{\"key\":\"value\"}", "Content-Type: application/json")
print("POST status: ${status}")
```

```osprey-ml
status = httpPost (clientId, "/post", "{\"key\":\"value\"}", "Content-Type: application/json")
print "POST status: ${status}"
```
