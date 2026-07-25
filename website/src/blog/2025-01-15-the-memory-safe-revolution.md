---
layout: page.njk
title: "Memory Safety and Osprey's Runtime Boundary"
excerpt: "Osprey-managed values are memory-safe; C calls cross that boundary."
description: "The memory-safety boundary around Osprey values, fibers and the C FFI."
tags: ["blog", "memory-safety", "functional-programming", "concurrency"]
author: "Christian Findlay"
readingTime: 3
image: /assets/images/blog/the-memory-safe-revolution.png
---

Osprey uses immutable values, checked operations and runtime-managed memory to
avoid common invalid-memory access in Osprey code. Native builds currently
offer three memory backends:

- `default`: allocation without reclamation
- `gc`: tracing garbage collection
- `arc`: Perceus reference counting

The backend is selected at build time and does not change the program's source.
The strict static-memory subset in the specification remains a design target;
it is not a current CLI mode.

Persistent lists and maps return new values while reusing unchanged structure:

```osprey
let first = listAppend(List(), "Ada")
let second = listAppend(first, "Grace")
print(listLength(first))
print(listLength(second))
```

Native fibers communicate through channels rather than shared mutable Osprey
values. Fibers are not currently available on the WebAssembly target.

The memory-safety guarantee ends at the C FFI boundary. An `extern fn` can call
code that misuses pointers or violates an Osprey runtime contract, so bindings
require the same review as other C integrations.

See the [memory-management specification](/spec/0018-memorymanagement/),
[concurrency specification](/spec/0011-lightweightfibersandconcurrency/) and
[FFI specification](/spec/0019-foreignfunctioninterface/).
