---
layout: page
title: "/ (Division Operator)"
description: "Divides the first number by the second."
---

**Description:** Divides the first number by the second. The result is always a `float`; a zero divisor is sent to the `Arith` handler installed around the code, which chooses the value. A nonzero literal divisor, as below, cannot fail. See [Arithmetic Effects](/spec/0037-arithmeticeffects/).

## Example

```osprey
let result = 15 / 3  // result = 5.0
```

```osprey-ml
result = 15 / 3  // result = 5.0
```
