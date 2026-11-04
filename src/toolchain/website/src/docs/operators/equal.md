---
layout: page
title: "== (Equality Operator)"
description: "Compares two values for equality."
---

Compares `int`, `float`, `string` or `bool` values of the same type. Strings compare by their bytes; NaN is unequal to every float, including itself. Payload-free union variants also support equality and variant tests. Records, lists and other aggregate values require an explicit supported `structuralEquals` comparison. See the [equality rules](/spec/0009-booleanoperations/#what--and--compare-bool-equality).

## Example

```osprey
let isEqual = 5 == 5  // isEqual = true
```

```osprey-ml
isEqual = 5 == 5  // isEqual = true
```
