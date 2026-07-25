# Osprey — Project Messaging and Philosophy

This document is the source of truth for how Osprey explains itself. It is for
contributors and AI agents writing the README, website, specifications,
documentation, examples, release notes and code comments.

It is not a marketing campaign, a collection of slogans or a guide to promoting
the project. Its purpose is consistency: every part of the repository should
express the same language philosophy, emphasize the same features and distinguish
what exists today from what is still being built.

## The central idea

Osprey is a statically typed functional language for developers who want safety,
performance and elegance together.

It speaks to two communities:

- Mainstream developers coming from C#, Go, Rust, Java, Kotlin or Swift should
  find a readable systems language with native output, explicit failure,
  practical interop and familiar syntax.
- Functional programmers coming from ML, OCaml, F# or Haskell should find a real
  functional language with Hindley-Milner inference, algebraic data types,
  currying, persistent data and algebraic effects.

Do not present these as two different products. Osprey has one semantic core,
one type system, one optimiser and one runtime. Its Default and ML flavors are
two first-class ways to write the same language.

## The short description

> Osprey is a statically typed functional language with native LLVM output,
> typed algebraic effects, isolated fibers and two first-class syntaxes. It
> combines the practical safety and performance expected of a modern systems
> language with the composability and clarity of ML-style programming.

When less space is available:

> Safe systems programming with functional elegance.

## What matters most

Describe Osprey through three connected values. Features are evidence for these
values, not an unrelated checklist.

### Safety

- **Failures are values.** Osprey uses `Option`, `Result` and exhaustive pattern
  matching instead of null, exceptions or panics as routine control flow.
- **Arithmetic does not silently wrap.** Checked operations make failure
  explicit rather than producing a plausible but incorrect value.
- **Effects have typed contracts.** Effect operations declare their argument and
  result types. Effect-bearing signatures document the capabilities used by a
  function.
- **Concurrency is isolated.** Fibers communicate by moving or copying values
  through channels rather than sharing mutable state.
- **Osprey code is memory-safe by construction.** The C FFI is an explicit trust
  boundary and must be described as such.

### Performance

- **Osprey compiles through LLVM to native binaries.** There is no VM or JIT
  warm-up.
- **Fibers are lightweight.** Concurrent work does not require an operating
  system thread per task.
- **Persistent collections use structural sharing.** Functional data structures
  do not imply copying an entire collection for every update.
- **Memory management is a build choice.** Osprey supports precise ARC, native
  tracing GC and a checked `--static-memory` subset that rejects programs which
  require runtime reference counting.
- **C interop is direct.** Existing native libraries can be linked through typed
  declarations and opaque pointers.

Do not make broad benchmark claims unless the published benchmark data supports
the exact wording. Native compilation is a fact. Relative performance is a
measurement and must remain attached to reproducible results.

### Elegance

- **One general effect mechanism replaces several kinds of plumbing.** Logging,
  storage, dependency injection, test implementations, retries and concurrency
  can be expressed with named operations and lexical handlers.
- **Concurrent code remains direct.** There is no separate `async fn` kind and
  no future type propagated through every intermediate call.
- **Hindley-Milner inference removes redundant annotations.** Precise static
  types should not make source code noisy.
- **Algebraic data types model domains directly.** Immutable values and
  exhaustive matching keep state and behavior explicit.
- **Syntax serves the reader.** Default flavor uses familiar braces and calls;
  ML flavor uses offside layout, currying and whitespace application. Neither is
  the secondary or compromised form.

## The main feature list

Use this order when a README or documentation page needs to explain Osprey
quickly:

- **One language, two first-class flavors** — familiar brace syntax or genuine
  ML syntax, both lowered to the same canonical AST.
- **Hindley-Milner types and algebraic data types** — inference, immutable domain
  models and exhaustive pattern matching.
- **Typed algebraic effects** — explicit operations and lexical handlers for
  application capabilities and control flow.
- **Isolated fiber concurrency** — lightweight tasks and message passing without
  shared mutable state or colored functions.
- **Selectable memory management** — precise ARC, tracing GC or a checked static
  subset, without changing application source.
- **Native LLVM and WebAssembly output** — systems-oriented deployment with a
  direct C FFI.

Not every page needs every point. Preserve the order and select the features
relevant to the page rather than inventing a new identity for Osprey.

## Translating concepts for each audience

The implementation is the same; only the point of reference changes.

| Osprey concept | Mainstream systems vocabulary | Functional vocabulary |
| --- | --- | --- |
| Algebraic effects | Typed DI, replaceable implementations, capability contracts and retry policies | Named effects, effect-bearing signatures, handlers and continuations |
| Fibers | Lightweight isolated concurrency without mutex-driven shared state | Direct-style concurrency using the same control machinery as effects |
| Algebraic data types | Domain models whose cases must all be handled | Sums, products and exhaustive pattern matching |
| Persistent collections | Immutable collections with structural sharing | Persistent vectors and HAMTs with useful asymptotics |
| Memory modes | Select ARC, GC or a checked static subset at build time | Reclamation strategy remains outside source semantics |
| Syntax flavors | Braces, `fn`, named arguments and familiar control flow | Layout, currying, whitespace application and partial application |

Use the vocabulary that helps the reader recognize the idea, then introduce the
precise Osprey term. Do not dumb down the Default flavor or bury the ML flavor in
category-theory terminology.

## How to write about effects

Effects are central to Osprey, but they are not the entire identity of the
language. Explain the practical problem before the abstraction:

> A deeply nested function needs to log, access storage or emit a metric. In
> Osprey it performs a named operation. A lexical handler supplies the real
> implementation at the composition root, and a test can install a different
> handler without changing the function.

Useful comparisons include dependency injection, interfaces with test
implementations, ambient request context, retries and async runtimes. These are
bridges for understanding, not claims that every framework disappears.

Avoid describing effects as completely compile-time safe today. Operation
arguments and results are statically checked, but complete effect-row propagation
and compile-time rejection of missing handlers are still in progress. An
unhandled effect currently produces a runtime diagnostic.

## How to write about the two flavors

Default flavor is not a beginner mode, and ML flavor is not an experimental
language layered on top. Both are intended to expose the full language.

- Default (`.osp`) is familiar to developers from brace-based languages.
- ML (`.ospml`) is layout-sensitive and curry-by-default.
- Both lower to the same canonical AST before type checking and code generation.
- Files may choose a flavor independently; cross-flavor project integration is
  the design direction and must not be described as complete until it is.

Prefer “two flavors, one language.” Avoid “two tribes,” which suggests that the
project is dividing people rather than giving them a readable surface.

## Writing rules for contributors and agents

- Lead with what Osprey enables, then name the mechanism.
- Connect features to safety, performance or elegance.
- Write for technically literate developers. Be direct, specific and calm.
- Prefer concrete behavior over superlatives such as “revolutionary,” “complete,”
  “zero-cost” or “world-first.”
- Do not manufacture conflict with Rust, Go, Haskell, OCaml, Koka or Effekt.
  Comparisons should clarify design choices, not declare winners.
- Do not call ordinary documentation copy, positioning or philosophy “marketing.”
- Do not turn repository documentation into promotional copy, social posts,
  objection handling or calls to action.
- Keep limitations beside the claims they qualify, not hidden in a distant note.
- Use runnable examples and repository evidence whenever possible.
- Maintain one source of truth. If another page disagrees with this document or
  the implementation, investigate and correct the disagreement rather than
  repeating it.

## Current qualifications

These constraints materially affect how the language must be described:

- Complete effect-row propagation and compile-time missing-handler rejection are
  in progress; missing handlers currently produce a runtime diagnostic.
- Explicit single-shot `resume` is native-only today. WebAssembly supports
  non-resuming handler dispatch.
- Tail-call optimisation is not implemented.
- Generics, complete multi-file module imports and a package manager remain
  roadmap work.
- WebAssembly memory behavior must be described according to the current runtime,
  not inferred from native ARC or GC support.
- The C FFI is outside Osprey's memory-safety guarantee.

Update this section when implementation status changes. A claim becoming true in
the compiler does not automatically update the README, website or generated
documentation.
