---
layout: page.njk
title: "Osprey Flavors: Two Syntaxes, One Language"
excerpt: "Default and ML syntax lower to the same Osprey program representation."
description: "How Osprey's Default and ML source flavors share one compiler pipeline."
modified: 2026-07-25
tags: ["blog", "language-design", "flavors", "functional-programming", "ml-syntax"]
author: "Christian Findlay"
readingTime: 3
image: /assets/images/blog/osprey-flavors.png
---

Osprey has two source flavors:

- **Default (`.osp`)** uses braces, `fn`, named arguments and familiar calls.
- **ML (`.ospml`)** uses layout, curry-by-default functions and whitespace
  application.

Each flavor has its own parser and lowerer. Both produce
`osprey_ast::Program` before type checking, effect checking and code generation.

```osprey
fn add(x) = fn(y) => x + y
```

```osprey-ml
add x y = x + y
```

The compiler tests these explicit-curry forms for AST equivalence. A Default
multi-parameter function, such as `fn add(x, y)`, is intentionally different:
it accepts two arguments at once rather than returning a function.

ML layout and matching are available in runnable examples:

```osprey-ml
classify n =
    match n
        0 => "zero"
        1 => "one"
        _ => "many"
```

Flavor selection works per file through `.osp` or `.ospml`, a leading source
marker, or `--flavor` for a single-file build. Multi-file cross-flavor imports
remain under development.

Both flavors support lexical `effect`, `perform` and `handle … in` syntax.
Effect inputs and outputs are checked, but complete effect-row propagation and
missing-handler rejection are not implemented. Resuming handlers are
native-only.

See the [tested examples](https://github.com/Nimblesite/osprey/tree/main/examples/tested)
and [language-flavor specification](/spec/0023-languageflavors/).
