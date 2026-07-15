---
layout: page
title: "check (Function)"
description: "Labeled equality assertion in Alcotest argument order (expected before actual). Behaves exactly like expect, with the label in the failure diagnostic."
---

**Signature:** `check(label: string, expected: any, actual: any) -> Unit`

**Description:** Labeled equality assertion in Alcotest argument order (expected before actual). Behaves exactly like expect, with the label in the failure diagnostic.

## Parameters

- **label** (string): A short description of what is being checked
- **expected** (any): The value the actual must equal
- **actual** (any): The computed value

**Returns:** Unit

## Example

```osprey
test("doubling", fn() => check("double", 42, 21 * 2))
```

```osprey-ml
test ("doubling", \() => check ("double", 42, 21 * 2))
```
