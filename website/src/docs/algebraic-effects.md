---
mlTwins: manual
layout: page
title: Algebraic effects
description: Declare typed operations, choose reusable handlers, and keep application code unchanged across production, tests, web and mobile hosts.
permalink: /docs/effects/
tags: [algebraic-effects, handlers, applications]
---

An algebraic effect is a typed request for work: read configuration, save an account, write a log. A handler supplies the implementation while a function runs. Application code can make those requests without passing service objects through every intermediate function.

## The same function, different implementations

```osprey
effect Account { name: fn() -> string }
fn greeting() = "Hello, " + perform Account.name()

let ada = handler Account { name => "Ada" }
let grace = handler Account { name => "Grace" }

print("${ada(greeting)} / ${grace(greeting)}")
// Hello, Ada / Hello, Grace
```

`Account.name` describes the operation's arguments and result. `perform` requests it. `handler` creates a reusable callable value; creating it does not run or install anything. `ada(greeting)` installs `ada`, calls `greeting`, and returns its answer. The compiler rejects a call to `greeting` that reaches the application boundary without a matching handler.

The function is passed as a callback because it must run **after** the handler is installed. Passing `greeting()` would run it first, before `ada` could answer its request. For work with arguments, wrap the call in a zero-argument function: `ada(|| => greetCustomer(customer))`.

Pass an existing zero-argument function directly: `ada(greeting)`. The analyzer's `redundant-callback` warning identifies forwarding wrappers it can prove unnecessary, such as `ada(|| => greeting())`, and names the direct replacement. A wrapper that supplies arguments or installs another handler still does useful work.

The same example in ML flavor:

```osprey-ml
effect Account
    name : Unit => string

greeting () = "Hello, " + perform Account.name ()

ada = handler Account
    name => "Ada"
grace = handler Account
    name => "Grace"

print "${ada greeting} / ${grace greeting}"
```

## Handle the rest of a block

Use `handle` when a handler is local to one block:

```osprey
effect Account { name: fn() -> string }
fn greeting() = "Hello, " + perform Account.name()

let message = {
    handle Account { name => "Ada" }
    greeting()
}
print(message)
```

The handler covers everything after it in the containing block, including requests reached through helper functions. Nested handlers take precedence for the operations they implement. Use a smaller block or a callable handler for a smaller region. The old `handle … in …` and ML `handle … do …` forms are rejected.

Handler arms run outside their own installed handler. An arm can forward the same operation to an enclosing handler without calling itself.

## Record work in a test

```osprey
effect Log { write: fn(string) -> Unit }
fn acceptOrder() = perform Log.write("Order accepted")

let terminal = handler Log { write message => print(message) }
terminal(acceptOrder)

mut captured = ""
let recording = handler Log { write message => { captured = message } }
recording(acceptOrder)
print(captured)
```

Both calls run `acceptOrder`. Production prints its message; the test records it. A factory can return a handler that captures its own configuration or state. A handler's operation types are checked even when the work does not call every arm.

## Select static interpretation

A value handler can be selected for compilation with `static`:

```osprey
effect Read { value: fn() -> int }
fn relay(callback) -> int !e = callback()
fn work() !Read = perform Read.value()

let answer = {
    handle static Read { value => 41 }
    relay(work)
}
print(answer)
```

This prints `41`. `handle static` removes the handled effect's dispatch during compilation. It can still leave ordinary calls, allocations and computations on runtime values. Static arms must satisfy the compiler's staging restrictions; static interpretation does not make arbitrary I/O executable during compilation.

Here `!Read` declares an allowed effect and `!e` carries the callback's unknown requirements through `relay`. Most application code leaves these annotations inferred. The current compiler supports this closed-program example; general independently quantified effect rows in reusable function types are still being implemented.

## Choose value or control operations explicitly

An ordinary operation returns its arm's value to `perform`, and the calling function continues. A declared `control` operation suspends the remainder of the computation. Its handler can resume it once or return an answer without continuing it:

```osprey
effect Confirm { control ask: fn() -> bool }
fn submit() = match perform Confirm.ask() {
    true => "Submitted"
    false => "Declined"
}

let proceed = handler Confirm { ask => resume(true) }
let stop = handler Confirm { ask => "Cancelled" }

print("${proceed(submit)} / ${stop(submit)}")
// Submitted / Cancelled
```

The declaration chooses control mode. An arm returning without `resume` abandons the computation; the presence of unreachable `resume` code never changes the mode. Dynamic control handlers currently require the native target. WebAssembly and mobile apps use value handlers or static interpretation.

A handler can also transform normal completion with a `return` clause:

```osprey
effect Account { name: fn() -> string }
fn greeting() = "Hello, " + perform Account.name()

let report = handler Account {
    name => "Ada"
    return message => "Report: ${message}"
}
print(report(greeting))
```

## Application examples

- [Talon Bank](/docs/web-apps/#algebraic-effects-what-runs-where) uses handlers for server storage and audit work, and for its browser application boundary. Browser commands carry asynchronous work to JavaScript.
- [Issue Inbox](/docs/mobile-apps/#application-effects) uses handlers to turn application requests into commands for the Swift and Android hosts. Platform completion arrives as another event.
- [Runnable handler examples](https://github.com/Nimblesite/osprey/tree/main/examples/handlers) cover factories, control flow, return clauses and static selection, alongside executable Koka and OCaml comparisons.

Callable handlers, rest-of-block handling, value/control declarations and static selection are available in both flavors. Reusable continuations, owned escaping continuations, named handler instances, masking and finalizers remain unfinished. [Plan 0016](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0016-algebraic-effects-and-handlers.md) records implementation evidence; the [effects specification](/spec/0017-algebraiceffects/) defines the full intended contract.
