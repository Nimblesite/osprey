---
layout: page
title: "Language Keywords"
description: "Keyword syntax for effects, declarations, modules and fibers."
---

Keyword spelling depends on the syntax flavor. Default uses `fn` and `let`; ML omits both for ordinary declarations; Python uses `def` and `True`/`False`. See [syntax flavors](/spec/0023-languageflavors/) for their shared language model.

## Effects and handlers

| Keyword | Purpose |
| --- | --- |
| `effect` | Declare typed operations a program can request. |
| `perform` | Request an operation from the active handler. |
| `handler` | Build a callable handler value. |
| `handle` | Install a handler for the rest of the enclosing block. |
| `control`, `resume` | Declare a control operation and resume the suspended computation from its arm. |
| `instance`, `mask` | Create a distinct effect capability or skip an enclosing handler activation. |
| `static` | Mark compile-time effects or handlers. |

Start with the [effects guide](/docs/effects/). The [effect specification](/spec/0017-algebraiceffects/) defines operation modifiers, handler clauses and target limits.

## Values, functions and types

| Keyword | Purpose |
| --- | --- |
| [let](let/) | Bind an immutable value in Default syntax. |
| `mut` | Declare a cell whose reassignment belongs to an effect handler; see [bindings](/spec/0003-syntax/#bindings). |
| [fn](fn/) | Declare a function in Default syntax. |
| [type](type/) | Declare an alias, tagged union or record type. |
| [match](match/) | Select a pattern and bind its values within that arm. |
| `if`, `else` | Select an expression branch in Default syntax. |
| [true](true/), [false](false/) | Boolean literals in Default and ML. |

## Modules and host calls

| Keyword | Purpose |
| --- | --- |
| [import](import/), `as` | Bring public names into scope, optionally under an alias. |
| `namespace`, `module` | Group declarations under logical names. |
| `export`, `opaque`, `signature` | Define public members, hidden type representations and module interfaces. |
| `state` | Declare a module that owns mutable state. |
| `extern` | Declare a function supplied by the host through the native ABI. |

See [modules and namespaces](/spec/0025-modulesandnamespaces/) and [foreign function interfaces](/spec/0019-foreignfunctioninterface/).

## Native fibers and channels

`spawn` starts a fiber, `await` waits for its result, and `yield` yields the current native thread. `send`, `recv` and `select` communicate through channels. See [fibers and concurrency](/spec/0011-lightweightfibersandconcurrency/) for blocking behavior and target support.

