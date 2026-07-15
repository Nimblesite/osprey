---
layout: page
title: "expect (Function)"
description: "Asserts two values are equal (canonical-string equality, Results auto-unwrapped). On mismatch, marks the enclosing test failed and prints a diagnostic; execution continues."
---

**Signature:** `expect(actual: any, expected: any) -> Unit`

**Description:** Asserts two values are equal (canonical-string equality, Results auto-unwrapped). On mismatch, marks the enclosing test failed and prints a diagnostic; execution continues.

## Parameters

- **actual** (any): The computed value
- **expected** (any): The value it should equal

**Returns:** Unit

## Example

```osprey
test("doubling", fn() => expect(21 * 2, 42))
```

```osprey-ml
test ("doubling", \() => expect (21 * 2, 42))
```
