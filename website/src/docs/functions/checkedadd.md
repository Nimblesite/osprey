---
layout: page
title: "checkedAdd (Function)"
description: "Integer addition that reports overflow instead of wrapping. The `+` operator returns plain int because a wrapped result is still representable; this returns Result<int, MathError>."
---

**Signature:** `checkedAdd(a: int, b: int) -> Result<int, Error>`

**Description:** Integer addition that reports overflow instead of wrapping. The `+` operator returns plain int because a wrapped result is still representable; this returns Result<int, MathError>.

## Parameters

- **a** (int): The first addend
- **b** (int): The second addend

**Returns:** Result<int, Error>

## Example

```osprey
let t = checkedAdd(a: 9223372036854775807, b: 1) ?: 0  // 0 — overflow reported
```

```osprey-ml
t = checkedAdd (a: 9223372036854775807, b: 1) ?: 0  // 0 — overflow reported
```
