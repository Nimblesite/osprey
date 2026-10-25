---
mlTwins: manual
layout: page
title: Algebraic effects
description: Declare typed operations, choose reusable handlers, and keep application code unchanged across production, tests, web and mobile hosts.
permalink: /docs/effects/
tags: [algebraic-effects, handlers, applications]
---

An algebraic effect is a typed request for work: read configuration, save an account, write a log. A handler supplies the implementation while a function runs. Application code can make those requests without passing service objects through every intermediate function.

## Save a draft, even when storage fails

The save logic requests storage; a handler writes the draft or simulates a failure so the editor can offer a retry.

```osprey
{{ draftExample.default | safe }}
```

This prints `{{ draftExample.output }}`. `disk` really writes `Ship it.` to `draft.txt`. `offline` returns a predictable failure without accessing the file system. Both run exactly the same `saveDraft` function. The `Result` says whether this request succeeded; the effect says which storage capability the surrounding application must provide.

`Drafts.save` declares the operation's arguments and result. `perform` requests it. Creating a `handler` runs no work. `disk(saveDraft)` installs the disk implementation before calling `saveDraft`. Calling `saveDraft()` without a matching handler is a compile error, even through intermediate helpers.

Pass the function, not its result: `saveDraft()` would run before `disk` could answer. Wrap work that takes arguments, such as `disk(|| => saveCustomerDraft(customer))`. A real editor can install its storage handler once around an event loop, so its UI and validation helpers need no storage parameters.

The same example in ML flavor:

```osprey-ml
{{ draftExample.ml | safe }}
```

This native example writes synchronously. In a mobile app, a value handler can describe a save command for the native host; completion arrives as another event.

## Handle the rest of a block

Use `handle` when a handler is local to one block:

```osprey
effect Drafts { save: fn(string, string) -> Result<int, Error> }
fn saveDraft() = match perform Drafts.save("draft.txt", "Ship it.") {
    Success _ => "Saved"
    Error { message } => "Retry: ${message}"
}

let message = {
    handle Drafts { save _ _ => Error { message: "offline" } }
    saveDraft()
}
print(message)
```

The handler covers the rest of the block, including requests made inside helpers. Nested handlers take precedence for the operations they implement. The old `handle … in …` and ML `handle … do …` forms are rejected.

Handler arms run outside their own handler, so an arm can forward the same operation to an enclosing handler.

## Record work in a test

```osprey
effect Drafts { save: fn(string, string) -> Result<int, Error> }
fn saveDraft() = match perform Drafts.save("draft.txt", "Ship it.") {
    Success _ => "Saved"
    Error { message } => "Retry: ${message}"
}

mut captured = ""
let recording = handler Drafts {
    save _ text => {
        captured = text
        Success { value: byteLength(text) }
    }
}
let saved = recording(saveDraft)
print("${saved}: ${captured}")
```

The test records exactly what the editor asked storage to save and returns a successful byte count. No file is written. A function can return a handler that captures configuration or state. Every arm is type-checked, even arms the work never calls.

## Select static interpretation

A value handler can be selected for compilation with `static`:

```osprey
effect DraftConfig { autosaveSeconds: fn() -> int }
fn relay(callback) -> int !e = callback()
fn work() !DraftConfig = perform DraftConfig.autosaveSeconds()

let answer = {
    handle static DraftConfig { autosaveSeconds => 30 }
    relay(work)
}
print(answer)
```

This prints `30`. `handle static` removes the handled effect's dispatch during compilation; ordinary calls and computation remain. Static handlers accept value operations only, and their arms cannot call runtime builtins such as `print`.

`!DraftConfig` declares an allowed effect and `!e` passes the callback's requirements through `relay`. Most code leaves both inferred. Open rows like `!e` are a closed-program prototype; effect rows are not yet part of function types.

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
effect Drafts { save: fn(string, string) -> Result<int, Error> }
fn saveDraft() = match perform Drafts.save("draft.txt", "Ship it.") {
    Success _ => "Saved"
    Error { message } => "Retry: ${message}"
}

let report = handler Drafts {
    save _ text => Success { value: byteLength(text) }
    return message => "Report: ${message}"
}
print(report(saveDraft))
```

## Arithmetic needs a policy

Integer `+`, `-` and `*` request the built-in `Arith` effect on overflow, and `/` and `%` request it on a zero divisor, so `fn double(n) = n * 2` needs an `Arith` handler around its callers: `handler Arith { overflow _ _ _ wrapped => wrapped }` wraps. Constant expressions and division by non-zero literals need none. See [arithmetic effects](/spec/0037-arithmeticeffects/).

## Application examples

- [Talon Bank](/docs/web-apps/#algebraic-effects-what-runs-where) uses handlers for server storage and audit work, and for its browser application boundary. Browser commands carry asynchronous work to JavaScript.
- [Issue Inbox](/docs/mobile-apps/#application-effects) uses handlers to turn application requests into commands for the Swift and Android hosts. Platform completion arrives as another event.
- [Runnable handler examples](https://github.com/Nimblesite/osprey/tree/main/examples/handlers) cover factories, control flow, return clauses and static selection, alongside executable Koka and OCaml comparisons.

Everything above works in Default and ML; the [Python flavor](/docs/python-flavor/#effects-and-handlers) spells the same effects with `@effect` classes, `perform(...)`, handler classes and `with`. [Feature status](/status/#algebraic-effects) lists what remains unfinished; the [effects specification](/spec/0017-algebraiceffects/) defines the full contract.
