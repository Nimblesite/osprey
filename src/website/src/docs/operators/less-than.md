---
layout: page
title: "< (Less Than Operator)"
description: "Checks if the first value is less than the second."
---

Checks whether the left value is less than the right. Both operands must have the same type: `int`, `float`, `string` or `bool`. Strings use byte order, and `false < true`. An ordered comparison involving NaN returns `false`. See the [ordering rules](/spec/0009-booleanoperations/#what----and--compare-bool-ordering).

## Example

```osprey
let isLess = 3 < 5  // isLess = true
```

```osprey-ml
isLess = 3 < 5  // isLess = true
```
