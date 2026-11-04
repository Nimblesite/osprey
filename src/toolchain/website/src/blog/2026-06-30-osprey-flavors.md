---
layout: page.njk
title: "Osprey Flavors: Any Syntax, One Language"
excerpt: "Every Osprey flavor lowers its concrete syntax tree into one canonical AST."
description: "How every Osprey source flavor shares one AST and one compiler pipeline."
modified: 2026-10-17
tags: ["blog", "language-design", "flavors", "functional-programming", "ml-syntax", "python-flavor"]
author: "Christian Findlay"
readingTime: 3
image: /assets/images/blog/osprey-flavors.png
---

A flavor is a surface syntax, not a separate language. Each flavor parses its
own concrete syntax tree and lowers it into the canonical `osprey_ast::Program`;
type checking, effect checking and code generation only ever see that AST. A new
syntax is a new frontend, and every flavor gets the whole language. Osprey ships
these today:

- **Default (`.osp`)** uses braces, `fn`, named arguments and familiar calls.
- **ML (`.ospml`)** uses layout, curry-by-default functions and whitespace
  application.
- **Python flavor (`.py`)** looks like typed Python, with decorators and `with`
  for effects. It is Osprey compiled by `osprey`; CPython never runs it.

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

Flavor selection works per file through `.osp` or `.ospml`, a leading source marker such as `# osprey: flavor=python`, or `--flavor` for a single-file build. A project can mix flavors and import across them.

Default and ML support `effect`, `perform`, `handle` and `handler`, and the Python flavor spells the same effects with `@effect` classes, `perform(...)`, handler classes and `with`; see [Effect Handlers You Can Call](/blog/2026-10-05-effect-handlers-you-can-call/).

See the [tested examples](https://github.com/Nimblesite/osprey/tree/main/tests/regressions)
and [language-flavor specification](/spec/0023-languageflavors/).
