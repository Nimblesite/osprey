---
layout: page
mlTwins: manual
title: "fn (Keyword)"
description: "Declare a function in Default flavor; parameter and result types are inferred."
---

`fn` declares a function in Default flavor. Omit parameter and return annotations when inference can derive them. ML declares functions without the `fn` keyword.

## Example

```osprey
fn greet(name) = "Hello, ${name}!"
print(greet("Alice"))
```

```osprey-ml
greet name = "Hello, ${name}!"
print (greet "Alice")
```

Both programs print `Hello, Alice!`.
