---
layout: page
title: "jsonFree (Function)"
description: "Releases a parsed JSON document handle obtained from jsonParse."
---

**Signature:** `jsonFree(document: int) -> Result<int, Error>`

**Description:** Releases a parsed JSON document handle obtained from jsonParse.

## Parameters

- **document** (int): Handle returned by jsonParse

**Returns:** Result<int, Error>

## Example

```osprey
jsonFree(doc)
```

```osprey-ml
jsonFree doc
```
