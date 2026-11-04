# Chapter 9 — Ask for an effect

An effect lets a function request an operation without choosing its implementation. A handler supplies that implementation while the function runs. We will use effects and handlers to give the Flight Log a learner's name and report a completed lesson.

Start with the smallest useful request: ask for the learner's name. Run `examples/chapter-09/greeting.osp` from the book:

```osprey
effect Learner { name: fn() -> string }

fn greeting() = "Ready to learn, " + perform Learner.name() + "?"

let mika = handler Learner { name => "Mika" }
let rowan = handler Learner { name => "Rowan" }

print("${mika(greeting)}\n${rowan(greeting)}")
```

The output is:

```text
Ready to learn, Mika?
Ready to learn, Rowan?
```

There is one `greeting` function. Each call uses a different answer to its request. Neither call changes the function's source.

All commands in this chapter assume you are in `docs/book`. For the first example, use `../../target/release/osprey examples/chapter-09/greeting.osp --check`, then repeat the command with `--run`. An installed compiler can be invoked as `osprey` instead.

## 1. Read a request and its answer

`effect Learner` declares a group of operations. Here there is just one: `name`. The type `fn() -> string` means it takes no arguments and returns a string. Declaring it does not request a name yet.

`perform Learner.name()` makes the request. Its result becomes the value of that expression, so `greeting` can join it with the two surrounding strings. The function knows what answer it needs: text. It does not decide where that text comes from.

The two `handler` expressions provide implementations. In `name => "Mika"`, the operation arm supplies the string `"Mika"`. An **arm** is one named case in a handler, much like one case in a `match`. `mika(greeting)` makes that implementation available while `greeting` runs and returns the finished greeting.

![The installed handler answers Learner.name with Mika; greeting continues and returns its completed string.](assets/diagrams/09-request-answer.png)

*Figure 9.1 — Follow the value back to the request. The handler's string fills the place occupied by `perform Learner.name()`.*

There are three distinct jobs:

| Construct | Job | In this example |
|---|---|---|
| `effect` | Describe the permitted request and answer | `Learner.name` returns text |
| `perform` | Ask for an operation now | `greeting` requests the name |
| `handler` | Supply implementations while work runs | `mika` answers with `"Mika"` |

The operation's declared type tells the compiler what every implementation must accept and return. It infers the type of `greeting` and checks its requests without extra annotations.

### Try it: change an answer

Change only `"Rowan"` to your own name. Predict both output lines before running. The first line should remain unchanged; the second should use your name. Now change the question mark in `greeting` to an exclamation mark. Both lines should change. One edit changes an implementation; the other changes the shared application behavior.

### Why “algebraic effects”?

**Algebraic effects** is the broader name for describing work through named operations and giving those operations meaning with handlers. Here, `name` is the operation and each handler supplies an answer. We will use “effects and handlers” throughout the examples.

An effect can request logging or storage, but it can also return a constant. That makes it useful for tests: a handler can supply a predictable answer.

## 2. Pass the work so the handler can run it

Pass `greeting`, not `greeting()`, to the handler.

A function is a value you can pass to another function. Calling it produces its result. These are different events:

| Expression | What is supplied to `mika` |
|---|---|
| `mika(greeting)` | The function, ready to be called |
| `mika(greeting())` | The result of calling `greeting` first |

`mika(greeting)` activates the handler before calling `greeting`. Writing `mika(greeting())` would call `greeting` too early and pass its string result where a function is required; the compiler rejects it.

The passed function is a **callback**: a function another piece of code calls. Here it runs during the handler call.

![Constructing mika leaves it inactive; applying it installs the handler, calls greeting, and returns after the scope ends.](assets/diagrams/09-handler-lifetime.png)

*Figure 9.2 — Passing a function gives the handler control over when the work starts. Constructing the handler alone installs nothing.*

Sometimes the work needs arguments. A callable handler accepts a function with zero parameters, so wrap the particular call you want to make. For example, `terminal(|| => finishLesson("Effects and handlers"))` passes a small function that already knows the lesson title. `|| =>` is the compact spelling of Chapter 8's `fn() =>`: an anonymous function with no parameters. Its body runs when the handler calls it.

If the work already has a zero-argument function, pass it directly: `mika(greeting)`.

Ordinary inputs still belong in parameters. A lesson title is an input; a reporting service is a useful effect when several helpers need it and the application chooses its implementation around their work.

## 3. Give the request a visible scope

A callable handler is convenient when you want to name or reuse an implementation. For work that belongs to one block, use `handle`. It applies to the remaining statements and final expression of its containing block.

Here is the complete `examples/chapter-09/scope.osp`:

```osprey
effect Learner { name: fn() -> string }

fn greeting() = "Ready to learn, " + perform Learner.name() + "?"

let report = {
    handle Learner { name => "Mika" }
    let before = greeting()
    let inside = {
        handle Learner { name => "Rowan" }
        greeting()
    }
    let after = greeting()
    "${before}\n${inside}\n${after}"
}
print(report)
```

It prints the greetings for **Mika, Rowan, then Mika**. The inner handler takes precedence while its block runs. When that block finishes, the outer implementation is available again. The first handler has not been overwritten by the second one.

Notice where `greeting` was defined: outside both blocks. The operation uses the innermost active matching handler when the function runs. The function's definition does not permanently attach it to one learner. Requests reached through further helper calls follow the same rule.

The final `report` is an ordinary string. Printing it later needs no `Learner` handler: the request has already been answered.

### Let the compiler find a missing answer

Check `examples/chapter-09/failscompilation/missing-handler.osp`. It declares `Learner` and `greeting`, then calls `print(greeting())` with no handler. The recorded diagnostic is:

```text
examples/chapter-09/failscompilation/missing-handler.osp: unhandled effect operations at program entry: Learner.name; add a matching `handle`
```

The message names the missing operation. Define `mika` and call `mika(greeting)`, or put the call after a matching `handle` inside a block.

There is a second rejection example, `wrong-answer.osp`. Its handler answers `name => 41`. The compiler reports `type mismatch: cannot unify string with int`. The operation promised text; a handler cannot change that contract. Repair the answer according to what the application needs.

## 4. Choose handlers for the Flight Log

Now use two effects together. `Learner` supplies configuration. `Progress` reports what happened. Its operation takes a string and returns `Unit`, meaning that the caller receives no useful payload from that operation.

This is the application and terminal run from `examples/chapter-09/flight-log.test.osp`:

```osprey
effect Learner { name: fn() -> string }
effect Progress { report: fn(string) -> Unit }

fn completion(title) = perform Learner.name() + " completed: " + title

fn finishLesson(title) = {
    let message = completion(title)
    perform Progress.report(message)
    message
}

fn reading(person) = handler Learner { name => person }

let mika = reading("Mika")
let terminal = handler Progress { report message => print(message) }

let live = mika(|| => terminal(|| => finishLesson("Effects and handlers")))
```

The terminal displays `Mika completed: Effects and handlers`, and `live` holds the same string. In `finishLesson`, the `Progress` operation returns `Unit`, then the final expression returns `message`. The operation's answer and the function's answer are separate values.

`reading` returns a handler that remembers `person`. Creating `mika` does not run a lesson; calling it with a callback does.

Read the last line from the outside in. First `mika` installs the learner implementation. Its callback applies `terminal`, installing the progress implementation. The innermost callback calls `finishLesson` with its title. `completion` requests the learner name; `finishLesson` requests the report. Both requests have an active implementation.

### Use a handler inside a test

Append this case from the same file. Chapter 8 introduced `test` and `expect(actual, expected)`:

```osprey
test("completion returns the expected message", || => {
    expect(live, "Mika completed: Effects and handlers")
    let checking = handler Progress {
        report message =>
            expect(message, "Mika completed: Effects and handlers")
    }
    let answer = mika(|| =>
        checking(|| => finishLesson("Effects and handlers")))
    expect(answer, "Mika completed: Effects and handlers")
})
```

The application functions are unchanged. A test handler checks the message wherever the application reports it. It does not write that report to the terminal. The assertion after the call checks the returned value as well.

The full source includes a second case using Rowan and a different lesson. Run `../../target/release/osprey test examples/chapter-09 --quiet`; both cases pass. The same file's recorded output includes the live terminal message, followed by the two passing cases and the test summary.

The assertion inside `checking` checks the message only if `report` is called. It cannot detect a missing call. The recorded output separately checks that the live Mika run prints its report; it does not prove that every test run reports. The assertions after the calls check the returned strings.

Choose handlers where the application starts the work. Helpers such as `completion` can request what they need without receiving a learner or reporting service as an extra parameter.

## 5. Understand what continues after a request

So far every operation is a **value operation**. When its arm returns normally, the value becomes the result of `perform`, and the requesting function continues. No explicit `resume` belongs in these ordinary implementations.

Some problems need a different choice: continue the remaining work, or finish the whole handled operation early. Osprey expresses that distinction in the operation declaration with `control`.

Run `examples/chapter-09/control.osp` on the native target:

```osprey
effect Confirm { control ask: fn() -> bool }

fn submit() = match perform Confirm.ask() {
    true => "Lesson submitted"
    false => "Lesson declined"
}

let yes = handler Confirm { ask => resume(true) }
let no = handler Confirm { ask => resume(false) }
let cancel = handler Confirm { ask => "Submission cancelled" }

print("${yes(submit)}\n${no(submit)}\n${cancel(submit)}")
```

The three lines are `Lesson submitted`, `Lesson declined`, and `Submission cancelled`.

At `perform Confirm.ask()`, `submit` pauses before choosing a `match` branch. `resume(true)` gives it `true` and continues; `resume(false)` gives it `false` and continues. The paused work is called a **continuation**.

`cancel` skips that remaining work and returns `"Submission cancelled"` from the handler call. The two types serve different purposes: `ask` supplies a boolean when resumed, while the whole handler call returns a string.

![Resuming false runs submit's remaining match and declines the lesson; answering without resume skips the match and cancels submission.](assets/diagrams/09-control-choice.png)

*Figure 9.3 — A negative answer and cancellation take different paths. One continues the caller; the other finishes the handler call directly.*

The declaration chooses value or control behavior. Merely adding or removing `resume` text from an arm does not redefine an operation's mode. Use an ordinary value operation when all you need is an implementation that returns an answer.

On the native target, each paused continuation can be resumed at most once. Run this control example natively; dynamic control operations are not supported on WebAssembly or mobile C ABI targets.

## 6. Select a handler during compilation

The ordinary examples above select handlers without a `static` marker. Osprey also supports selecting a value handler during compilation. `examples/chapter-09/static.osp` keeps the same `Learner` declaration and `greeting` function, then uses:

```osprey
let message = {
    handle static Learner { name => "Mika" }
    greeting()
}
print(message)
```

It prints `Ready to learn, Mika?` again. With `static`, the compiler selects the implementation so the program does not need to look up this handler at runtime. Ordinary calls and allocations may remain; this does not mean the whole program runs during compilation.

Static handlers have additional restrictions. Use ordinary handlers for this chapter's application and tests; the static example is just a constant answer.

## 7. Check your understanding

Make each change in a copy and predict the result first:

1. In `scope.osp`, change the inner learner to `"Ari"`. Which lines change?
2. Add `fn welcome() = greeting()` and pass `welcome` to `mika`. Must the helper also receive a handler parameter?
3. In the Flight Log, change the title to `"Pattern matching"`. Which wrapper still contributes useful work?
4. In `control.osp`, replace the cancellation arm with `resume(false)`. Which final line changes, and why?

**Answers:** Only the middle scope line changes. `welcome` needs no handler parameter because its request happens while `mika` is active. The zero-argument wrapper still supplies the lesson title. Replacing cancellation with `resume(false)` runs the remaining match and produces `Lesson declined`.

### Agent handoff

```text
Extend the Chapter 9 Flight Log in Default flavor.
Keep completion and finishLesson unchanged.
Add a learner handler for Ari and a test Progress handler
that checks the report for the lesson "Reading types".
Check the returned string as well as the report payload.
Keep handler choices around the work that needs them.
Pass existing zero-argument functions directly; use a lambda
when it supplies arguments or installs another handler.

Run:
osprey examples/chapter-09/flight-log.test.osp --check
osprey test examples/chapter-09 --quiet

Review and update flight-log.test.expectedoutput for the
new test's name and count, then run make check-examples.
Report the exact output and explain when each handler is active.
```

Check that explanation against the composition line. A correct final string alone does not explain whether the handler was active at the right time. From the book directory, `make check-examples` also compares every runnable example with its recorded output and checks the rejection fixtures.

## Landing check

- An effect declaration describes typed operations; `perform` requests one.
- A handler provides implementations during a particular execution scope.
- Passing a function lets the handler become active before the work starts.
- A nested handler answers matching requests while its scope is active, including through helpers.
- Value operations return to their caller; declared control operations can resume or abandon the remaining work.
- Handler choices belong at a visible boundary, where production and test implementations can be exchanged.

Chapter 10 adds unreliable outside work. The next question is how an effect operation can return `Result`, so a storage implementation can report failure without hiding it from the application.

### Authoritative sources

- [Effects and handlers](https://github.com/Nimblesite/osprey/blob/main/docs/specs/language/0017-AlgebraicEffects.md): callable handlers, rest-of-block scope, operation typing, value/control modes, and static interpretation. This specification also contains planned features; use [plan 0016](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0016-algebraic-effects-and-handlers.md) for delivery limits.
- [Runnable handler examples](https://github.com/Nimblesite/osprey/tree/main/src/osprey/examples/handlers): current execution examples and comparisons with other languages.
- Executable source, exact output, tests, and rejection fixtures in `examples/chapter-09/`. The compiler identity and verification are recorded in `evidence.json`; the publication release remains unpinned.
