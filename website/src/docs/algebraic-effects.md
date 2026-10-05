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

`Account.name` declares the operation's arguments and result. `perform` requests it. `handler` creates a reusable value; creating it runs nothing. `ada(greeting)` installs `ada`, calls `greeting`, and returns its answer. A call to `greeting` with no matching handler is a compile error.

Pass the function, not its result: `greeting()` would run before `ada` could answer. Wrap work that takes arguments: `ada(|| => greetCustomer(customer))`. The `redundant-callback` warning flags wrappers that only forward, such as `ada(|| => greeting())`.

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

The handler covers the rest of the block, including requests made inside helpers. Nested handlers take precedence for the operations they implement. The old `handle … in …` and ML `handle … do …` forms are rejected.

Handler arms run outside their own handler, so an arm can forward the same operation to an enclosing handler.

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

Production prints the message; the test records it. A function can return a handler that captures configuration or state. Every arm is type-checked, even arms the work never calls.

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

This prints `41`. `handle static` removes the handled effect's dispatch during compilation; ordinary calls and computation remain. Static handlers accept value operations only, and their arms cannot call runtime builtins such as `print`.

`!Read` declares an allowed effect and `!e` passes the callback's requirements through `relay`. Most code leaves both inferred. Open rows like `!e` are a closed-program prototype; effect rows are not yet part of function types.

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

The declaration chooses the mode, never the presence of `resume` in an arm. Dynamic control handlers require the native target; WebAssembly and mobile apps use value handlers or static interpretation.

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

## Arithmetic needs a policy

Integer `+`, `-` and `*` request the built-in `Arith` effect on overflow, and `/` and `%` request it on a zero divisor, so `fn double(n) = n * 2` needs an `Arith` handler around its callers: `handler Arith { overflow _ _ _ wrapped => wrapped }` wraps. Constant expressions and division by non-zero literals need none. See [arithmetic effects](/spec/0037-arithmeticeffects/).

## Application examples

- [Talon Bank](/docs/web-apps/#algebraic-effects-what-runs-where) uses handlers for server storage and audit work, and for its browser application boundary. Browser commands carry asynchronous work to JavaScript.
- [Issue Inbox](/docs/mobile-apps/#application-effects) uses handlers to turn application requests into commands for the Swift and Android hosts. Platform completion arrives as another event.
- [Runnable handler examples](https://github.com/Nimblesite/osprey/tree/main/examples/handlers) cover factories, control flow, return clauses and static selection, alongside executable Koka and OCaml comparisons.

Everything above works in both flavors. [Feature status](/status/#algebraic-effects) lists what remains unfinished; the [effects specification](/spec/0017-algebraiceffects/) defines the full contract.
