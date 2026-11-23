---
layout: page.njk
title: "Escaping Callback Hell: Flat Error Handling with `use`"
excerpt: "Osprey's new `use` statement turns a pyramid of nested callbacks into a flat list of steps. The compiler rewrites it to the same nested code, so nothing changes at run time."
description: "How Osprey's `use` statement flattens nested callbacks and Result chains, how it compares with async/await, do notation and Rust's question mark operator, and the compile errors it reports."
date: 2026-11-24
tags: ["blog", "error-handling", "callbacks", "functional-programming", "language-design"]
author: "Christian Findlay"
readingTime: 5
---

Some code is a chain of steps where each step can fail: find a customer, check their credit, bill them. When each step takes the rest of the work as a callback, every step nests one level deeper than the last one. The code drifts to the right and ends in a row of closing brackets. JavaScript developers call this callback hell, and the same shape appears in any language that chains `Result` values through closures.

[Osprey](/) is safe, elegant and fast. It has no exceptions and no panics: a step that can fail returns a `Result`, and the caller has to handle both outcomes. That makes failure visible, but it can also make these pyramids common. The new `use` statement flattens them.

## The pyramid

Here are three fallible steps chained with the prelude's `andThen`. `andThen` runs the callback only when the previous step succeeded, and passes an `Error` straight through.

```osprey
fn order(id) =
    andThen(findCustomer(id), fn(customer) =>
        andThen(checkCredit(customer), fn(_) =>
            andThen(bill(customer), fn(invoice) =>
                Success { value: "${invoice.name} owes ${invoice.total}" })))
```

This works, but the logic is hard to see. Each new step adds an indent and another closing bracket. Before this change, the deepest chains in Osprey's own code were twelve levels deep.

## The same function with `use`

```osprey
type Customer = { name: string, credit: int }
type Invoice = { name: string, total: int }

fn findCustomer(id) = match id {
    1 => Success { value: Customer { name: "Ada", credit: 500 } }
    2 => Success { value: Customer { name: "Bob", credit: 50 } }
    _ => Error { message: "no customer ${id}" }
}
fn checkCredit(c) = match c.credit > 100 {
    true => Success { value: c }
    false => Error { message: "${c.name} is over the limit" }
}
fn bill(c) = Success { value: Invoice { name: c.name, total: 120 } }

fn order(id) = {
    use customer <- andThen(findCustomer(id))
    use _ <- andThen(checkCredit(customer))
    use invoice <- andThen(bill(customer))
    Success { value: "${invoice.name} owes ${invoice.total}" }
}

fn show(r) = match r {
    Success { value } => value
    Error { message } => "refused: ${message}"
}
print("${show(order(1))}\n${show(order(2))}\n${show(order(3))}")
```

```text
Ada owes 120
refused: Bob is over the limit
refused: no customer 3
```

Each step is one line. The first failure stops the chain, and its message reaches the caller unchanged.

## What `use` means

`use p <- f(a, b)` is shorthand for `f(a, b, fn(p) => <the rest of the block>)`. The rest of the block becomes the last argument to the call, a callback that receives `p`.

The parser does the whole rewrite. The type checker and the code generator see only the ordinary call and lambda. That means:

- **No new semantics.** Anything true of the nested version is true of the flat one.
- **No cost at run time.** When dozens of pyramids across the Osprey codebase moved to `use`, the compiled LLVM IR stayed byte-identical.
- **It works with any function that takes a callback last,** not only `andThen`. `use` is not limited to `Result`.

## Variations

```osprey
fn withLog(name, body) = {
    print("enter ${name}")
    let r = body()
    print("leave ${name}")
    r
}

fn work() = {
    use <- withLog("outer")
    use <- withLog("inner")
    42
}
print(work())
```

```text
enter outer
enter inner
leave inner
leave outer
42
```

- `use <- f(...)` passes a callback that takes no argument. This suits resource scopes such as locks, transactions and timers.
- `use _ <- f(...)` is a guard step: it must succeed, and its value is discarded.
- `use a, b <- f(...)` passes a callback with two parameters.
- `use` works in any block: a function body, a match arm, a handler arm or a lambda.

## In the ML flavor

Osprey is one language written in four syntax flavors. In the ML flavor, which uses indentation instead of braces, the same function reads:

```osprey-ml
order id =
    use customer <- andThen (findCustomer id)
    use _ <- andThen (checkCredit customer)
    use invoice <- andThen (bill customer)
    Success(value = "${invoice.name} owes ${invoice.total}")
```

## Compared with async/await, do notation and `?`

Other languages fix this problem in other ways.

- **JavaScript's async/await** flattens promise chains, but only for promises, and it adds a second kind of function.
- **Rust's question mark operator** returns early on an `Err`. It is built into the language for `Result` and `Option`.
- **Haskell's do notation** flattens any monad, but it is tied to the `Monad` type class.
- **Gleam's `use`** is the closest relative. Osprey's version is the same idea: plain sugar over a callback.

Osprey chose the callback form because it needs no new kind of function and no new type machinery. It covers `Result`, resources and custom control flow with one rule. For concurrency, Osprey already uses fibers and effects, so `use` does not need to handle async code.

## Mistakes the compiler catches

Each of these is a compile error that names the fix:

- a `use` on the last line of a block, which leaves the callback with no body;
- a `use` outside any block, such as at the top level of a file;
- a right-hand side that is not a call, such as `use x <- 42`;
- a callback with the wrong number of parameters:

```text
`andThen` passes 1 value to its callback, but this callback takes 2: take 1, or bind 1 with `use`
```

`use` is now a keyword in both flavors, so it can no longer name a variable, parameter or function. A method may still be called `use`: `app.use(log)` is an ordinary method call.

## Try it

The full rules are in the [block expressions specification](/spec/0008-blockexpressions/). Osprey is actively evolving. Try it on your projects, validate it against your workload, and assess the risks before relying on it in production.
