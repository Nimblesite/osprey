---
layout: page
title: "let (Keyword)"
description: "Bind an immutable value in Default syntax; its type is inferred."
---

`let` binds an immutable value in Default syntax. Its type is inferred from the expression. ML and Python syntax omit `let` for ordinary bindings. Mutable cells use `mut`, with reassignment restricted to effect handlers; see [bindings](/spec/0003-syntax/#bindings).

## Example

```osprey
let name = "Alice"
let age = 25
let isActive = true
```

```osprey-ml
name = "Alice"
age = 25
isActive = true
```
