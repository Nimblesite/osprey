---
layout: page
title: "* (Multiplication Operator)"
description: "Multiplies two numbers."
---

**Description:** Multiplies two numbers. Integer operands give an `int`; if the result overflows, the `Arith` handler installed around the code chooses the value. Constant expressions such as the one below are evaluated at compile time. See [Arithmetic Effects](/spec/0037-arithmeticeffects/).

## Example

```osprey
let result = 6 * 7  // result = 42
```

```osprey-ml
result = 6 * 7  // result = 42
```
