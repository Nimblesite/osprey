---
layout: page.njk
title: "Exceptions and Panics Were a Mistake: Better Error Handling"
excerpt: "Exceptions and panics turn ordinary control flow into hidden exits. Osprey makes expected failure explicit with Result types and algebraic effect handlers."
description: "Why exceptions and panics create hidden control flow, what software research found, and how Osprey handles errors with Result types and algebraic effects."
tags: ["blog", "error-handling", "exceptions", "panics", "algebraic-effects", "result-types", "ocaml", "koka", "eff", "language-design"]
author: "Christian Findlay"
modified: 2026-07-25
readingTime: 14
image: /assets/images/blog/exceptions-and-panics-were-a-mistake.png
imageAlt: "A cyan wireframe osprey above branching error-handling paths while one hidden exception path fractures below"
---

Here is the claim: **recoverable failure should never be a secret exit from a function**.

If a function's type says it returns `B`, it should return `B`. It should not also be able to jump over an unknown number of stack frames, skip the code after the call, arrive at a handler that may or may not exist, or terminate the process. Both unchecked exceptions and panics can skip ordinary returns, unwind frames and terminate when they are not intercepted. Neither puts routine recoverable failure in the function's ordinary return type.

That is what I mean by “exceptions” in the title: privileged, unchecked, non-local exits whose possibility is absent from the function type. I do not mean that a runtime can never stop after memory corruption, a failed invariant or an external kill. Those are boundaries of execution. I mean that file-not-found, invalid input, a missing database row and every other expected failure do not deserve a secret control-flow channel.

Osprey has no built-in `throw`, `try`, `catch` or `panic` keywords. It represents ordinary failure as `Result<T, E>`, or it declares a typed algebraic effect and gives that effect a lexical handler. Algebraic effects can express exception-style early exit, so this is not a claim that non-local control flow is impossible. It is a claim that non-local control flow should stop lying about itself.

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

The first says that failure is one of the returned values. The second says that evaluation may perform a named effect. Either form is designed to give the compiler and the reader the missing edge.

Exceptions became attractive because error codes were awful. A caller could ignore `-1`, confuse it with a valid value, or forget to inspect a global error slot. Automatic propagation and separated recovery code felt like an escape from that mess. The foundational papers were trying to make programs more reliable, not less: John Goodenough's 1975 [*Exception Handling: Issues and a Proposed Notation*](https://doi.org/10.1145/361227.361230) and Barbara Liskov and Alan Snyder's 1979 [*Exception Handling in CLU*](https://doi.org/10.1109/TSE.1979.230191) both treated exceptional outcomes as part of an operation's defined behaviour.

But mainstream unchecked exceptions kept the convenient propagation and lost the honest contract. Any innocent-looking call may grow an invisible exit later. The caller still type-checks. The control-flow graph silently changes.

Some programmers genuinely like exceptions. What they like is real: direct-style code, automatic propagation and a recovery policy separated from the operation that detects the problem. What they are mistaking is that syntax for a necessary semantic feature. It is not necessary. Sum types give us explicit outcomes; exhaustive pattern matching forces us to cover them; algebraic effects give us direct style and non-local policy without hard-coding one invisible unwinding mechanism.

Once a language can express every outcome and reject incomplete branching, the old unchecked-exception escape hatch is redundant. This is the no-brainer at the centre of the argument.

## What research says about exception-handling defects

This is not just aesthetic disgust at `try` and `catch`.

Westley Weimer and George Necula's ACM TOPLAS paper [*Exceptional Situations and Program Reliability*](https://web.eecs.umich.edu/~weimerw/p/weimer-toplas2008.pdf) used path-sensitive analysis on more than five million lines of Java and found over **1,300 exception-handling defects** involving unreleased resources or broken cleanup obligations. After post-processing heuristics, the final reports had no observed false positives; the authors estimated that those filters could hide 5–10% of real defects. Their fault model covered declared checked exceptions, so this is evidence about the cleanup complexity of visible exception flow, not specifically unchecked exceptions.

Kirsten Bradley and Michael Godfrey studied **2,721 open-source C++ systems** in [*A Study on the Effects of Exception Usage in Open-Source C++ Systems*](https://plg2.cs.uwaterloo.ca/~migod/papers/2019/scam19.pdf). For the user-defined exception flows their tool modelled, adding exception edges increased call-graph edges by an average of **22.1%**, though the median increase was **5.1%**. Roughly **eight out of nine** functions that might throw did not originate the exception; they merely inherited the hidden edge from below. That study is exploratory rather than proof that every extra edge becomes a bug, but it quantifies exactly what an unchecked function signature conceals.

Bruno Cabral and Paulo Marques examined **32 Java and .NET applications**, 3.41 million lines of code and 18,589 `try` blocks and handlers in [*Exception Handling: A Field Study in Java and .NET*](https://eden.dei.uc.pt/~bcabral/ExceptionHandling_A_Field_Study_camready.pdf). They found handlers commonly logging, notifying, rethrowing, returning or terminating rather than performing specialised recovery. The mechanism separates the handler from the failure site; the field evidence does not show that the separated code usually recovers.

Panics do not fix expected failure. A panic may unwind and be intercepted, or it may abort, but it moves a domain-level outcome onto an abrupt runtime path.

Boqin Qin and colleagues manually studied a selected corpus of **110 unexpected panic issues** in Rust for their 2024 IEEE Transactions on Software Engineering paper, [*Understanding and Detecting Real-World Safety Issues in Rust*](https://doi.org/10.1109/TSE.2024.3380393). **107 of 110** occurred in safe code. In that corpus, **39** came from missing `Result` or `Option` handling—27 through `unwrap()` and 12 through `expect()`—while the rest included arithmetic, assertion and bounds failures. This is a taxonomy of selected issues, not an estimate of how prevalent each cause is across all Rust code.

Rust's `Result` is not the problem there. The escape hatch that converts an explicit `Err` alternative into an abrupt panic path is the problem. If a language introduces a truthful error value and then makes it effortless to pretend the error case cannot happen, it has rebuilt the same hidden control-flow problem one `unwrap` at a time.

Even the C++ standards work acknowledges the visibility problem. The WG21 proposal for [`std::expected<T, E>`](https://www.open-std.org/jtc1/sc22/wg21/docs/papers/2022/p0323r12.html) contrasts invisible exceptions with a return type whose failure case is visible and must be confronted to retrieve the value. Herb Sutter's exception-friendly proposal [P0709R4](https://www.open-std.org/jtc1/sc22/wg21/docs/papers/2019/p0709r4.pdf) starts from the long-standing unresolved problems that have split C++ projects into exception and no-exception dialects. You do not have to be anti-exception to see the design wound.

## Why checked exceptions are not the whole answer

Java is the obvious objection. The [Java Language Specification](https://docs.oracle.com/javase/specs/jls/se25/html/jls-11.html) makes checked exceptions part of a method's contract and requires them to be caught or declared. That is better than an invisible exit. It also proves the premise: failure belongs in the static contract.

The problem is not that Java checks too much. It is that its checked-exception system is a crude, nominal effect system with a permanently open unchecked side door. Declarations ripple through intermediate methods, broad supertypes erase useful precision, and `RuntimeException` remains outside the requirement. Static visibility still helps: Maria Kechagia and colleagues' Android study [*The Exception Handling Riddle*](https://doi.org/10.1016/j.jss.2018.04.034) analysed 3,539 applications and 901,274 crashes, and its controlled trial found that making exceptions checked improved stability more effectively than documentation alone. That supports the narrower thesis here—put failure in the static contract—without proving that Java chose the best contract.

Donna Malayeri and Jonathan Aldrich measured declaration imprecision in [*Practical Exception Specifications*](https://doi.org/10.1007/11818502_11). Across six open-source Java programs, between **16% and 81%** of exception types in `throws` declarations were imprecise, with a **46% average**. In their simulated package/module setup, module-oriented inference reduced programmer-written declarations by 50–93%. The lesson is not “give up on static error contracts.” It is “infer and compose them at the right abstraction boundary.” That is effect-system territory.

## Result types vs exceptions: failure as ordinary data

Osprey's **Result pattern**—also called **errors as values**—is deliberately boring:

```osprey
fn describePort(text) = match parseInt(text) {
    Success { value }   => "port ${value}"
    Error   { message } => "invalid port: ${message}"
}

print(describePort("8080"))
```

`parseInt` returns a `Result`. `Success` and `Error` are Osprey's built-in result variants. For a known `Result`, Osprey's [exhaustiveness checker](/spec/0007-patternmatching/) rejects a match that forgets either case. Direct access to the success payload is also rejected unless code matches it, selects an explicit `?:` fallback, or enters one of the currently permitted auto-unwrapping contexts described in the [error-handling specification](/spec/0013-errorhandling/).

This changes failure from a control-flow ambush into a value that can be stored, returned, transformed, tested and inspected. The function's caller owns the policy because the caller has the whole value in hand. Adding a new variant to a domain-specific outcome union changes the data shape and makes an incomplete match fail at compile time instead of quietly changing a distant call graph.

Use `Result<T, E>` when failure is part of the value-level API: parsing, validation, lookup, checked arithmetic, file access, HTTP and similar operations where a caller naturally chooses among outcomes. The repository's runnable [validation pipeline](https://github.com/Nimblesite/osprey/blob/main/examples/tested/basics/errors/validation_pipeline.osp) constructs `Result<int, string>`, propagates specific messages and exhaustively matches every outcome.

## Algebraic effects vs exceptions: direct style without the lie

`Result` is not always the most readable way to express a policy that spans many layers. Logging, dependency injection, retry, cancellation and exception-style early exit all benefit from handling code outside the function that requests the operation. Osprey uses algebraic effects for that.

The theoretical foundation is Gordon Plotkin and Matija Pretnar's [*Handling Algebraic Effects*](https://lmcs.episciences.org/705), which generalises exception handlers to operations for effects such as state, nondeterminism and I/O. Daan Leijen's [Koka effect-system paper](https://www.microsoft.com/en-us/research/wp-content/uploads/2016/02/koka-effects-2013.pdf) shows the static destination: infer row-polymorphic effects in function types so an expression without the exception effect cannot produce an unhandled exception.

Osprey separates three things that conventional exceptions fuse together:

- An `effect` declares the available operations and their argument and result types.
- `perform` requests one of those operations.
- A lexical `handle … in` region supplies the policy.

### Osprey vs OCaml 5, Koka, Eff and Unison

People searching for an **algebraic effects programming language** will repeatedly meet four useful reference points. The **Eff programming language** is an ML-style language built around first-class algebraic effects and handlers; Bauer and Pretnar's [effect system for core Eff](https://lmcs.episciences.org/1153) statically tracks them. The **Koka programming language** infers row-polymorphic effect types, making effects central to its function contracts. **OCaml 5 effect handlers** provide native control primitives, but the [OCaml manual](https://ocaml.org/manual/effects.html) is explicit that, unlike Eff and Koka, OCaml does not statically ensure that every performed effect is handled. **Unison abilities** are its name for algebraic effects, and [Unison's language reference](https://www.unison-lang.org/docs/language-reference/abilities-and-ability-handlers/) puts ability requirements directly in function types.

Osprey pairs the two error-handling forms in one language: `Result<T, E>` for ordinary errors as values, and named effect operations for direct-style, handler-controlled policy. Its intended static destination is therefore closer to Koka, Eff and Unison than to OCaml's currently untracked handlers. Its surface is different: Osprey exposes C-style and ML-style syntax for the same semantics, uses lexical `handle … in` regions, and compiles the examples below as ordinary Osprey programs. The important alpha disclaimer is that complete effect-row propagation is still a target, not a guarantee of every current build.

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

The handler arm returns `8080`, so that value becomes the result of `perform` and `readPort` continues. Another handler could read configuration, prompt a user, record a metric or choose a test fixture. `readPort` does not change.

This is the convenience people reach for exceptions to obtain: direct-style code with policy elsewhere. But the operation has a name, its input and output are typed, the function advertises `!InvalidPort`, and the handler is visible around the computation it governs.

### Algebraic effect exception example: resume or abort

Algebraic handlers can also implement a real exception-style early exit. `resume(value)` continues the suspended function and makes `value` the result of `perform`. Returning from a resuming handler arm without calling `resume` discards that continuation, so the arm's value becomes the result of the whole `handle … in` expression.

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

On success, `resume(value)` returns to `boot`, prints the startup message and produces `0`. On failure, the arm prints the error and returns `1` without resuming. The rest of `boot` never runs. That is an algebraic-effect exception: typed, lexically handled and explicit about whether the continuation survives.

The repository's native regression case [resume_abort_early_exit.osp](https://github.com/Nimblesite/osprey/blob/main/examples/tested/effects/resume_abort_early_exit.osp) exercises both branches and verifies the exact output. The broader [algebraic effects suite](https://github.com/Nimblesite/osprey/tree/main/examples/tested/effects) covers nested handlers, handler scoping, state, fibers and code that runs after `resume` returns.

### Combine algebraic effects with `Result`

Effects and results are not rival camps. A handler can implement an effect operation by returning an explicit `Result`, leaving the caller to handle the domain failure as data:

```osprey
effect Accounts {
    find: fn(int) -> Result<string, string>
}

fn greeting(id) -> Result<string, string> !Accounts =
    match perform Accounts.find(id) {
        Success { value }   => Success { value: "Hello, ${value}" }
        Error   { message } => Error { message: message }
    }

let lookup = handle Accounts
    find id => Error { message: "user ${id} was not found" }
in greeting(42)

match lookup {
    Success { value }   => print(value)
    Error   { message } => print(message)
}
```

In production, the `Accounts` handler could call a database. In a test, it could return a fixture. Either way, the effect chooses *where the capability comes from*, while `Result` preserves the ordinary success-or-failure contract of the lookup. This is a cleaner separation than throwing a database exception through every abstraction between storage and presentation.

## Result type or algebraic effect?

The two Osprey mechanisms are designed to solve different problems without creating an untyped escape hatch:

| Mechanism | Put failure here when… | The caller handles it with… |
| --- | --- | --- |
| `Result<T, E>` | failure is an outcome the caller may inspect, store, transform or propagate | an exhaustive `match` or an explicit fallback |
| Algebraic effect | the policy or capability belongs outside the function and direct style matters | a lexical handler that may substitute, resume or abort |
| Runtime termination | execution cannot safely continue because the runtime or process boundary has failed | a diagnostic and process boundary, not a routine API |

Conventional exceptions hard-code signalling, handler search and stack unwinding into a privileged feature. Algebraic effects expose the useful part—the non-local interaction—and make the handler choose the continuation policy. The same mechanism can recover, retry, substitute, redirect, resume or abort. Exception-style behaviour becomes one interpretation of a typed operation, not a hole punched through the language.

`Result` provides the complementary design. Luc Maranget's [*Warnings for Pattern Matching*](https://www.cambridge.org/core/services/aop-cambridge-core/content/view/3165B75113781E2431E3856972940347/S0956796807006223a.pdf/warnings-for-pattern-matching.pdf) formalises one such exhaustiveness analysis for ML-style patterns. A compiler can ask the question humans routinely forget: is there a value that no arm handles? Once failure is a variant, that same class of analysis can audit error handling.

The target is not less error handling. It is less *accidentally missing* error handling.

## Does Osprey enforce error handling today?

Osprey is alpha software, and the honest status matters more than the slogan.

The shipped compiler checks effect operation arguments and results, runs lexical handlers, supports native single-shot deep continuations, requires exhaustive matches for known `Result` values, and rejects direct payload access. The three examples above were compiled and run with the current compiler, and the linked files under `examples/tested` are regression-tested programs rather than aspirational pseudocode.

The `Result<T, E>` surface is generic, but the current compiler's conforming `Error` payload is string-backed; arbitrary values of `E` are not yet accepted as the `message`. Today, `E` therefore acts more like an error-category parameter around a built-in string payload than a fully generic payload sum. Richer discriminated-union error payloads remain deferred, which is why the examples here use `Result<T, string>`.

Two static-safety gaps remain.

First, Osprey does not yet retain and propagate complete effect rows through every function type and call. A missing handler can therefore reach runtime and abort with an `unhandled effect` diagnostic. Full compile-time rejection is tracked in the [algebraic-effects specification](/spec/0017-algebraiceffects/) and on the [implementation status page](/status/). Resuming handlers are currently native-only while their continuation runtime is completed for WebAssembly.

Second, Osprey currently permits `Result` auto-unwrapping in six convenience context classes. In those contexts the current code generator reads the success slot without branching on the discriminant, so an `Error` can be silently discarded and a zero/default payload observed. The compiler therefore does **not** yet force explicit handling of every `Result` on every path. Given the Rust evidence around `unwrap` and `expect`, those contexts deserve suspicion. They are alpha-era defects to close, not exceptions to the design principle.

So this post is the design bar, not a premature victory lap: operation rows must become complete static capabilities, and a represented error must never be silently discarded. Osprey already has the right primitives and executable semantics. The remaining job is to make the checker as uncompromising as the language's thesis.

## Stop hiding the second return channel

Exceptions were an understandable escape from sentinel values in languages whose ordinary types and branches could not tell the whole truth. Panics remain appropriate for violated invariants or states in which continuing is unsafe. Using either mechanism for expected, recoverable failures is the mistake argued here.

If an operation can recoverably fail, put the failure in `Result<T, E>` or in a typed effect contract. If the caller receives a sum, make it cover every constructor. If policy belongs higher in the program, install a handler and explicitly decide whether to resume or abort. Keep truly fatal runtime failure at the process boundary.

Automatic propagation is useful. Invisible propagation is not. Separation of policy is useful. An untyped second control-flow graph is not. The strongest case for exceptions is a case for automatic propagation and separated policy; neither benefit requires an untyped, invisible path.

It does not.

Exceptions and panics were a mistake. The better answer is simple: make failure part of the program, and make the compiler see every path.

## References

- John B. Goodenough, [*Exception Handling: Issues and a Proposed Notation*](https://doi.org/10.1145/361227.361230), Communications of the ACM, 1975.
- Barbara H. Liskov and Alan Snyder, [*Exception Handling in CLU*](https://doi.org/10.1109/TSE.1979.230191), IEEE Transactions on Software Engineering, 1979.
- Westley Weimer and George C. Necula, [*Exceptional Situations and Program Reliability*](https://web.eecs.umich.edu/~weimerw/p/weimer-toplas2008.pdf), ACM TOPLAS, 2008.
- Donna Malayeri and Jonathan Aldrich, [*Practical Exception Specifications*](https://doi.org/10.1007/11818502_11), Advanced Topics in Exception Handling Techniques, LNCS 4119, 2006.
- Bruno Cabral and Paulo Marques, [*Exception Handling: A Field Study in Java and .NET*](https://eden.dei.uc.pt/~bcabral/ExceptionHandling_A_Field_Study_camready.pdf), ECOOP, 2007.
- Maria Kechagia et al., [*The Exception Handling Riddle: An Empirical Study on the Android API*](https://doi.org/10.1016/j.jss.2018.04.034), Journal of Systems and Software, 2018.
- Kirsten Bradley and Michael W. Godfrey, [*A Study on the Effects of Exception Usage in Open-Source C++ Systems*](https://plg2.cs.uwaterloo.ca/~migod/papers/2019/scam19.pdf), SCAM, 2019.
- Boqin Qin et al., [*Understanding and Detecting Real-World Safety Issues in Rust*](https://doi.org/10.1109/TSE.2024.3380393), IEEE Transactions on Software Engineering, 2024.
- Gordon D. Plotkin and Matija Pretnar, [*Handling Algebraic Effects*](https://lmcs.episciences.org/705), Logical Methods in Computer Science, 2013.
- Andrej Bauer and Matija Pretnar, [*An Effect System for Algebraic Effects and Handlers*](https://lmcs.episciences.org/1153), Logical Methods in Computer Science, 2014.
- Daan Leijen, [*Koka: Programming with Row-Polymorphic Effect Types*](https://www.microsoft.com/en-us/research/wp-content/uploads/2016/02/koka-effects-2013.pdf), Microsoft Research, 2013.
- Luc Maranget, [*Warnings for Pattern Matching*](https://www.cambridge.org/core/services/aop-cambridge-core/content/view/3165B75113781E2431E3856972940347/S0956796807006223a.pdf/warnings-for-pattern-matching.pdf), Journal of Functional Programming, 2007.
- Oracle, [*The Java Language Specification, Chapter 11: Exceptions*](https://docs.oracle.com/javase/specs/jls/se25/html/jls-11.html).
- C++ Working Group, [*P0323R12: `std::expected`*](https://www.open-std.org/jtc1/sc22/wg21/docs/papers/2022/p0323r12.html), 2022.
- Herb Sutter, [*P0709R4: Zero-overhead deterministic exceptions*](https://www.open-std.org/jtc1/sc22/wg21/docs/papers/2019/p0709r4.pdf), 2019.
