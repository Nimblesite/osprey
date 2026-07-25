---
layout: page
title: "httpCreateClient (Function)"
description: "Creates an HTTP client and returns its handle, or a negative runtime error."
---

**Signature:** `httpCreateClient(base_url: string, timeout: int) -> int`

**Description:** Creates an HTTP client and returns its handle, or a negative runtime error.

## Parameters

- **base_url** (string): Base URL for requests (e.g., "http://api.example.com")
- **timeout** (int): Request timeout in milliseconds

**Returns:** int

## Example

```osprey
let clientId = httpCreateClient("http://httpbin.org", 5000)
print("Client created")
```

```osprey-ml
clientId = httpCreateClient ("http://httpbin.org", 5000)
print "Client created"
```
