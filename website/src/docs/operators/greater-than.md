---
layout: page
title: "> (Greater Than Operator)"
description: "Checks if the first value is greater than the second."
---

Checks whether the left value is greater than the right. Both operands must have the same type: `int`, `float`, `string` or `bool`. Strings use byte order, and `false < true`. An ordered comparison involving NaN returns `false`. See the [ordering rules](/spec/0009-booleanoperations/#what----and--compare-bool-ordering).

## Example

```osprey
let isGreater = 7 > 3  // isGreater = true
```

```osprey-ml
isGreater = 7 > 3  // isGreater = true
```
