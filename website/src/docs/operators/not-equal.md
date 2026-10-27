---
layout: page
title: "!= (Inequality Operator)"
description: "Compares two values for inequality."
---

Compares `int`, `float`, `string` or `bool` values of the same type, and supports payload-free union variants and variant tests. NaN is unequal to every float, including itself. Records and lists require an explicit supported `structuralEquals` comparison. See the [equality rules](/spec/0009-booleanoperations/#what--and--compare-bool-equality).

## Example

```osprey
let isNotEqual = 5 != 3  // isNotEqual = true
```

```osprey-ml
isNotEqual = 5 != 3  // isNotEqual = true
```
