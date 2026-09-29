---
layout: page
title: "+ (Addition Operator)"
description: "Adds two numbers together."
---

**Description:** Adds two numbers together. Integer operands give an `int`; if the result overflows, the `Arith` handler installed around the code chooses the value. Constant expressions such as the one below are evaluated at compile time. See [Arithmetic Effects](/spec/0037-arithmeticeffects/).

## Example

```osprey
let result = 5 + 3  // result = 8
```

```osprey-ml
result = 5 + 3  // result = 8
```
