---
layout: page
title: "websocketClose (Function)"
description: "Closes the WebSocket connection and returns the runtime status."
---

**Signature:** `websocketClose(wsID: int) -> int`

**Description:** Closes the WebSocket connection and returns the runtime status.

## Parameters

- **wsID** (int): WebSocket identifier to close

**Returns:** int

## Example

```osprey
let status = websocketClose(wsId)
print(status)
```

```osprey-ml
status = websocketClose wsId
print status
```
