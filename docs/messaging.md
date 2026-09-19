# Osprey — Project Messaging and Philosophy

This document is the source of truth for how Osprey explains itself. It is for
contributors and AI agents writing the README, website, specifications,
documentation, examples, release notes and code comments.

It is not a marketing campaign, a collection of slogans or a guide to promoting
the project. Its purpose is consistency: every part of the repository should
express the same language philosophy, emphasize the same features and distinguish
what exists today from what is still being built.

## The central idea

Osprey is a functional programming language for developers who want safe code,
fast programs and less clutter.

Osprey is deliberately practical. It takes useful ideas from functional and
systems programming and makes them work together without ceremony. Describe the
language in terms of code people can understand, programs they can ship and
problems the compiler or runtime helps them avoid. The tone should be
no-nonsense: clear, confident and specific, without hype or invented conflict.

It speaks to two communities:

- Mainstream developers coming from C#, Go, Rust, Java, Kotlin or Swift should
  find code they can read straight away, native programs they can ship, and
  practical ways to handle errors, concurrency and existing C libraries.
- Functional programmers coming from ML, OCaml, F# or Haskell should find the
  things they expect: inferred types, pattern matching, immutable data, currying
  and first-class effects.

Do not present these as two different products. Osprey is one language that can
be written in two styles. Both styles are checked, compiled and run in exactly
the same way.

## The short description

> Osprey is a practical functional language for building safe, fast native
> programs without the usual clutter. It combines strong inferred types,
> first-class effects and lightweight concurrency with a choice of familiar
> brace syntax or clean ML syntax.

When less space is available:

> Safe systems programming with functional elegance.

## What matters most

Describe Osprey through four connected values. Features are evidence for these
values, not an unrelated checklist.

### Practicality

- **Useful ideas without ceremony.** Strong types are inferred, effects remove
  plumbing and concurrent functions do not need a separate colour.
- **Failures are ordinary data.** Results make expected failure visible and let
  the caller decide what to do.
- **Deployment is direct.** Compile native binaries, WebAssembly, or C ABI application libraries for iOS and Android. Call existing C libraries when needed.
- **Choices remain local.** Select syntax per file and memory management per
  build instead of reshaping the whole application.

### Safety

- **Errors are part of the result.** Code must deal with failure instead of
  hiding it behind null, exceptions or crashes.
- **Arithmetic does not silently wrap.** Checked operations make failure
  explicit rather than producing a plausible but incorrect value.
- **Side effects are easy to see and replace.** Code says when it needs to log,
  load data or do other outside work. The application decides how that work is
  done.
- **Concurrency is isolated.** Fibers communicate by moving or copying values
  through channels rather than sharing mutable state.
- **Osprey code manages memory safely.** Calling C crosses that safety boundary,
  so C integrations still need careful review.

### Performance

- **Osprey compiles through LLVM to native binaries.** There is no VM or JIT
  warm-up.
- **Fibers are lightweight.** Concurrent work does not require an operating
  system thread per task.
- **Immutable collections reuse unchanged data.** Updating a collection does not
  mean copying the whole thing.
- **Memory management is a build choice.** Choose reference counting, garbage
  collection or a strict mode that refuses code needing runtime memory tracking.
- **Existing C libraries are usable.** Declare the functions you need and link
  the library into the program.

Do not make broad benchmark claims unless the published benchmark data supports
the exact wording. Native compilation is a fact. Relative performance is a
measurement and must remain attached to reproducible results.

### Elegance

- **First-class effects remove plumbing.** Code can ask to log, load data or
  retry work without carrying service objects through every function. Tests can
  replace the real behavior without changing the code under test.
- **Concurrent code remains direct.** There is no separate `async fn` kind and
  no future type propagated through every intermediate call.
- **The compiler works out the types.** You keep strong type checking without
  writing obvious types everywhere.
- **Data models list every possible case.** Pattern matching makes code handle
  those cases directly and the compiler checks that none were forgotten.
- **Syntax serves the reader.** Default flavor uses familiar braces and calls;
  ML flavor uses offside layout, currying and whitespace application. Neither is
  the secondary or compromised form.

## The main feature list

Use this order when a README or documentation page needs to explain Osprey
quickly:

- **One language, two first-class flavors** — familiar brace syntax or genuine
  ML syntax, with the same behavior and performance.
- **Strong types without noisy annotations** — the compiler works out types,
  while pattern matching checks that every case is handled.
- **First-class effects** — use logging, storage, retries and other outside work
  without passing service objects through every layer of the program.
- **Isolated fiber concurrency** — lightweight tasks and message passing without
  shared mutable state or colored functions.
- **Selectable memory management** — reference counting, garbage collection or
  a strict static mode, without rewriting the application.
- **Native LLVM, WebAssembly, and mobile C ABI output** — compile application logic for native iOS and Android hosts, with target-specific capability checks.

Not every page needs every point. Preserve the order and select the features
relevant to the page rather than inventing a new identity for Osprey.

## Translating concepts for each audience

The implementation is the same; only the point of reference changes.

| Osprey concept | Mainstream systems vocabulary | Functional vocabulary |
| --- | --- | --- |
| First-class effects | Replace dependency injection, test doubles and retry wrappers with one language feature | Use effects without building transformer stacks or threading `IO` through everything |
| Fibers | Lightweight concurrency without shared-state locking | Write concurrent code in direct style without wrapping every result in a future |
| Data types and matching | List every valid state and make the compiler check every case | Sums, products and exhaustive pattern matching |
| Immutable collections | Update data safely without copying the whole collection | Persistent collections with efficient structural sharing |
| Memory modes | Choose reference counting, garbage collection or strict static memory | Change memory management without changing the meaning of the program |
| Syntax flavors | Braces, `fn`, named arguments and familiar control flow | Layout, currying, whitespace application and partial application |

Use the vocabulary that helps the reader recognize the idea, then introduce the
precise Osprey term. Do not dumb down the Default flavor or bury the ML flavor in
category-theory terminology.

## How to write about effects

Effects are central to Osprey, but they are not the entire language. Start with
what they save developers from:

> Your code needs to save something to a database. Normally you pass a database
> object through several functions just so the last one can use it. In Osprey,
> the last function simply asks to save the data. The application decides what
> “save” means in production. A test can make it save to memory instead. The
> functions in between do not need to know or change.

This same idea works for logging, metrics, retries and other work that normally
needs dependency injection, global state or wrapper libraries. These comparisons
help explain effects; they do not mean every framework automatically disappears.

The contract and delivery evidence live in [the effects spec](specs/0017-AlgebraicEffects.md)
and [plan 0016](plans/0016-algebraic-effects-and-handlers.md). Describe effect
management first: what an application needs to do, which handler supplies it,
and where that choice applies. Distinguish demonstrated compiler behavior from
specified targets; do not infer correctness from old examples alone.

## How to write about the two flavors

Default flavor is not a beginner mode, and ML flavor is not an experimental
language layered on top. Both are intended to expose the full language.

- Default (`.osp`) is familiar to developers from brace-based languages.
- ML (`.ospml`) is layout-sensitive and curry-by-default.
- Both become the same internal program before type checking and compilation.
- Files may choose a flavor independently, and project modules can import files in either flavor. The [mobile example](../examples/mobile/README.md) combines ML application modules with a small Default C ABI entry file.

Prefer “two flavors, one language.” Avoid “two tribes,” which suggests that the
project is dividing people rather than giving them a readable surface.

## Writing rules for contributors and agents

- Lead with what Osprey enables, then name the mechanism.
- Keep the overall picture balanced: practicality, safety, performance and
  elegance should define the language. Syntax flavors are an important feature,
  not the whole identity.
- Connect features to safety, performance or elegance.
- Write for technically literate developers. Be direct, specific and calm.
- Prefer concrete behavior over superlatives such as “revolutionary,” “complete,”
  “zero-cost” or “world-first.”
- Explain specialist terms the first time they appear. Prefer the everyday
  explanation when the specialist term adds nothing.
- Internal compiler language belongs in specifications and implementation notes,
  not in introductory documentation. Never copy phrases such as “effect-row
  propagation,” “missing-handler rejection,” “canonical AST” or “reclamation
  strategy” into reader-facing blurbs without explaining them in plain English.
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

- [Plan 0016](plans/0016-algebraic-effects-and-handlers.md) owns current effects
  support, known failures and target limitations. Do not maintain a second
  capability checklist here or present an unfinished contract as shipped.
- Static interpretation removes effect dispatch; the resulting computation can
  still make ordinary calls, allocate and read runtime values. Say “no runtime
  effect dispatch,” not “no runtime cost.” Dependency analysis is a sound set
  of possible reads, not a promise to know precisely which branch will run.
- The [reactive mobile application](../examples/mobile/README.md) is implemented using ordinary Osprey modules and explicit event/state/command transitions. Osprey defines its screen tree, state, GitHub request and decoding logic, SQLite schema and statements, offline cache, search, bookmarks, notes, and priorities. Native hosts render the tree and execute platform services. This working application does not imply that the staged-effects reactive runtime is implemented.

- Integer arithmetic returns a `Result`, not a plain `int`. `+`, `-`, `*`, `abs`
  and `intDiv` all carry a `MathError` channel that the caller must discharge,
  usually with `?:`. That is a real cost to describe honestly: a `?: 0` on an
  overflowing expression fabricates a value and the program exits successfully
  with a wrong answer ([#230](https://github.com/Nimblesite/osprey/issues/230)).
  [Spec 0037](specs/0037-ArithmeticEffects.md) specifies the replacement — plain
  `int` with faults dispatched to an `Arith` handler — as a normative target;
  **none of it is implemented**. Describe today's arithmetic as checked and
  explicit, and the effect form only as specified behaviour.
- Tail-call optimisation is not implemented.
- Multi-file project modules and cross-flavor imports are implemented. A package manager remains roadmap work; describe further module or generic features according to their individual implementation status.
- GPU computation is a typed language surface with a host execution backend:
  `GpuBuffer` types, data-parallel kernels the compiler proves pure at compile
  time, and dense buffers that run as native loops today
  (docs/specs/0034-GPUComputation.md). Device code generation (PTX, Metal,
  WebGPU) is staged roadmap work — do not describe Osprey as executing on GPU
  hardware yet.
- WebAssembly memory behavior must be checked against what the browser runtime
  currently supports rather than assumed from native builds.
- Mobile C ABI targets currently support the default allocator, which retains general allocations for the process lifetime. They do not expose a stable library teardown or returned-string release API. Platform networking belongs to the native host; unsupported built-in native process, HTTP, WebSocket, and resumable-effect operations fail compilation.
- The C FFI is outside Osprey's memory-safety guarantee.

Update this section when implementation status changes. A claim becoming true in
the compiler does not automatically update the README, website or generated
documentation.
