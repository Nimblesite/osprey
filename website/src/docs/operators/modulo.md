---
layout: page
title: "% (Modulo Operator)"
description: "Returns the remainder of dividing the first number by the second."
---

**Description:** Returns the remainder of dividing the first number by the second. Integer operands give an `int`; a zero divisor is sent to the `Arith` handler installed around the code, which chooses the value. A nonzero literal divisor, as below, cannot fail. See [Arithmetic Effects](/spec/0037-arithmeticeffects/).

## Example

```osprey
let result = 17 % 5  // result = 2
```

```osprey-ml
result = 17 % 5  // result = 2
```
