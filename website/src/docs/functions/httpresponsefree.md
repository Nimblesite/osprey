---
layout: page
title: "httpResponseFree (Function)"
description: "Releases a response handle; an invalid handle or double free returns Error."
---

**Signature:** `httpResponseFree(responseID: int) -> Result<int, Error>`

**Description:** Releases a response handle; an invalid handle or double free returns Error.

## Parameters

- **responseID** (int): Handle returned by httpGetResponse

**Returns:** Result<int, Error>

## Example

```osprey
match httpResponseFree(response) {
  Success { value } => print("released")
  Error { message } => print(message)
}
```

```osprey-ml
match httpResponseFree response
    Success value => print "released"
    Error message => print message
```
