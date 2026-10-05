# Chapter 9 — Ask for an effect

The Flight Log needs to know who is learning. Later it will need to save a lesson and report progress. Each of those jobs raises two questions: what does the application need, and who will provide it when the application runs?

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

That separation is the starting point for **algebraic effects**. This chapter builds it into a small application, explains the function passed to each handler, and shows how the compiler checks that requests have answers. You can work through it without knowing anything about continuations or functional-programming theory.

All commands in this chapter assume you are in `Book`. For the first example, use `../target/release/osprey examples/chapter-09/greeting.osp --check`, then repeat the command with `--run`. An installed compiler can be invoked as `osprey` instead.

## 1. Read a request and its answer

Read the first line as an interface. `effect Learner` names a group of operations. This group has one operation, `name`. Its signature, `fn() -> string`, says that callers supply no arguments and receive a string. The declaration does not choose a name or read anything yet.

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

The written operation signature matters. It defines a contract before an implementation exists. Ordinary function parameters and return types can still be inferred; `greeting` needs no repeated `string` annotation. Leaving off an effects annotation also does not disable checking. The compiler follows the operations the program may request.

### Try it: change an answer

Change only `"Rowan"` to your own name. Predict both output lines before running. The first line should remain unchanged; the second should use your name. Now change the question mark in `greeting` to an exclamation mark. Both lines should change. One edit changes an implementation; the other changes the shared application behavior.

### Why “algebraic”?

The word refers to describing a computation through named operations and supplying an interpretation for them. Here the operation is `name`, and each handler supplies an interpretation of that request. You do not need equations to use this idea.

Effects are often used for work such as logging, configuration, or storage. A handler can also answer entirely from a constant, as these two do. An effect request does not inherently require a file, network connection, or mutation. That is precisely why it can be useful in a deterministic test.

## 2. Pass the work so the handler can run it

The most easily missed character in the example is the absence of parentheses after `greeting` in `mika(greeting)`.

A function is a value you can pass to another function. Calling it produces its result. These are different events:

| Expression | What is supplied to `mika` |
|---|---|
| `mika(greeting)` | The function, ready to be called |
| `mika(greeting())` | The result of calling `greeting` first |

The handler must be installed before the request happens. When given `greeting`, `mika` can establish its scope, call the function, then return its answer. With `greeting()`, argument evaluation tries to do the work before the handler call has established that scope. The resulting string is also the wrong kind of argument: the callable handler expects work it can call.

That passed function is a **callback**. In this example the term means only “a function that another piece of code calls.” It does not imply a background thread, an event loop, or delayed work on another day. This callback runs as part of the handler call.

![Constructing mika leaves it inactive; applying it installs the handler, calls greeting, and returns after the scope ends.](assets/diagrams/09-handler-lifetime.png)

*Figure 9.2 — Passing a function gives the handler control over when the work starts. Constructing the handler alone installs nothing.*

Sometimes the work needs arguments. A callable handler accepts a function with zero parameters, so wrap the particular call you want to make. For example, `terminal(|| => finishLesson("Algebraic effects"))` passes a small function that already knows the lesson title. `|| =>` is the compact spelling of Chapter 8's `fn() =>`: an anonymous function with no parameters. Its body runs when the handler calls it.

That wrapper contributes something: it supplies the argument. If an existing zero-argument function already describes all the work, pass it directly. `mika(|| => greeting())` adds an unnecessary wrapper here; the analyzer can warn with `redundant-callback` and suggest `greeting`.

### Where does a relay fit?

A helper such as `fn relay(callback) = callback()` receives a function and calls it. It is useful for explaining how a request travels through an intermediate call, but it adds no application behavior by itself. The first example can call `mika(greeting)` directly. You do not need to add a relay to use effects.

A real wrapper might install several handlers or choose a policy before calling the supplied work. At that boundary, a callback has a clear purpose. Helpers farther inside the application can request their declared operations without receiving another parameter solely to carry a service through the call chain.

An ordinary parameter is still a good choice for ordinary input. Keep a lesson title as an argument. For a one-line greeting alone, a `name` parameter would also be reasonable. An effect becomes more useful when several functions need a service whose implementation is selected around a whole operation, such as completing and saving a lesson.

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

The final `report` is an ordinary string. Printing it later does not require a `Learner` handler because the request has already been answered. Conversely, returning a function that will make a request later does not make its future execution handled. The caller must arrange a suitable scope when that future work actually runs.

Current Osprey writes `handle E { ... }` followed by the work in the same block. Older `handle ... in ...` and `handle ... do ...` examples use removed syntax. A handler with an empty remainder has no work to handle and is rejected.

### Let the compiler find a missing answer

Check `examples/chapter-09/failscompilation/missing-handler.osp`. It declares `Learner` and `greeting`, then calls `print(greeting())` with no handler. The recorded diagnostic is:

```text
examples/chapter-09/failscompilation/missing-handler.osp: unhandled effect operations at program entry: Learner.name; add a matching `handle`
```

The message names the missing operation. Adding an effects annotation to `greeting` would describe a requirement, not provide an implementation. Repair the caller by using `mika(greeting)` with a declared handler, or put the call after a matching `handle` inside a block.

There is a second rejection example, `wrong-answer.osp`. Its handler answers `name => 41`. The compiler reports `type mismatch: cannot unify string with int`. The operation promised text; a handler cannot change that contract. Repair the answer according to what the application needs.

## 4. Build the Flight Log boundary

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

let live = mika(|| => terminal(|| => finishLesson("Algebraic effects")))
```

The terminal displays `Mika completed: Algebraic effects`, and `live` holds the same string. In `finishLesson`, the `Progress` operation returns `Unit`, then the final expression returns `message`. The operation's answer and the function's answer are separate values.

`reading` is a **handler factory**: an ordinary function that returns a handler. The arm remembers the `person` supplied to the factory. Creating `mika` still runs no lesson. That happens only when the handler is applied to the callback.

Read the last line from the outside in. First `mika` installs the learner implementation. Its callback applies `terminal`, installing the progress implementation. The innermost callback calls `finishLesson` with its title. `completion` requests the learner name; `finishLesson` requests the report. Both requests have an active implementation.

Those wrappers are doing useful work. One establishes an additional scope; the other supplies a title. Intermediate helpers need neither a logger parameter nor a learner-service parameter. The title remains an explicit input to the application operation.

### Use a handler inside a test

Append this case from the same file. Chapter 8 introduced `test` and `expect(actual, expected)`:

```osprey
test("completion returns the expected message", || => {
    expect(live, "Mika completed: Algebraic effects")
    let checking = handler Progress {
        report message =>
            expect(message, "Mika completed: Algebraic effects")
    }
    let answer = mika(|| =>
        checking(|| => finishLesson("Algebraic effects")))
    expect(answer, "Mika completed: Algebraic effects")
})
```

The application functions are unchanged. A test handler checks the message wherever the application reports it. It does not write that report to the terminal. The assertion after the call checks the returned value as well.

The full source includes a second case using Rowan and a different lesson. Run `../target/release/osprey test examples/chapter-09 --quiet`; both cases pass. The same file's recorded output includes the live terminal message, followed by the two passing cases and the test summary.

Each check proves something specific. The handler assertion checks a supplied message; by itself, it would not catch code that never reports anything. The complete-output comparison also requires the terminal message on the live Mika path. Together they check that visible report and the returned values; they do not independently establish that every test invocation reported. Keep this distinction when you design your own tests.

This example uses an assertion directly in the handler. It does not need a mutable recording buffer. For larger workflows, use explicit operation results and narrow test implementations, as Chapter 10 does for storage.

### Choose the boundary deliberately

The composition line is a small application boundary: the place that chooses implementations and starts work. It might live in `main`, an HTTP request handler, or a platform event dispatcher. Business functions describe the requests they need. The surrounding application supplies them.

This does not make dependencies disappear. They are declared as operations, checked by the compiler, and implemented in handlers. A clear boundary gives readers one place to find the choices. Keep effect names specific enough to explain what capability the application is requesting.

Talon Bank uses the same separation for storage and audit reporting. Its native mobile clients share application code with the web client; handlers describe commands, and each host executes its platform work. A network completion comes back as another event. This does not imply that a mobile handler can retain and resume an arbitrary suspended Osprey computation.

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

At `perform Confirm.ask()`, the remaining work is the `match` that would inspect the boolean and return a string. That suspended remainder is called a **continuation**. `resume(true)` supplies the boolean and runs that remaining work; `resume(false)` runs it with the other answer.

The `cancel` arm supplies no boolean. It answers the entire handler call with a string, so the suspended `match` never runs. This explains why a control arm can return a string even though `ask` declares a boolean operation result: the boolean is what a resumed `perform` receives; the string is the answer to the surrounding handled computation.

![Resuming false runs submit's remaining match and declines the lesson; answering without resume skips the match and cancels submission.](assets/diagrams/09-control-choice.png)

*Figure 9.3 — A negative answer and cancellation take different paths. One continues the caller; the other finishes the handler call directly.*

The declaration chooses value or control behavior. Merely adding or removing `resume` text from an arm does not redefine an operation's mode. Use an ordinary value operation when all you need is an implementation that returns an answer.

Current native dynamic control is **single-shot**: a captured remainder can be resumed at most once. Resumption restores the handled scope, so later requests made by the resumed work can use it again; this is called **deep** handling. Reusable or escaping continuations remain unfinished. Dynamic control is rejected on WebAssembly and mobile C ABI targets; the practical storage and platform examples in this book use value operations.

## 6. Recognise static selection without depending on it

The ordinary examples above select handlers without a `static` marker. Osprey also supports selecting a value handler during compilation. `examples/chapter-09/static.osp` keeps the same `Learner` declaration and `greeting` function, then uses:

```osprey
let message = {
    handle static Learner { name => "Mika" }
    greeting()
}
print(message)
```

It prints `Ready to learn, Mika?` again. Static selection removes the handled operation's runtime effect dispatch. It can still leave ordinary function calls, allocations, and computations on runtime values. Do not read `static` as a promise that the whole application runs during compilation or that arbitrary file and network work can be evaluated there.

The compiler applies additional restrictions to static arms. Begin with ordinary handlers until there is a reason to choose static interpretation. Handler scope, operation types, and the reason for passing a callback remain the same ideas.

You may also encounter signatures such as `!Learner` or a lowercase `!e` after a return type. These describe permitted effects; the latter carries an unknown remainder in supported higher-order examples. This chapter leaves ordinary requirements inferred. General independently quantified effect rows in reusable function types are still delivery work, so the advanced notation is not needed for these application patterns.

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

- [Algebraic Effects](https://github.com/Nimblesite/osprey/blob/main/docs/specs/0017-AlgebraicEffects.md): callable handlers, rest-of-block scope, operation typing, value/control modes, and static interpretation. This specification also contains planned features; use [plan 0016](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0016-algebraic-effects-and-handlers.md) for delivery limits.
- [Runnable handler examples](https://github.com/Nimblesite/osprey/tree/main/examples/handlers): current execution examples and comparisons with other languages.
- Executable source, exact output, tests, and rejection fixtures in `examples/chapter-09/`. The compiler identity and verification are recorded in `evidence.json`; the publication release remains unpinned.
