---
layout: page
title: "type (Keyword)"
description: "Type declaration keyword. Used to define custom types and type aliases."
---

`type` declares an alias, tagged union or record type. Here `UserId` names `int`, `Status` has two variants, and `User` has named fields.

## Example

```osprey
type UserId = int
type Status = Active | Inactive
type User = { name: string, age: int }
```

```osprey-ml
type UserId =
    int

type Status =
    Active
    Inactive

type User =
    name : string
    age : int
```
