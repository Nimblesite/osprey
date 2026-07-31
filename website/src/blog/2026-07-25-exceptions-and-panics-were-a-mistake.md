---
layout: page.njk
title: "Exceptions and Panics Were a Mistake: Better Error Handling"
excerpt: "Exceptions and panics turn ordinary control flow into hidden exits. Osprey makes expected failure explicit with Result types and algebraic effect handlers."
description: "Why exceptions and panics create hidden control flow, what software research found, and how Osprey handles errors with Result types and algebraic effects."
tags: ["blog", "error-handling", "exceptions", "panics", "algebraic-effects", "result-types", "ocaml", "koka", "eff", "language-design"]
author: "Christian Findlay"
modified: 2026-07-26
readingTime: 20
image: /assets/images/blog/exceptions-and-panics-were-a-mistake.png
imageAlt: "A cyan wireframe osprey above branching error-handling paths while one hidden exception path fractures below"
---

Exceptions and panics are not inevitable aspects of programming languages. They only make software more error-prone and make code harder to reason about. **Recoverable failure should never be a secret exit from a function**. This post is about how Osprey allows you to handle errors in a better way.

That is a strong claim, so here is what the evidence actually shows. Exceptions add routes through a program that its function signatures do not show. Researchers have found real leaks and crashes on those routes. In two large Java projects, exception patterns were linked to bugs found after release even after the researchers accounted for the usual code and change metrics. Other languages prove that we can keep automatic error handling without hiding failure. No study proves that every exception causes a bug. The studies below show a repeated, measurable problem across several languages and thousands of programs.

If a function's type says it returns `B`, it should return `B`. It should not also be able to jump over an unknown number of stack frames, skip the code after the call, arrive at a handler that may or may not exist, or terminate the process. Both unchecked exceptions and panics can skip ordinary returns, unwind frames and terminate when they are not intercepted. Neither puts routine recoverable failure in the function's ordinary return type.

Here, “exception” means an expected failure that escapes through `throw` or `panic` instead of appearing in the return value. A missing file, bad input or a missing database row belongs in the function's result. Memory corruption, a broken invariant or the operating system killing the process are different: the program may have no safe way to continue.

Osprey has no built-in `throw`, `try`, `catch` or `panic` keywords. Expected failures return `Result<T, E>`. When error handling belongs higher up the call stack, code can ask for a named effect and the surrounding `handle` block decides what to do. An effect can still stop work early, just like an exception. The difference is that the operation is named, checked and handled around the code that uses it. Osprey does not ban early exits. It makes them show themselves.

## Why exceptions are bad error handling

The usual type of a fallible function is dishonest:

```text
A -> B
```

Its real contract is closer to one of these:

```text
A -> Result<B, E>
A -> B !E
```

The first returns either success or failure. The second says the function may ask for the named effect `E`. Both tell the compiler and the reader that the call has another possible outcome.

Exceptions became attractive because error codes were awful. A caller could ignore `-1`, confuse it with a valid value, or forget to inspect a global error slot. Automatic propagation and separated recovery code felt like an escape from that mess. The foundational papers were trying to make programs more reliable, not less: John Goodenough's 1975 [*Exception Handling: Issues and a Proposed Notation*](https://doi.org/10.1145/361227.361230) and Barbara Liskov and Alan Snyder's 1979 [*Exception Handling in CLU*](https://doi.org/10.1109/TSE.1979.230191) both treated exceptional outcomes as part of an operation's defined behaviour.

But mainstream unchecked exceptions kept the convenient propagation and lost the honest contract. Any innocent-looking call may grow an invisible exit later. The caller still type-checks. The routes through the code silently change.

Exceptions have real benefits: straight-line code, automatic propagation and recovery code kept away from the place that detects the error. None of those benefits requires an unchecked `throw`. `Result` makes success and failure explicit. Pattern matching checks both cases. Algebraic effects keep the straight-line style and let higher-level code decide how to recover.

Once a language can name every outcome and reject code that forgets one, it does not need unchecked exceptions for routine errors.

## The evidence: hidden error paths cause real problems

This is not an argument about whether `try` and `catch` look nice. Researchers have measured the cost of the control flow they hide.

Westley Weimer and George Necula put the problem plainly in [*Exceptional Situations and Program Reliability*](https://web.eecs.umich.edu/~weimerw/p/weimer-toplas2008.pdf):

> “exceptions create hidden control-flow paths that are difficult for programmers to reason about”

They checked 27 Java programs containing more than five million lines of code. They found over **1,300 exception-handling defects** involving leaked resources or cleanup code that could be skipped. More than half of those defect paths went through a `catch` block and still leaked the resource.

The researchers checked the final reports and found no false warnings. They estimated that their filters might also have hidden 5–10% of real defects. The study covered declared checked exceptions and did not measure how often each path ran. It shows that exceptional paths make correct cleanup harder; it does not compare exceptions directly with every alternative.

Guilherme de Pádua and Weiyi Shang traced more than **77,000 exception paths** through over 10,000 handlers in 16 Java and C# projects for [*Revisiting Exception Handling Practices with Exception Flow Analysis*](https://guipadua.github.io/resources/scam2017-revisiting-eh_cr.pdf). One `try` block could let as many as 12 potentially recoverable exceptions escape. A single exception type could come from as many as 34 different methods.

> “Such results highlight the additional challenge of composing quality exception handling code.”

They then asked a practical question: could these exception paths help predict which files would have bugs after release? In [*Studying the Relationship between Exception Handling Practices and Post-release Defects*](https://guipadua.github.io/resources/eh-model-defects2018_cr.pdf), they added exception-path data to models that already used the usual code and change measurements. The extra data significantly improved the models for Hadoop and Hibernate, although it did not improve the model for the C# system Umbraco.

> “exception flow characteristics in Java projects have a significant relationship with post-release defects.”

That is a link found in two Java systems, not proof that exceptions caused the defects. It still matters: exception paths told the models something useful that the usual measurements did not.

Kirsten Bradley and Michael Godfrey examined **2,721 open-source C++ systems** in [*A Study on the Effects of Exception Usage in Open-Source C++ Systems*](https://plg2.cs.uwaterloo.ca/~migod/papers/2019/scam19.pdf). When their tool added the possible paths created by user-defined exceptions, the average number of connections in the call graph grew by **22.1%**. The median growth was **5.1%**.

Roughly **eight out of nine** functions that might throw did not create the exception themselves. They simply inherited that possibility from a function farther down the call stack. The study did not test whether every added edge became a bug, but it put a number on how much control flow an unchecked function signature can hide.

> “the use of C++ EH can add subtle complexity to the design of the system”

Roy Maxion and Robert Olszewski tested how well people handle exceptional cases in [*Eliminating Exception Handling Errors with Dependability Cases*](https://doi.org/10.1109/32.877848). Their experiment involved 119 participants. Giving them a structured way to work through exceptional cases improved the robustness of their error handling by **34%** (`p < .01`).

The researchers identified:

> “cognitive factors that impede the mental generation (or recollection) of exception cases”

This experiment compared unaided work with a structured reasoning method. It did not compare exceptions with `Result` or algebraic effects. Its finding is simpler: programmers miss exceptional cases, and they do better when those cases are laid out systematically.

Bruno Cabral and Paulo Marques examined **32 Java and .NET applications**, containing 3.41 million lines of code and 18,589 `try` blocks and handlers, in [*Exception Handling: A Field Study in Java and .NET*](https://eden.dei.uc.pt/~bcabral/ExceptionHandling_A_Field_Study_camready.pdf). Handlers commonly logged, notified, rethrew, returned or terminated instead of performing specialised recovery. Moving error handling away from the code that finds the problem does not mean the handler will recover from it.

> “exceptions are not being correctly used as an error recovery mechanism”

Panics do not solve ordinary failure either. A panic might unwind and be caught, or it might abort the program. Either way, an expected outcome has become a sudden runtime exit.

Boqin Qin and colleagues manually examined a selected set of **110 unexpected panic issues** in Rust for their 2024 IEEE Transactions on Software Engineering paper, [*Understanding and Detecting Real-World Safety Issues in Rust*](https://doi.org/10.1109/TSE.2024.3380393). **107 of the 110** occurred in safe code.

In that set, **39** panics came from failing to handle a `Result` or `Option`: 27 used `unwrap()` and 12 used `expect()`. The other issues included arithmetic failures, failed assertions and out-of-bounds access. Because the researchers selected panic issues to study, these numbers do not tell us how common each cause is across all Rust code.

> “they can abruptly halt programs, resulting in reduced reliability”

`Result` did its job in those 39 cases: it exposed the possibility of failure. The escape hatch hid that possibility again. If a language provides an honest error value and then makes it easy to pretend the error cannot happen, it rebuilds the same hidden control-flow problem one `unwrap` at a time.

Even the C++ standards work recognises the visibility problem. The proposal for [`std::expected<T, E>`](https://www.open-std.org/jtc1/sc22/wg21/docs/papers/2022/p0323r12.html) contrasts invisible exceptions with a return type that shows the failure case. Code must deal with that case before it can retrieve the value.

Herb Sutter's exception-friendly proposal [P0709R4](https://www.open-std.org/jtc1/sc22/wg21/docs/papers/2019/p0709r4.pdf) starts with the long-running problems that have split C++ projects into exception and no-exception dialects. You do not have to reject every exception to accept that their current design has serious costs.

These studies approached the problem in different ways, but they found the same basic issue. Exceptions add paths that function signatures do not show. Those paths make cleanup easier to miss, make call graphs larger and sometimes end in crashes. That is what “harder to reason about” means here: there are more paths to find and more chances to overlook one.

## Checked exceptions expose the problem but do not solve it cleanly

Java is the obvious counterexample. The [Java Language Specification](https://docs.oracle.com/javase/specs/jls/se25/html/jls-11.html) puts checked exceptions in a method's contract. A method must catch them or declare them. That is better than an invisible exit, and it proves the basic point: possible failure belongs in the contract.

Java's design is still awkward. Every method between the error and its handler may need another declaration. Broad exception types lose useful detail. `RuntimeException` remains outside the checks.

Making failures visible still helps, but the evidence should not be stretched. Maria Kechagia and colleagues examined 3,539 Android applications and 901,274 crash traces in [*The Exception Handling Riddle*](https://doi.org/10.1016/j.jss.2018.04.034). They found undocumented combinations of APIs and exceptions in real crashes and concluded:

> “exceptions that demonstrably lead to crashes should become checked”

Their separate experiment involved 25 participants. The differences between its groups were small enough that they could have been due to chance (`p = .14`). The crash data supports exposing undocumented failure. The experiment does not prove that Java-style checked exceptions are always better.

Donna Malayeri and Jonathan Aldrich checked how accurately Java methods declare exceptions in [*Practical Exception Specifications*](https://doi.org/10.1007/11818502_11). Across six open-source programs, between **16% and 81%** of the exception types in `throws` declarations were too broad or could not actually be thrown. The average was **46%**.

Their package-and-module approach let the compiler work out more of the contract. It reduced the declarations programmers had to write by 50–93%. The paper starts from a familiar problem: exceptions “introduce implicit control flow, complicating the task of understanding, maintaining, and debugging programs.”

The answer is not to abandon checked error contracts. It is to let the compiler work them out and combine them at useful boundaries instead of making programmers repeat the same declarations through every method.

## Result types vs exceptions: failure as ordinary data

Osprey's **Result pattern**—also called **errors as values**—is deliberately boring:

```osprey
fn describePort(text) = match parseInt(text) {
    Success { value }   => "port ${value}"
    Error   { message } => "invalid port: ${message}"
}

print(describePort("8080"))
```

`parseInt` returns either `Success` or `Error`. When Osprey knows that a value is a `Result`, a match has to cover both. Code also cannot simply grab the success value: it must match the result, use an explicit `?:` fallback, or use one of the places where Osprey currently unwraps automatically, listed in the [error-handling specification](/spec/0013-errorhandling/).

Failure is now ordinary data. Code can return it, store it, test it or transform it. The caller decides what to do because the failure is right there in the return value. If another outcome is added later, existing matches fail to compile until they handle it.

Use `Result<T, E>` for failures that callers expect to deal with: invalid input, a missing record, checked arithmetic, file access or an HTTP error. The runnable [validation pipeline](https://github.com/Nimblesite/osprey/blob/main/tests/regressions/basics/errors/validation_pipeline.test.osp) returns `Result<int, string>`, keeps the original error messages and handles both outcomes.

## Algebraic effects vs exceptions: direct code without hidden exits

`Result` is useful when the caller needs the failure as a value. It can become repetitive when the decision belongs several calls higher. Logging, retries, cancellation and recovery often work better when the surrounding application decides what to do. Osprey uses algebraic effects for that.

Gordon Plotkin and Matija Pretnar showed that exception handlers are one use of a broader effects-and-handlers model in [*Handling Algebraic Effects*](https://lmcs.episciences.org/705):

> “We further generalise exception handlers to arbitrary algebraic effects.”

Andrej Bauer and Matija Pretnar put the relationship plainly in [*Programming with Algebraic Effects and Handlers*](https://doi.org/10.1016/j.jlamp.2014.02.001):

> “An exception is an effect with a single operation raise with an empty result type.”

The practical point is simple: `throw` does not have to be a special hidden route out of a function. Failure can be a named request. The surrounding handler can stop, recover, return a `Result` or supply different behaviour in a test.

Daan Leijen's [Koka effect-system paper](https://www.microsoft.com/en-us/research/wp-content/uploads/2016/02/koka-effects-2013.pdf) shows that a compiler can track this in function types:

> “if an expression can be typed without an exn effect, then it will never throw an unhandled exception.”

That result does not measure bug rates. It proves that a language can record the possibility of an exception and guarantee when none can escape. Unchecked exceptions are a language choice, not something programming requires.

Osprey splits the feature into three visible parts:

- An `effect` names the operations that code may request and the types they use.
- `perform` makes one of those requests.
- `handle … in` says what to do when code inside that block makes the request.

### Osprey vs OCaml 5, Koka, Eff and Unison

If you search for an **algebraic effects programming language**, these are the main points of comparison:

- The **Eff programming language** is built around algebraic effects and handlers. Its [effect system](https://lmcs.episciences.org/1153) tracks which effects code may use.
- The **Koka programming language** works out the effects used by each function and records them in its type.
- **OCaml 5 effect handlers** provide the runtime control feature, but the [OCaml manual](https://ocaml.org/manual/effects.html) says the compiler does not guarantee that every effect has a handler.
- **Unison abilities** are Unison's name for effects, and [required abilities](https://www.unison-lang.org/docs/language-reference/abilities-and-ability-handlers/) appear directly in function types.

Osprey combines `Result<T, E>` for ordinary failures with named effects for decisions that belong outside the function. Both work in its brace and ML syntax. Osprey is aiming for the compile-time checks used by Koka, Eff and Unison, but it does not yet follow every effect through every function call. It cannot yet promise that every missing handler is caught during compilation.

### Algebraic effect example: recover by substituting a value

Here is a typed recovery effect. The parser reports why it failed; the outer policy decides that this application should use port 8080:

```osprey
effect InvalidPort {
    recover: fn(string) -> int
}

fn readPort(text) !InvalidPort = match parseInt(text) {
    Success { value }   => value
    Error   { message } => perform InvalidPort.recover(message)
}

let port = handle InvalidPort
    recover message => 8080
in readPort("not-a-port")
```

The handler returns `8080`, which becomes the result of `perform`, and `readPort` carries on. Another handler could read configuration, prompt the user, record a metric or supply a test value. `readPort` itself does not change.

This keeps the convenience people want from exceptions without hiding what can happen. `!InvalidPort` says that `readPort` may make this request, and the surrounding handler shows how the application will answer it.

The runnable [recoverable_errors.osp](https://github.com/Nimblesite/osprey/blob/main/tests/regressions/effects/recoverable_errors.test.osp) and [recoverable_errors.ospml](https://github.com/Nimblesite/osprey/blob/main/tests/regressions/effects/recoverable_errors.test.ospml) use the same pattern in a batch workflow. One handler skips an impossible order; another reduces it to the available stock. The paired [retry_until_valid.osp](https://github.com/Nimblesite/osprey/blob/main/tests/regressions/effects/retry_until_valid.test.osp) and [ML version](https://github.com/Nimblesite/osprey/blob/main/tests/regressions/effects/retry_until_valid.test.ospml) retry validation and continue once they receive a valid value.

### Algebraic effect exception example: resume or abort

An effect handler can also stop work early, just like an exception. In a handler region that contains `resume`, `resume(value)` returns to the point that called `perform`, using `value` as its result. If the selected branch returns without calling `resume`, the paused function does not continue. The branch's value becomes the result of the whole `handle … in` block. In a handler with no `resume` anywhere, returning from an arm has different behavior: it supplies the current operation result and the caller continues.

```osprey
effect PortError {
    parse: fn(string) -> int
}

fn boot(text) -> int !PortError = {
    let port = perform PortError.parse(text)
    print("starting server on ${port}")
    0
}

let exitCode = handle PortError
    parse text => match parseInt(text) {
        Success { value } => resume(value)
        Error { message } => {
            print("invalid port: ${message}")
            1
        }
    }
in boot("not-a-port")
```

On success, `resume(value)` returns to `boot`, which prints the startup message and returns `0`. On failure, the handler prints the error and returns `1`; the rest of `boot` never runs. This has the early-exit behaviour of an exception, but the operation has a name and type, and its handler is visible around the code.

The repository's paired Default/ML [resume regression suites](https://github.com/Nimblesite/osprey/tree/main/tests/effects/resume) exercise both branches and assert internal continuation state. The paired [direct recovery suites](https://github.com/Nimblesite/osprey/tree/main/tests/effects/errors) test fallback substitution, collect-all validation, nested policies, state and recursion. The runnable [algebraic effects examples](https://github.com/Nimblesite/osprey/tree/main/tests/regressions/effects) cover handler scoping, state, fibers and code that runs after `resume` returns.

### Combine algebraic effects with `Result`

Effects and results work together. One safe pattern is for the handler to provide a dependency and for the caller to turn that value into a `Result`:

```osprey
effect Accounts {
    find: fn(int) -> string
}

fn greeting(id) -> Result<string, string> !Accounts =
    match perform Accounts.find(id) {
        ""   => Error { message: "user ${id} was not found" }
        name => Success { value: "Hello, ${name}" }
    }

let lookup = handle Accounts
    find id => ""
in greeting(42)

match lookup {
    Success { value }   => print(value)
    Error   { message } => print(message)
}
```

A production `Accounts` handler could query a database. A test handler could return a fixed value. The effect lets surrounding code provide the lookup, while `Result` tells the immediate caller that `greeting` may fail. No database exception has to travel invisibly through the layers in between.

The tested suite includes paired Default/ML versions of [result_and_effects](https://github.com/Nimblesite/osprey/blob/main/tests/regressions/effects/result_and_effects.test.osp), [typed_error_channels](https://github.com/Nimblesite/osprey/blob/main/tests/regressions/effects/typed_error_channels.test.osp) and [collect_all_errors](https://github.com/Nimblesite/osprey/blob/main/tests/regressions/effects/collect_all_errors.test.osp). They show local Results feeding a handler policy, separate named error channels, and accumulation of multiple validation failures instead of stopping at the first one. The ML twins sit beside those files with the `.ospml` extension.

**Update:** a direct handler operation declared to return a whole `Result<T, E>` used to corrupt that value, and this post recommended keeping the `Result` outside the direct operation boundary. [Critical issue #183](https://github.com/Nimblesite/osprey/issues/183) is fixed — the paired reproducers now pass in both flavors under all three memory backends, so a direct operation may return a complete `Result`.

## Result type or algebraic effect?

The choice depends on who needs to make the decision:

| Mechanism | Use it when… | Handle it with… |
| --- | --- | --- |
| `Result<T, E>` | the caller needs to inspect, store, transform or return the failure | a complete `match` or an explicit fallback |
| Algebraic effect | code higher in the application should provide the behaviour, without passing objects through every function | a surrounding handler that can supply a value, resume the work or stop it |
| Runtime termination | continuing would be unsafe | a diagnostic and process exit, not a routine API |

With conventional exceptions, the language decides how `throw` searches for `catch` and unwinds the stack. An effect makes the request visible and lets the handler choose whether to recover, retry, supply a value or stop. Early exit is one possible use of the feature, not a hidden route built into every function.

`Result` gets its safety from checked pattern matching. Luc Maranget's [*Warnings for Pattern Matching*](https://www.cambridge.org/core/services/aop-cambridge-core/content/view/3165B75113781E2431E3856972940347/S0956796807006223a.pdf/warnings-for-pattern-matching.pdf) describes the compiler analysis behind that check. The practical payoff is straightforward: the compiler can tell you when a match forgot `Success` or `Error`.

The goal is not less error handling. It is fewer errors that were never handled.

## Does Osprey enforce error handling today?

Osprey is alpha software, so here is exactly what works today.

The compiler checks the values passed into and returned from effect operations. It runs handlers around a block of code and, in native builds, allows a handler to resume paused work once. For known `Result` values, it checks that matches cover every case and normally prevents code from grabbing the success value directly. The examples above compile and run today, and every linked file under `tests/regressions/effects` now has both a Default and an ML regression test.

Important limits include:

- An `Error` currently always carries a string message. Although signatures use `Result<T, E>`, `E` cannot yet be an arbitrary error value. That is why these examples use `Result<T, string>`.
- ~~The compiler does not yet prove that every effect has a handler.~~ **It does now.** A program that performs an effect nothing handles fails to build, with the effect and operation named: `unhandled effect operations at program entry: Log.write; add a matching handle`. The check reaches through helper calls, lambdas passed to higher-order functions, and fibers. It reasons over a closed program's operation summaries rather than an effect-row variable in a function type, so a future surface with independently quantified rows in public higher-order signatures would need more work — see the [algebraic-effects specification](/spec/0017-algebraiceffects/). Handlers that pause and resume work remain native-only; WebAssembly handlers must return immediately.
- The compiler automatically extracts the success value from a `Result` in six convenience cases. If the value is actually an `Error`, the current code can discard that error and produce a zero or default value. Osprey therefore does **not** yet force explicit handling of every `Result` on every path. This is an alpha safety gap to fix, not intended language behaviour.
- Resuming effect operations currently transport 16 arguments. The compiler accepts a 17th, but the runtime silently replaces it with zero. This is tracked as [critical issue #182](https://github.com/Nimblesite/osprey/issues/182); paired tests prove the safe boundary and skip the corrupting case.
- ~~Direct handlers corrupt whole `Result<T, E>` operation values.~~ **Fixed** — [critical issue #183](https://github.com/Nimblesite/osprey/issues/183) now passes in both flavors under all three memory backends. Whole Results passed through explicit `resume` keep their separate tests.
- An unannotated four-argument curried ML helper can silently skip effects performed through its body. The verified workaround is a flat parenthesised parameter list while [critical issue #184](https://github.com/Nimblesite/osprey/issues/184) is open.
- Under ARC memory management, a resuming handler whose completed answer is a dynamic string currently leaks one managed object. Paired tests keep the failing path visible while [critical issue #185](https://github.com/Nimblesite/osprey/issues/185) is open.

This post states the standard Osprey is aiming for. To meet it, the compiler must reject missing handlers, preserve every accepted operation value and never silently discard an `Error`. The language features already run; the checks and two value-transport paths above are not yet complete.

## Stop hiding the second return channel

Exceptions were a reasonable improvement over magic error codes and sentinel values. Panics still make sense when an invariant has failed and continuing would be unsafe. The mistake is using either mechanism for expected failures that callers can recover from.

If an operation can recoverably fail, return `Result<T, E>` or declare a typed effect. Match both `Success` and `Error`. If the decision belongs higher in the application, install a handler and state whether it resumes the work or stops it. Reserve process termination for failures from which the process cannot safely recover.

Automatic propagation is useful; hidden propagation is not. Handling policy higher in the application is useful; hiding a second route out of every function is not. Results and typed effects keep the useful parts without concealing the failure.

Exceptions and panics were a mistake. The better answer is simple: make failure visible in the program, and let the compiler check every path.

## References

- John B. Goodenough, [*Exception Handling: Issues and a Proposed Notation*](https://doi.org/10.1145/361227.361230), Communications of the ACM, 1975.
- Barbara H. Liskov and Alan Snyder, [*Exception Handling in CLU*](https://doi.org/10.1109/TSE.1979.230191), IEEE Transactions on Software Engineering, 1979.
- Westley Weimer and George C. Necula, [*Exceptional Situations and Program Reliability*](https://web.eecs.umich.edu/~weimerw/p/weimer-toplas2008.pdf), ACM TOPLAS, 2008.
- Roy A. Maxion and Robert T. Olszewski, [*Eliminating Exception Handling Errors with Dependability Cases: A Comparative, Empirical Study*](https://doi.org/10.1109/32.877848), IEEE Transactions on Software Engineering, 2000.
- Donna Malayeri and Jonathan Aldrich, [*Practical Exception Specifications*](https://doi.org/10.1007/11818502_11), Advanced Topics in Exception Handling Techniques, LNCS 4119, 2006.
- Bruno Cabral and Paulo Marques, [*Exception Handling: A Field Study in Java and .NET*](https://eden.dei.uc.pt/~bcabral/ExceptionHandling_A_Field_Study_camready.pdf), ECOOP, 2007.
- Guilherme B. de Pádua and Weiyi Shang, [*Revisiting Exception Handling Practices with Exception Flow Analysis*](https://guipadua.github.io/resources/scam2017-revisiting-eh_cr.pdf), SCAM, 2017 ([DOI](https://doi.org/10.1109/SCAM.2017.16)).
- Guilherme B. de Pádua and Weiyi Shang, [*Studying the Relationship between Exception Handling Practices and Post-release Defects*](https://guipadua.github.io/resources/eh-model-defects2018_cr.pdf), MSR, 2018 ([DOI](https://doi.org/10.1145/3196398.3196435)).
- Maria Kechagia et al., [*The Exception Handling Riddle: An Empirical Study on the Android API*](https://doi.org/10.1016/j.jss.2018.04.034), Journal of Systems and Software, 2018.
- Kirsten Bradley and Michael W. Godfrey, [*A Study on the Effects of Exception Usage in Open-Source C++ Systems*](https://plg2.cs.uwaterloo.ca/~migod/papers/2019/scam19.pdf), SCAM, 2019.
- Boqin Qin et al., [*Understanding and Detecting Real-World Safety Issues in Rust*](https://doi.org/10.1109/TSE.2024.3380393), IEEE Transactions on Software Engineering, 2024.
- Gordon D. Plotkin and Matija Pretnar, [*Handling Algebraic Effects*](https://lmcs.episciences.org/705), Logical Methods in Computer Science, 2013.
- Andrej Bauer and Matija Pretnar, [*Programming with Algebraic Effects and Handlers*](https://doi.org/10.1016/j.jlamp.2014.02.001), Journal of Logical and Algebraic Methods in Programming, 2015.
- Andrej Bauer and Matija Pretnar, [*An Effect System for Algebraic Effects and Handlers*](https://lmcs.episciences.org/1153), Logical Methods in Computer Science, 2014.
- Daan Leijen, [*Koka: Programming with Row-Polymorphic Effect Types*](https://www.microsoft.com/en-us/research/wp-content/uploads/2016/02/koka-effects-2013.pdf), Microsoft Research, 2013.
- Luc Maranget, [*Warnings for Pattern Matching*](https://www.cambridge.org/core/services/aop-cambridge-core/content/view/3165B75113781E2431E3856972940347/S0956796807006223a.pdf/warnings-for-pattern-matching.pdf), Journal of Functional Programming, 2007.
- Oracle, [*The Java Language Specification, Chapter 11: Exceptions*](https://docs.oracle.com/javase/specs/jls/se25/html/jls-11.html).
- C++ Working Group, [*P0323R12: `std::expected`*](https://www.open-std.org/jtc1/sc22/wg21/docs/papers/2022/p0323r12.html), 2022.
- Herb Sutter, [*P0709R4: Zero-overhead deterministic exceptions*](https://www.open-std.org/jtc1/sc22/wg21/docs/papers/2019/p0709r4.pdf), 2019.
