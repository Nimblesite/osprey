---
layout: page
title: "% (Modulo Operator)"
description: "Returns the remainder of dividing the first number by the second."
---

**Description:** Returns the remainder of dividing the first number by the second. Integer operands give an `int`; a zero divisor is sent to the `Arith` handler installed around the code, which chooses the value. A nonzero literal divisor, as below, cannot fail. See [Arithmetic Effects](/spec/0037-arithmeticeffects/).

With a float operand, `%` returns a `float` remainder with the dividend's sign: `-5.5 % 2.0` is `-1.5`. Either signed zero divisor requests `Arith.divideByZero`; integer zero divisors request `Arith.remainderByZero`. Infinite dividends and NaN operands can produce NaN. See the [float result contract](/spec/0037-arithmeticeffects/#floating-point-results--float-ieee-results).

## Example

```osprey
let result = 17 % 5  // result = 2
```

```osprey-ml
result = 17 % 5  // result = 2
```
