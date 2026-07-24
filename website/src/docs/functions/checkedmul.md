---
layout: page
title: "checkedMul (Function)"
description: "Integer multiplication that reports overflow instead of wrapping, returning Result<int, MathError>. The guarded sibling of `*`."
---

**Signature:** `checkedMul(a: int, b: int) -> Result<int, Error>`

**Description:** Integer multiplication that reports overflow instead of wrapping, returning Result<int, MathError>. The guarded sibling of `*`.

## Parameters

- **a** (int): The first factor
- **b** (int): The second factor

**Returns:** Result<int, Error>

## Example

```osprey
let p = checkedMul(a: 6, b: 7) ?: 0  // 42
```

```osprey-ml
p = checkedMul (a: 6, b: 7) ?: 0  // 42
```
