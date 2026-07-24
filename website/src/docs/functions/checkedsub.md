---
layout: page
title: "checkedSub (Function)"
description: "Integer subtraction that reports overflow instead of wrapping, returning Result<int, MathError>. The guarded sibling of `-`."
---

**Signature:** `checkedSub(a: int, b: int) -> Result<int, Error>`

**Description:** Integer subtraction that reports overflow instead of wrapping, returning Result<int, MathError>. The guarded sibling of `-`.

## Parameters

- **a** (int): The minuend
- **b** (int): The subtrahend

**Returns:** Result<int, Error>

## Example

```osprey
let d = checkedSub(a: 10, b: 4) ?: 0  // 6
```

```osprey-ml
d = checkedSub (a: 10, b: 4) ?: 0  // 6
```
