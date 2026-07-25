---
layout: page
title: "randomBelow (Function)"
description: "A cryptographically-secure uniform random integer in [0, n), unbiased by rejection sampling. Returns Result<int, Error> when n is positive and Error otherwise."
---

**Signature:** `randomBelow(n: int) -> Result<int, Error>`

**Description:** A cryptographically-secure uniform random integer in [0, n), unbiased by rejection sampling. Returns Result<int, Error> when n is positive and Error otherwise.

## Parameters

- **n** (int): Exclusive upper bound; must be positive

**Returns:** Result<int, Error>

## Example

```osprey
let d = randomBelow(6) ?: 0  // a fair die face 0..5
```

```osprey-ml
d =
    match randomBelow 6
        Success value => value  // a fair die face 0..5
        Error message => 0
```
