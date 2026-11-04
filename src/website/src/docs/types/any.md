---
layout: page
mlTwins: manual
title: "any (Type)"
description: "An erased value whose fields are accessed through structural pattern matching."
---

An `any` annotation erases a value's static type. Match its structure before accessing fields; direct arithmetic, calls and field access are rejected. Primitive runtime type tests and unchecked casts are not supported.

## Example

```osprey
type Note = { text: string }
let value: any = Note { text: "Hello" }
print(match value {
    { text } => "note: ${text}"
    _ => "other"
})
```

```osprey-ml
type Note =
    text : string
value : any
value = Note(text = "Hello")
match value
    { text } => print "note: ${text}"
    _ => print "other"
```

Both programs print `note: Hello`. The field pattern selects a matching record shape and binds `text` inside that arm. Include a catch-all: erased scalars have no fields and take the fallback arm.

Erasing a record keeps its fields, including complete `Result` values. A record update preserves every field it does not replace; it also leaves the original record unchanged.

```osprey
type Outcome = { answer: Result<int, Error>, note: string }
let original = Outcome { answer: Error { message: "saved" }, note: "first" }
let copied = original { note: "copy" }
let erased: any = copied
print(toString(erased))
// { answer: Error(saved), note: copy }
```

```osprey-ml
type Outcome =
    answer: Result<int, Error>
    note: string
original = Outcome(answer = Error(message = "saved"), note = "first")
copied = original(note = "copy")
erased : any
erased = copied
print (toString erased)
// { answer: Error(saved), note: copy }
```
