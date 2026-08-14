# The Osprey Book — structural outline

## Shape of the first edition

The first edition targets about **42,600 words**, **154 print-equivalent pages**, and **42 purposeful visuals**. EPUB pages reflow, so word and visual budgets control scope; the page count is a design target.

| Material | Words | Print-equivalent pages | Visuals |
|---|---:|---:|---:|
| Front matter | 1,200 | 5 | 1 |
| Part I — Make the computer do something | 10,600 | 37 | 11 |
| Part II — Build honest data | 11,200 | 41 | 11 |
| Part III — Meet the outside world | 8,700 | 32 | 10 |
| Part IV — Make it yours | 8,900 | 31 | 9 |
| Back matter and glossary | 2,000 | 8 | 0 |
| **Total** | **42,600** | **154** | **42** |

## Reader journey

The reader begins with one source file and visible output. The book delays setup complexity, type-system terminology, and syntax alternatives until each becomes useful.

The running **Flight Log** project grows in four passes:

1. **Make it run.** Print a launch line, name values, write functions, and make decisions.
2. **Make it honest.** Transform collections, model valid states, expose failure, and test behavior.
3. **Let it interact.** Request effects, use files or HTTP, and coordinate fibers.
4. **Make it yours.** Choose a target, optionally choose another source flavor, inspect performance, and plan a capstone.

Default flavor carries the complete path. ML appears later as an optional translation of concepts the reader already owns. Future flavors can join the same role without restructuring the book.

## Recurring chapter contract

Every chapter follows the learning loop from `EDITORIAL-BRIEF.md`:

1. **Visible outcome** — something runs, changes, or is rejected for a useful reason.
2. **Small source** — Default flavor, runnable, and narrow enough to type by hand.
3. **Prediction** — one change the reader thinks through before running.
4. **Plain-language principle** — behavior first, specialist term second.
5. **Compiler feedback** — one purposeful mistake without invented diagnostics.
6. **Flight Log checkpoint** — one bounded addition to the running project.
7. **Agent handoff** — a paste-ready task plus verification command.
8. **Landing check** — what the reader can now prove.

No chapter introduces more than four conceptual families. Every factual visual is deterministic or directly captured from the pinned edition.

## Front matter — How to use this book

**Target:** 1,200 words · 5 pages · 1 visual

- Who this is for and what it assumes
- Browser-first and local-toolchain paths
- Why the book leads with Default flavor
- How Flight Log checkpoints work
- How to use a coding agent without outsourcing understanding
- Alpha-software and edition boundaries
- Visual: the four-part reading journey

## Part I — Make the computer do something

### Chapter 1 — One file, one result

**Target:** 3,000 words · 10 pages · 3 visuals

**Reader outcome:** Create, read, change, and run a small Default-flavor Osprey program; explain `fn`, `main`, a function call, an immutable binding, inference, and a pipeline in everyday language.

- Start in the Playground or verify a local install
- Run `fn main() = print("Hello from Osprey")`
- Read source from the outside in
- Turn a string into a named function
- Bind two immutable values with `let`
- Let inference remove obvious annotations
- Pass a result into `print` with `|>`
- Make one safe change and one purposeful compiler error
- Optional ML peek, clearly marked as skippable
- Flight Log checkpoint: print the first launch line
- Visuals: First Flight opener; program anatomy; source-to-output path

### Chapter 2 — Give values useful names

**Target:** 2,500 words · 9 pages · 2 visuals

**Reader outcome:** Use strings, booleans, numbers, immutable bindings, function parameters, interpolation, and small expressions without reaching for mutable state.

- A name points to a value; it is not a storage box that must change
- Choose names from the problem rather than from the type
- String interpolation and function parameters
- Checked integer arithmetic introduces `Result` without teaching recovery yet
- Keep functions small and expression-shaped
- Under the wing: immutability and referential transparency
- Flight Log checkpoint: derive a readable summary from project data
- Visuals: value graph; expression-in/expression-out

### Chapter 3 — Let the compiler work out the types

**Target:** 2,600 words · 9 pages · 3 visuals

**Reader outcome:** Read common Osprey types, trust inference for ordinary code, and interpret a type mismatch as useful information.

- Types describe possible values
- Inference from literals, calls, fields, and returns
- When an annotation adds real information
- Why a compiler can reject a bad combination before running it
- Generic identity and reusable functions, only as behavior
- Distinguish an inferred type variable from `any`
- Flight Log checkpoint: add a typed record boundary
- Visuals: inference trail; useful versus redundant annotations; mismatch locator

### Chapter 4 — Make every decision visible

**Target:** 2,500 words · 9 pages · 3 visuals

**Reader outcome:** Use `match` for booleans, known cases, and destructuring; explain why every possible case must be handled.

- Decisions are expressions that produce values
- Exact values, wildcard, and boolean matches
- Destructure one record payload
- Exhaustiveness and unreachable arms
- A first union with two meaningful cases
- Why visible cases beat hidden fall-through
- Flight Log checkpoint: render planned and completed entries
- Visuals: exhaustive fan-out; pattern anatomy; missing-case feedback

## Part II — Build honest data

### Chapter 5 — Move a collection through a pipeline

**Target:** 2,700 words · 10 pages · 3 visuals

**Reader outcome:** Build lists and maps, then use `range`, `map`, `filter`, `fold`, and `|>` to describe a transformation.

- Lists and persistent updates
- Map keys and safe lookup
- Pipelines read in data-flow order
- Transform, keep, combine, consume
- Lambdas only after named functions are clear
- No hidden mutable loop counter
- Flight Log checkpoint: filter active goals and produce a summary
- Visuals: pipeline flow; structural sharing; map/filter/fold roles

### Chapter 6 — Model only valid states

**Target:** 2,800 words · 10 pages · 3 visuals

**Reader outcome:** Design records and unions so impossible combinations are difficult or impossible to construct.

- Records group values that belong together
- Unions list every shape a value may take
- Named and positional payloads
- Pattern matching narrows a union safely
- Record update creates a new value
- Under the wing: products, sums, and algebraic data types
- Flight Log checkpoint: model `Planned`, `Learning`, and `Complete`
- Visuals: record versus union; invalid-state removal; immutable update sharing

### Chapter 7 — Keep failure in the open

**Target:** 3,000 words · 11 pages · 3 visuals

**Reader outcome:** Read and produce `Result`, handle both branches with `match`, and use `?:` only when a real fallback policy exists.

- Expected failure is an outcome, not a surprise exit
- `Success` and `Error`
- Parsing and checked integer arithmetic
- Preserve the original error value
- Exhaustive recovery versus an explicit fallback
- Why no exception or panic is needed for ordinary failure
- Flight Log checkpoint: parse a goal estimate honestly
- Visuals: two-route Result; propagation versus handling; fallback decision

### Chapter 8 — Prove what the program does

**Target:** 2,700 words · 10 pages · 2 visuals

**Reader outcome:** Write focused Osprey tests, use meaningful assertions, and separate compiler rejection from runtime behavior.

- A test states a behavior someone cares about
- Arrange a small value, act with a pure function, check the result
- Test every union and Result case
- Golden output for a whole interaction
- Compile-fail examples for forbidden programs
- Read locations and expectations before editing
- Flight Log checkpoint: cover summary and parse behavior
- Visuals: evidence pyramid; red-to-green feedback loop

## Part III — Meet the outside world

### Chapter 9 — Ask for an effect

**Target:** 3,000 words · 11 pages · 4 visuals

**Reader outcome:** Separate a request to perform outside work from the handler that decides how it happens.

- Pure decisions at the centre, outside work at the edge
- Declare an effect operation
- `perform` asks; a handler answers
- Inputs, outputs, and missing handlers are checked
- Replace production behavior in a test without service plumbing
- Native-only resumption limit
- Under the wing: algebraic effects without transformer stacks
- Flight Log checkpoint: request logging through an effect
- Visuals: request/handler boundary; direct-style call; test handler; missing-handler gate

### Chapter 10 — Read, write, and call the web

**Target:** 3,000 words · 11 pages · 3 visuals

**Reader outcome:** Perform one file or HTTP workflow while keeping every expected failure visible.

- Pick one supported native boundary for the running example
- Decode at the edge and model data inside
- Handle `Result` next to the decision that can recover
- Keep secrets and personal data out of logs
- WebAssembly runtime limits remain explicit
- C integrations cross Osprey's safety boundary
- Flight Log checkpoint: save or fetch a deterministic fixture
- Visuals: pure core/impure shell; boundary validation; platform support map

### Chapter 11 — Let work happen together

**Target:** 2,700 words · 10 pages · 3 visuals

**Reader outcome:** Spawn isolated fibers, pass values through channels, and avoid shared mutable state or colored function chains.

- Concurrency is multiple jobs making progress
- Fibers are lightweight work units
- Send values instead of sharing writable memory
- `spawn`, `send`, `recv`, `await`, and `yield`
- Failure and effects remain visible
- Native support boundary
- Flight Log checkpoint: process independent entries concurrently
- Visuals: isolated paths; channel handoff; structured lifetime

## Part IV — Make it yours

### Chapter 12 — Ship a real program

**Target:** 2,900 words · 10 pages · 3 visuals

**Reader outcome:** Check, run, compile, and choose an appropriate native or WebAssembly target without making unsupported portability claims.

- `--check`, `--run`, and `--compile`
- Native binaries through LLVM and clang
- WebAssembly's portable subset
- Debug information and the profiler
- Memory modes as build choices
- C FFI power and safety boundary
- Flight Log checkpoint: produce one native artifact and one supported Wasm artifact
- Visuals: compile pipeline; target decision; memory-mode comparison

### Chapter 13 — Choose another flavor when it helps

**Target:** 2,600 words · 9 pages · 3 visuals

**Reader outcome:** Recognise flavor as a source-level reading preference, translate one file between Default and ML, and verify unchanged behavior.

- Default remains the complete teaching path
- ML is an optional layout-and-currying surface
- Same shared checking and code generation after lowering
- One file selects one flavor; a project may contain current flavor extensions
- Translate with an agent, then `--check` and test
- Surface differences that are more than punctuation
- The architecture may support more flavors later
- Flight Log checkpoint: translate one pure module without changing its tests
- Visuals: source feathers converging; translation proof loop; curried versus flat call

### Chapter 14 — Build your own flight plan

**Target:** 3,400 words · 12 pages · 3 visuals

**Reader outcome:** Scope a small Osprey application, keep its pure centre visible, choose evidence, and describe the next learning step.

- Choose a problem with observable outcomes
- Sketch data states before functions
- Put expected failure in the design
- Name outside work as effects or boundary calls
- Add tests before expanding the surface
- Measure performance before selecting a memory mode or optimizing
- State alpha and roadmap constraints in a project README
- Three capstone routes: command-line tool, supported browser app, small native service
- Flight Log checkpoint: turn the running project into a one-page build plan
- Visuals: capstone canvas; evidence path; next-step map

## Back matter

### Appendices and next steps

**Target:** 1,000 words · 4 pages

- Appendix A — Command quick reference
- Appendix B — Default syntax one-page guide
- Appendix C — Agent prompt and verification recipe
- Appendix D — Current platform and feature qualifications
- Appendix E — Flight Log finished-source map
- Where to go next: documentation, specs, status, Playground, releases, and corrections

### Glossary

**Target:** 1,000 words · 4 pages

- Beginner-facing definitions for values, bindings, functions, expressions, and types
- Osprey terms for records, unions, matching, Results, effects, handlers, and fibers
- Flavor vocabulary that remains open to future surfaces
- Cross-links back to the chapters where each term becomes useful

## Explicitly out of scope for the first edition

- Compiler implementation details before they help the reader deploy or debug
- Category theory as prerequisite knowledge
- A complete API or built-in-function reference
- Teaching ML before the Default path is comfortable
- Claims that Osprey will always have exactly two source flavors
- Roadmap-only package workflows, complete imports, strict static memory, or hardware GPU execution
- Pretending all WebAssembly targets have native runtime services
- Presenting C calls as covered by Osprey's memory-safety guarantee
- Invented diagnostics, Playground screens, benchmark results, or performance rankings
