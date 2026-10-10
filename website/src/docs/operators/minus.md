---
layout: page
title: "- (Subtraction Operator)"
description: "Subtracts the second number from the first."
---

**Description:** Subtracts the second number from the first. Integer operands give an `int`; if the result overflows, the `Arith` handler installed around the code chooses the value. Constant expressions such as the one below are evaluated at compile time. See [Arithmetic Effects](/spec/0037-arithmeticeffects/).

With a float operand, the result is a `float`. Floating-point arithmetic follows IEEE-754: infinity and NaN remain values, and underflow preserves subnormal values and signed zero. It does not request an integer overflow policy. See the [float result contract](/spec/0037-arithmeticeffects/#floating-point-results--float-ieee-results).

## Example

```osprey
let result = 10 - 4  // result = 6
```

```osprey-ml
result = 10 - 4  // result = 6
```
