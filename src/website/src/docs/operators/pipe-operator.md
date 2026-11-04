---
layout: page
mlTwins: manual
title: "|> (Pipe Operator)"
description: "Pass the left value as the first argument of the call on the right."
---

`value |> f(a)` means `f(value, a)`. A bare function on the right receives the value as its argument. Chain calls to make the data flow explicit.

## Example

```osprey
fn double(n) = wrapMul(n, 2)
let values = range(1, 4) |> map(double) |> map(toString) |> toList |> join(",")
let difference = 7 |> wrapSub(2)
print("${values} / ${difference}")
```

```osprey-ml
double n = wrapMul n 2
values = range 1 4 |> map double |> map toString |> toList |> join ","
difference = 7 |> wrapSub 2
print "${values} / ${difference}"
```

Both programs print `2,4,6 / 5`. The subtraction demonstrates argument order: `7 |> wrapSub(2)` subtracts 2 from 7. The total `wrapMul` and `wrapSub` helpers choose wrapping explicitly.
