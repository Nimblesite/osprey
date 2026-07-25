---
layout: page
title: "websocketKeepAlive (Function)"
description: "Blocks until SIGINT or SIGTERM so server threads remain alive."
---

**Signature:** `websocketKeepAlive() -> Unit`

**Description:** Blocks until SIGINT or SIGTERM so server threads remain alive.

**Returns:** Unit

## Example

```osprey
websocketKeepAlive()  // Blocks until Ctrl+C
```

```osprey-ml
websocketKeepAlive ()  // Blocks until Ctrl+C
```
