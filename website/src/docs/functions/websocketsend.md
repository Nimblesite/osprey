---
layout: page
title: "websocketSend (Function)"
description: "Sends one text frame and returns 0 or a negative runtime error."
---

**Signature:** `websocketSend(wsID: int, message: string) -> int`

**Description:** Sends one text frame and returns 0 or a negative runtime error.

## Parameters

- **wsID** (int): WebSocket identifier from websocketConnect
- **message** (string): Message to send

**Returns:** int

## Example

```osprey
let status = websocketSend(wsId, "Hello, WebSocket!")
print(status)
```

```osprey-ml
status = websocketSend wsId "Hello, WebSocket!"
print status
```
