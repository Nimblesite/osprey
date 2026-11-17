# What is Osprey?

This brief answers one question in a plain, consistent way: **"What is this?"** Use it for the homepage hero, the README opening, talks, posts and conversations. For wording rules, banned phrases and the qualifications that must sit beside each claim, see [messaging.md](messaging.md). That brief still governs everything this one does not cover.

## The answer

> **Osprey is one platform for building whole products (native apps, backends, AI models, audio software and verified maths) in a single safe, fast language.**

If someone wants to know more, give the second sentence:

> **You write the logic once, and the compiler checks every part. Handlers decide where it runs: SwiftUI or WinUI, CPU or GPU, the real disk or a test.**

Use those two sentences together. Every longer explanation expands them; none replaces them.

## Why "platform", not "language"

Calling Osprey "a functional language with algebraic effects" undersells it and loses most listeners. Most languages already have types and pattern matching, so effects are the only feature that sounds new, and a feature on its own is not a reason to switch.

What people get is everything a product needs, built from one source, in one type system, with one toolchain:

| You build | With Osprey |
| --- | --- |
| Desktop and mobile apps | Osprey Native: one app drawn with SwiftUI, WinUI and Android widgets, or on a GPU canvas |
| Backends | HTTP and WebSocket servers, SQL and storage behind handlers, fibers for concurrency |
| AI and ML | Python-flavor ports of `torch`, `transformers`, `numpy` and more, running GPT-2 on Metal or OpenCL |
| Audio | `@osprey/synth`: the Prism modular synth, which hosts third-party VST3 plugins, and Light Orchestra, played from phones |
| Maths and proof | Formulas written in Lean that compile to Osprey, and a proof checker developed against Lean as an oracle |
| Tooling | Compiler, language server, debugger, profiler, formatter, package manager, VS Code extension, KLEE bug hunting |

Normally each row needs its own language, runtime and build system, joined at boundaries where bugs and glue code pile up. In Osprey those boundaries are **effects**: typed and checked by the compiler, and swappable without changing the code that uses them. Effects are not the headline. They are the reason one language can stretch this far.

## One idea, shown everywhere

Every demo should show the same move: **the same logic, a different handler.**

- **Apps:** the Issue Inbox state and screens run on iOS and Android, and only the host differs.
- **AI:** GPT-2 is written once against a `Tensor` effect. One handler runs it on host lists and another runs it on the GPU.
- **Backends:** an install writes to the real disk in production, and a test swaps in a full disk to check the rollback.
- **Audio:** Prism's graph renders offline in tests and live in the native studio.

When you run out of space, keep the single example that best fits the audience and cut the rest.

## Answers by audience

- **App developers:** "Write your app once in Osprey and ship it with each platform's real native widgets: SwiftUI, WinUI and Android. It is not a JavaScript engine and not a widget toolkit that draws its own copies."
- **Python and ML people:** "Keep `def`, `lambda` and f-strings, and gain static types and native GPU kernels. GPT-2 runs from the published weights."
- **Backend developers:** "No panics, no hidden I/O. Every failure is a `Result`, and every outside call is an effect you can swap out in tests."
- **Audio developers:** "Build the DSP graph, the editor and the plugin in one language, then test the sound offline."
- **Mathematicians:** "Write the formula in Lean and run it as compiled Osprey."
- **Skeptics:** "It is actively evolving. Here is a real app, here is its test suite, try it on your workload." Then link the limits in [messaging.md](messaging.md#qualifications-to-preserve).

## What the language gives the platform

The three pillars from [messaging.md](messaging.md#the-three-pillars) still hold, but they now explain *why the platform holds together*:

- **Safe:** no panics, exhaustive matching and explicit `Result` failures. Joining many domains is only sane if no part can crash another.
- **Elegant:** inferred types and effects instead of plumbing. One codebase stays small enough for people and AI agents to read.
- **Fast:** LLVM-native code with no VM, and GPU kernels when a handler asks for them. One language does not mean a slow common denominator.

## Don't say

- "A new programming language" as the first line. It answers the wrong question.
- "Replaces Swift, Python, Rust…" Osprey hosts platform code and interoperates with it; it does not claim to replace those languages.
- "Runs any Python program." Python flavor is Osprey syntax, and the ports are Osprey packages.
- "Does everything." Name the real products instead: Issue Inbox, Prism, Light Orchestra, Osprey Agent, GPT-2, the storage browser.
- Effects jargon before the reader knows what Osprey is for. Explain `perform` and `handle` only after the answer has landed.

## Checklist for a "what is this?" answer

- It opens with **one platform, whole products, one safe, fast language**.
- It names at least two concrete domains, each backed by a real example.
- It shows **same logic, different handler** once.
- It keeps the [maturity wording](messaging.md#maturity-and-invitation) and the limits beside any claim that needs them.
- It ends with one next step: try the playground, run an example, or read a guide.
