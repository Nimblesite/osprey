---
layout: page
title: "test (Function)"
description: "Runs `body` as one named test case and prints a TAP result line. A case fails when any assertion inside it fails; the program exits non-zero if any case failed."
---

**Signature:** `test(name: string, body: () -> t0) -> Unit`

**Description:** Runs `body` as one named test case and prints a TAP result line. A case fails when any assertion inside it fails; the program exits non-zero if any case failed.

## Parameters

- **name** (string): The test case's name
- **body** (() -> t0): A zero-parameter function containing the case's assertions

**Returns:** Unit

## Example

```osprey
test("addition works", fn() => expect(2 + 3, 5))
```

```osprey-ml
test ("addition works", \() => expect (2 + 3, 5))
```
