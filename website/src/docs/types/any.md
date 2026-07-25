---
layout: page
title: "Any (Type)"
description: "An explicitly dynamic value that must be type-matched before concrete operations"
---

**Description:** An explicitly dynamic value. Match on its runtime type before
using it as a concrete value; direct arithmetic, calls and field access are
rejected.

## Example

```osprey
let value: Any = 42
let text: Any = "Hello"
```

```osprey-ml
value : Any
value = 42

text : Any
text = "Hello"
```
