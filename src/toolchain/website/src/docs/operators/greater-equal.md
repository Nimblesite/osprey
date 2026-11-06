---
layout: page
title: ">= (Greater Than or Equal Operator)"
description: "Checks if the first value is greater than or equal to the second."
---

Checks whether the left value is greater than or equal to the right. Both operands must have the same type: `int`, `float`, `string` or `bool`. Strings use byte order, and `false < true`. An ordered comparison involving NaN returns `false`. See the [ordering rules](/spec/0009-booleanoperations/#what----and--compare-bool-ordering).

## Example

```osprey
let isGreaterOrEqual = 5 >= 5  // isGreaterOrEqual = true
```

```osprey-ml
isGreaterOrEqual = 5 >= 5  // isGreaterOrEqual = true
```
