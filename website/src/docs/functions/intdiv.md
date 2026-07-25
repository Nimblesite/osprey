---
layout: page
title: "intDiv (Function)"
description: "Truncating integer division (rounds toward zero), divide-by-zero checked. The `/` operator is float-only; this is its integer sibling, returning Result<int, Error>."
---

**Signature:** `intDiv(a: int, b: int) -> Result<int, Error>`

**Description:** Truncating integer division (rounds toward zero), divide-by-zero checked. The `/` operator is float-only; this is its integer sibling, returning Result<int, Error>.

## Parameters

- **a** (int): The dividend
- **b** (int): The divisor (zero yields Error)

**Returns:** Result<int, Error>

## Example

```osprey
fn half(n) = intDiv(n, 2)  // intDiv(7, 2) == 3
```

```osprey-ml
half n = intDiv (n, 2)  // intDiv(7, 2) == 3
```
