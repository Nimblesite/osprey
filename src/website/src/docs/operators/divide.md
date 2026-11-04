---
layout: page
title: "/ (Division Operator)"
description: "Divides the first number by the second."
---

**Description:** Divides the first number by the second. The result is always a `float`; a zero divisor is sent to the `Arith` handler installed around the code, which chooses the value. A nonzero literal divisor, as below, cannot fail. See [Arithmetic Effects](/spec/0037-arithmeticeffects/).

Both `0.0` and `-0.0` request `Arith.divideByZero`, even when the numerator is NaN or infinity. With a nonzero divisor, IEEE-754 results may include infinity, NaN and signed zero; division does not promise a finite result.

## Example

```osprey
let result = 15 / 3  // result = 5.0
```

```osprey-ml
result = 15 / 3  // result = 5.0
```
