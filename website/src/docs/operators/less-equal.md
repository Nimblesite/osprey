---
layout: page
title: "<= (Less Than or Equal Operator)"
description: "Checks if the first value is less than or equal to the second."
---

Checks whether the left value is less than or equal to the right. Both operands must have the same type: `int`, `float`, `string` or `bool`. Strings use byte order, and `false < true`. An ordered comparison involving NaN returns `false`. See the [ordering rules](/spec/0009-booleanoperations/#what----and--compare-bool-ordering).

## Example

```osprey
let isLessOrEqual = 5 <= 5  // isLessOrEqual = true
```

```osprey-ml
isLessOrEqual = 5 <= 5  // isLessOrEqual = true
```
