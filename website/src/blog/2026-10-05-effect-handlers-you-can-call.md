---
layout: page.njk
title: "Effect Handlers You Can Call"
excerpt: "Osprey handlers are now ordinary values, operations declare whether they can be abandoned, and integer overflow is an effect the compiler makes you handle."
description: "What changed in Osprey's algebraic effects: callable handler values, rest-of-block handle, declared control operations, return clauses, arithmetic effects and compile-time handlers."
tags: ["blog", "algebraic-effects", "handlers", "language-design", "koka", "ocaml"]
author: "Christian Findlay"
readingTime: 6
---

An algebraic effect is a typed request for work: load a record, write a log line, ask the user a question. A handler answers the request. The code making the request doesn't know or care which handler that is.

Osprey's handlers changed shape over the last month. A handler is now a value you call. `handle` covers the rest of its block. An operation's declaration decides whether its handler may abandon the work. Integer overflow is an effect. The website's test suite compiles every program below and checks that it prints the output shown, or fails with the error shown.

## A handler is a value

```osprey
effect Store { load: fn(string) -> string }
fn greeting(id) = "Hello, " + perform Store.load(id)

let database = handler Store { load id => "Ada (row ${id})" }
let fixture = handler Store { load _ => "Test User" }

print("${database(|| => greeting("42"))} / ${fixture(|| => greeting("42"))}")
```

```text
Hello, Ada (row 42) / Hello, Test User
```

`handler` builds a value; creating it runs nothing. `database(work)` installs the handler, calls `work`, and returns its result. Pass a function rather than its result, because the work must start after the handler is installed. `greeting` is the same code in production and in the test, with no service object passed through it.

The ML flavor spells the same program with layout and currying:

```osprey-ml
effect Store
    load : string => string
greeting id = "Hello, " + perform Store.load id

database = handler Store
    load id => "Ada (row ${id})"
fixture = handler Store
    load _ => "Test User"

print "${database (\() => greeting "42")} / ${fixture (\() => greeting "42")}"
```

```text
Hello, Ada (row 42) / Hello, Test User
```

## A missing handler is a compile error

```osprey
effect Store { load: fn(string) -> string }
fn greeting(id) = "Hello, " + perform Store.load(id)
print(greeting("42"))
```

```text
unhandled effect operations at program entry: Store.load; add a matching `handle`
```

The checker follows requests through helpers, lambdas passed to higher-order functions and fibers. Editor hover, signature help and generated API docs list the operations a function's callers must provide, including ones requested deep inside its helpers.

## `handle` covers the rest of the block

```osprey
effect Log { write: fn(string) -> Unit }
fn audit(step) = perform Log.write("audit: ${step}")
fn transfer() = {
    audit("debit")
    audit("credit")
    "done"
}

fn run() = {
    handle Log { write line => print(line) }
    transfer()
}
print(run())
```

```text
audit: debit
audit: credit
done
```

The handler applies to every statement after it in the block. The old `handle … in …` and ML `handle … do …` forms are removed and rejected.

## Handlers compose

A handler arm runs outside its own handler, so it can make the same request again and reach the next handler out:

```osprey
effect Log { write: fn(string) -> Unit }
fn transfer() = {
    perform Log.write("debit")
    perform Log.write("credit")
    "done"
}

let console = handler Log { write line => print(line) }
let tagged = handler Log { write line => perform Log.write("[audit] ${line}") }
print(console(|| => tagged(transfer)))
```

```text
[audit] debit
[audit] credit
done
```

## The declaration decides value or control

An ordinary operation returns its arm's value to `perform`, and the caller carries on. An operation declared `control` hands its arm the rest of the computation. The arm can `resume` it once, or return an answer and abandon it:

```osprey
effect Confirm { control ask: fn(string) -> bool }
fn submit(order) = match perform Confirm.ask(order) {
    true => "Submitted ${order}"
    false => "Declined ${order}"
}

let yes = handler Confirm { ask _ => resume(true) }
let cancel = handler Confirm { ask order => "Cancelled ${order}" }

print("${yes(|| => submit("A-1"))} / ${cancel(|| => submit("A-1"))}")
```

```text
Submitted A-1 / Cancelled A-1
```

Osprey used to infer the mode by searching an arm for `resume`, so a `resume` in a branch that never ran changed what the program did. The declaration now decides, as Koka's `fun`/`ctl` split does. The [handler comparison](https://github.com/Nimblesite/osprey/tree/main/examples/handlers) runs the same four probes in both Osprey flavors, Koka, OCaml, Eff and Effekt. All six print `42/142/0/0`.

## `return` transforms the answer

```osprey
effect Store { load: fn(string) -> string }
fn greeting(id) = "Hello, " + perform Store.load(id)

let report = handler Store {
    load _ => "Ada"
    return message => "Report: ${message}"
}
print(report(|| => greeting("42")))
```

```text
Report: Hello, Ada
```

The `return` clause rewrites normal completion. A control arm that abandons the work skips it.

## Arithmetic is an effect

Integer `+`, `-` and `*` can overflow. In Osprey that is neither a silent wraparound nor a crash: the operation requests `Arith.overflow`, and the compiler requires a handler for it.

```osprey
fn area(w, h) = w * h
let big = 4611686018427387904
print(area(big, 4))
```

```text
unhandled effect operations at program entry: Arith.overflow; add a matching `handle`
```

The handler chooses the policy for its region, including calls into helpers:

```osprey
fn area(w, h) = w * h

let wrapping = handler Arith { overflow _ _ _ wrapped => wrapped }
let capped = handler Arith { overflow _ _ _ _ => 9223372036854775807 }
let big = 4611686018427387904
print("${wrapping(|| => area(big, 4))} / ${capped(|| => area(big, 4))}")
```

```text
0 / 9223372036854775807
```

Constant expressions are folded at compile time, and division by a non-zero literal needs no handler. Code where wraparound is the intent, such as a hash, can call `wrapAdd`, `wrapSub` and `wrapMul` without a handler. The [arithmetic effects specification](/spec/0037-arithmeticeffects/) covers division, remainders and floating point.

## Interpret a handler at compile time

`handle static` answers requests while compiling:

```osprey
effect Config { region: fn() -> string }
fn endpoint() = "https://" + perform Config.region() + ".example.com"

let url = {
    handle static Config { region => "eu" }
    endpoint()
}
print(url)
```

```text
https://eu.example.com
```

The LLVM IR for this program contains no handler-stack calls. Ordinary computation, such as the string concatenation, still runs. The compiler rejects a static handler for a `control` operation, or one whose arm calls a runtime builtin such as `print`.

At runtime, the compiler gives each effect operation a dense number, so finding the active handler for a `perform` is one array read.

## What isn't finished

- Continuations are deep and single-shot, and only the native target resumes them. WebAssembly, iOS and Android support value handlers and `handle static`, and reject dynamic `control` operations before linking.
- Effect rows are not yet part of function types. The `!e` annotation is a closed-program prototype.
- Multi-shot continuations, named handler instances, masking and finalizers are planned.

[Plan 0016](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0016-algebraic-effects-and-handlers.md) tracks the evidence and the remaining work. To try handlers, start with the [effects guide](/docs/effects/) or the [playground](/playground/).
