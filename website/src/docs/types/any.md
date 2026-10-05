---
layout: page
title: "any (Type)"
description: "An explicitly dynamic value that must be type-matched before concrete operations"
---

**Description:** An explicitly dynamic value. Match on its runtime type before
using it as a concrete value; direct arithmetic, calls and field access are
rejected.

## Example

```osprey
let value: any = 42
let text: any = "Hello"
```

```osprey-ml
value : Any
value = 42

text : Any
text = "Hello"
```

Erasing a record keeps its fields, including complete `Result` values. A record update preserves every field it does not replace; it also leaves the original record unchanged.

```osprey
type Outcome = { answer: Result<int, Error>, note: string }
let original = Outcome { answer: Error { message: "saved" }, note: "first" }
let copied = original { note: "copy" }
let erased: any = copied
print(toString(erased))
// { answer: Error(saved), note: copy }
```
