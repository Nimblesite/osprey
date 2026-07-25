# Osprey — Messaging Kit

Copy blocks for the site, README, socials and ads. Lead with the shared product story:
**safety, performance and elegance**. Then translate it for two audiences:
mainstream systems developers and functional programmers.

---

## 1. Positioning statement (the one paragraph everything else compresses)

> Osprey is a statically typed functional language for building fast, reliable
> software without giving up readable code. It compiles through LLVM to native
> binaries, isolates concurrent work with fibers, and makes failure and effects
> explicit in the type system. Hindley-Milner inference, algebraic data types and
> algebraic effects keep the code direct and composable. Choose familiar braces
> or a true ML syntax; both use the same type checker, optimiser and runtime.

---

## 2. Hero lines

**Primary (site):**
> **Safe systems programming. Functional elegance.**
> Native binaries, typed effects and isolated concurrency in one language with
> two first-class syntaxes.

**Alternates:**
- *Direct-style code. Typed effects. No colored functions, no monad stack, no runtime lock-in.*
- *The safety you expect from a modern systems language, with the composition you expect from ML.*
- *Familiar when you want it. Functional all the way down.*
- *One language for native performance, explicit effects and readable code.*
- *Swap the handler, not the code.*

**Subhead:**
> Osprey combines checked arithmetic, algebraic data types and isolated fibers
> with LLVM-native code generation. Typed effects replace much of the plumbing
> around dependency injection, mocking and async code. Memory management is a
> build choice: precise ARC, tracing GC, or a checked static-memory subset.

---

## 3. Osprey in 30 seconds

### Safety

- **Failures are values.** No null, exceptions or panics as routine control flow;
  use `Option`, `Result` and exhaustive pattern matching.
- **Arithmetic does not silently wrap.** Checked operations return a typed error.
- **Effects have typed contracts.** Operations declare their argument and result
  types, and effect-bearing signatures document required capabilities. Complete
  effect-row propagation and missing-handler rejection are still in progress.
- **Concurrency is isolated.** Fibers communicate by moving or copying values
  over channels, avoiding shared mutable state and its data races.
- **Memory is safe by construction in Osprey code.** The C FFI is an explicit
  boundary that remains yours to audit.

### Performance

- **Native code through LLVM.** No VM and no JIT warm-up.
- **Lightweight fibers.** Run large numbers of concurrent tasks without an OS
  thread per task.
- **Persistent collections with structural sharing.** Immutable lists and maps
  retain practical asymptotics without copying whole structures.
- **Choose memory management at build time.** Use precise ARC, native tracing GC,
  or `--static-memory`, which rejects programs that require runtime reference
  counting.
- **A direct C FFI.** Link existing native libraries through typed declarations
  and opaque pointers.

### Elegance

- **One general effect system.** Logging, storage, retries, testing and
  concurrency use the same composable mechanism instead of separate frameworks.
- **Direct-style concurrent code.** No `async fn`, future types or `.await`
  threaded through every intermediate call.
- **Hindley-Milner inference.** Types stay precise without turning every function
  into an annotation exercise.
- **Functional data modelling.** Algebraic data types, immutable values and
  exhaustive matching make domain logic compact and explicit.
- **Two first-class syntaxes.** Use familiar braces or layout-sensitive,
  curry-by-default ML syntax. Both lower to the same canonical AST.

### The same features, in each audience's language

| Osprey feature | For C#, Go, Rust and Java developers | For ML and FP developers |
| --- | --- | --- |
| Typed algebraic effects | DI, mocks, ambient context and retry policies without framework plumbing | Named effects and lexical handlers without transformer stacks or `lift` |
| Isolated fibers | Lightweight concurrency without shared-state locking or colored functions | Direct-style concurrency built on the same continuation machinery as other effects |
| Algebraic data types | Explicit domain models with exhaustive handling of every case | Familiar sums, products and pattern matching with HM inference |
| Pluggable memory | Choose ARC, GC or a checked zero-runtime-memory subset at build time | Pure source semantics remain independent of reclamation strategy |
| Two syntax flavors | Braces, `fn`, named arguments and familiar control flow | Offside layout, currying, whitespace application and partial application |
| LLVM code generation | Native binaries and straightforward C interop | High-level functional composition without giving up native output |

---

## 4. Two flavors, one language

### Default flavor — familiar to mainstream developers

**Headline:** *If you can read Go, you can read Osprey.*

Braces, `fn`, `if`/`else`, named arguments and `match`. It is designed to be
readable on first contact for developers coming from C#, Go, Rust, Java, Kotlin
or Swift. This is a syntax choice, not a reduced version of the language.

- **Native binaries via LLVM.** No VM, no JIT warmup, no shipping a runtime.
- **C FFI that actually works.** `// @link: sqlite3`, `extern fn`, typed signatures, opaque `Ptr` handles with no arithmetic and no dereference. The SQLite integration in the repo is driven entirely through it.
- **Memory management you choose at link time.** Perceus-style precise ARC (non-atomic — fibers share nothing, so no atomics anywhere) or a tracing GC. Or `--static-memory`, which *fails the build* on any construct that would need a refcount — Rust-class output, no borrow checker to fight, and byte-identical behaviour to the default mode.
- **Arithmetic that can't silently wrap.** Checked operations return `Result<int, MathError>`.
- **No null. No exceptions. No panics as control flow.** Option and Result, exhaustively matched.
- **Fibers instead of threads.** Spawn thousands, await out of order, message-pass over channels, and never reach for a mutex.

> **Pull quote:** *You wanted Rust's output without Rust's ceremony. `--static-memory` compiles to zero runtime memory operations and tells you exactly which value defeated it and why.*

### ML flavor — native to functional programmers

**Headline:** *Offside rule. Curry by default. Effects instead of transformers.*

Layout-sensitive, with whitespace application `f a b`, `=>` clauses and partial
application that falls naturally out of currying. It is not a compatibility
dialect: both flavors lower to the **same canonical AST** before type checking.
After lowering, nothing downstream can tell which one you wrote.

- **Hindley-Milner inference.** Annotate when you want documentation, not when the compiler is stuck.
- **No `IO` at the top of every signature.** An effect row is a set of named operations, not a monolith. `!Logger` means logging, not "anything at all."
- **No transformer stack. No `lift`. No `mtl`.** Handlers nest lexically and compose. The inner one wins.
- **Persistent collections with real asymptotics.** `List` is a 32-way bitmapped vector trie; `Map` is a HAMT with bitmap-packed children. O(log₃₂ n). Structural sharing means old versions stay valid in O(1) extra space.
- **The heap is provably acyclic**, so reference counting is *complete* — no cycle collector, and the ARC and tracing backends are observationally identical.
- **Mix flavors per file, in one folder.** `math.ospml` and `app.osp` compile into one program. The team never has to pick a tribe.

> **Pull quote:** *The currying twin of a Default function lowers to a machine-checked identical AST. This is one language wearing two faces, not a transpiler.*

---

## 5. Algebraic effects — the practical pitch

**Headline:** *It's dependency injection that the type checker enforces.*

**Lead:**
> Every codebase has the same problem: some deep function needs to log, or read the database, or emit a metric, and now that dependency has to travel through nine layers of signatures — or you give up and reach for a global, a DI container, or a mock framework. Effects delete that problem. Declare the operation. `perform` it where you need it. Bind it to a real implementation at your composition root. To test it, change the handler and nothing else.

**Body:**

```osprey
effect Logger { log: fn(string) -> Unit }

fn greet(name: string) -> Unit !Logger =
  perform Logger.log("Hello, ${name}!")

// Production
handle Logger log msg => print(msg) in greet("Alice")

// Test — same code, silent handler, no mock framework
handle Logger log msg => 0 in greet("Bob")
```

**What that buys you, concretely:**

| The thing you do today | The thing you do in Osprey |
| --- | --- |
| DI container, service locator, constructor injection | `handle … in` at the composition root |
| Mocking framework, `jest.mock`, interface + fake impl | A three-line handler arm |
| Thread-locals / ambient context for request IDs, tracing | A handler installed around the request |
| `IORef` + monad transformer stack + `lift` | An effect row on the signature |
| Passing a `Logger` through nine call frames | `!Logger` on the one function that uses it |
| Hardcoded `sqlite3_open` in your API layer | `Ledger::Store` — bound to SQLite in `main`, to a fake in tests |
| Retry / timeout / backoff wrapper libraries | A handler that decides whether to `resume` |

**The capability angle (this one lands with systems people):** *an effect row is a permission list.* A function whose type doesn't mention `Store` cannot touch your database — and the browser build of Talon Bank literally cannot import the storage implementation. Capability safety falls out of the type, not out of code review.

**Nesting:** handlers are lexical and nest. Silence one noisy subsystem for one call by wrapping it. No flags, no globals, no reconfiguration.

**Proof, not a whitepaper:** the reference app (Talon Bank) uses `Ledger::Store` for storage, `Api::Audit` for audit logging, and a `Metrics` state module owning a request counter inside its handler region. Importing the module initializes nothing. Only the composition root installs the SQLite implementation.

> **Pull quote:** *You've been writing effect handlers your whole career. They were just called "the DI container," "the mock," and "that wrapper we added for retries."*

---

## 6. Fibers vs async/await — the section that does the work

**Headline:** *async/await is an effect system with exactly one effect, hardcoded into your type signatures.*

**Lead:**
> Osprey has a general effect system with real continuations, so concurrency doesn't need a keyword, a coloring rule, or a runtime you marry for life. `spawn` an expression, `await` it when you want the value. That's the whole API.

```osprey
let a = spawn work(6)
let b = spawn work(7)
print("a=${await(a)}, b=${await(b)}")   // out-of-order awaits are fine

let ch = Channel(1)
send(ch, 42)
print("got ${recv(ch)}")
```

**Why this beats async/await, point by point:**

1. **No function coloring.** A function that does IO is not a different *kind* of function. There is no `async fn`, so there is no async-vs-sync split running up your entire call graph — and no ecosystem that has to ship two versions of every library (`reqwest` and `reqwest::blocking`, `requests` and `httpx`).
2. **No `.await` confetti.** You write straight-line code. `await` appears where you actually need a value, not at every intermediate call.
3. **No executor lock-in.** Tokio vs async-std vs smol vs "which runtime does this crate assume" is a category of problem Osprey doesn't have. Scheduling is a handler.
4. **No `Pin`, no `Send + Sync + 'static`, no `Box<dyn Future>`.** Fibers share nothing — values cross a `spawn` or a channel by move or copy, never by sharing — so there's no shared-state puzzle to encode in the type system. It also means every reference count in the runtime is non-atomic. Isolation buys you *speed*, not just safety.
5. **Real stacks, real debugging.** A fiber has a stack. You get a call stack, not a state-machine transform that lost your frames.
6. **`await` isn't the only shape.** `fiberDone` is a non-blocking probe. `yield` hands control back cooperatively. Channels do message passing. You aren't limited to what one combinator library thought of.
7. **The same machinery does more than concurrency.** Continuations are a language primitive here, so retry, backtracking, generators, and transactional rollback are all *handlers* — not four separate libraries with four separate idioms.
8. **Testing is deterministic.** Swap the handler, control the scheduling.

> **Pull quote:** *Rust, Python, JS and C# each bolted one effect — asynchrony — into the type system and called it a keyword. Osprey generalized it. async/await is a special case of something Osprey ships whole.*

**Honesty note to keep in the copy (it earns trust, and the crowd will find it anyway):** explicit single-shot `resume` is native-only today. Wasm gets non-resuming handler dispatch; a thread-free continuation strategy for Wasm is in progress.

---

## 7. Memory — pluggable, invisible, provably interchangeable

**Headline:** *Memory management is a link-time decision, not a language you have to learn.*

- **`--memory=arc` (default):** Perceus-style precise reference counting on the shared residue only, statically elided wherever the compiler can prove ownership. Non-atomic, because fibers share nothing. Precise, so it works on every target — including `wasm32`, where it's the only reclaiming option.
- **`--memory=gc`:** conservative tracing collector. Native targets only. It exists as the *conformance oracle*: if any program can tell the two apart, the spec is broken.
- **`--static-memory`:** compilation fails at every point that would need a runtime refcount, naming the shared value and the conflicting owners. What compiles contains **zero** runtime memory operations and behaves byte-for-byte like the default. Not a dialect — a strict subset.
- **Custom managers:** the boundary is a small C interface (alloc/retain/release/collect). Link an arena, a pool, a debugging allocator.

**Why it's sound rather than lucky:** immutable values can't reference values created after them, so the heap is acyclic by construction. Reference counting is therefore *complete* — no cycle collector, ever. And because no program can observe when or whether memory is reclaimed (no finalizers, no destructors, no destruction ordering — by spec), every backend is interchangeable. Resource cleanup is a *scoped effect handler*, not a destructor tied to a value's death.

> **Pull quote:** *No finalizers, and there never will be. Files and sockets close because a handler brackets them, not because a refcount happened to hit zero.*

---

## 8. Web: Wasm + React

**Headline:** *Your application logic in Wasm. React as a dumb renderer.*

Osprey owns the model, the transitions, routing, validation and the view document. A small JS host turns the emitted element tree into React elements via `createElement` and lets React reconcile the DOM. Browser work — `fetch`, focus, history — travels back as *commands*: data, not calls.

- **No `useState`, no reducers, no context.** One authoritative serializable model lives in Osprey. React is the renderer, not the source of truth.
- **One whole-document envelope per render** — `{model, view, commands}` — not a chatty per-node FFI.
- **The same type checker and effect system** as your native server. Talon Bank ships both: an Osprey Wasm client and a native Osprey server with a SQLite-backed `Store` handler.
- **`update` stays deterministic** for a given event and model, because it returns command *descriptions* rather than performing side effects.

Be straight about status: the host is a reference implementation to fork, not a published npm framework. Routing is hash-based. Client-only rendering. The Wasm runtime currently uses the non-reclaiming allocator.

---

## 9. Performance

**Headline:** *It's a functional language. It's also a native binary.*

Compiles through LLVM with stream fusion and structural sharing, benchmarked head-to-head against Rust, C, OCaml and Haskell on the same naive algorithm with the same parameters, output verified against an oracle before anything is timed. The harness generates the tables mechanically — never hand-edited — and you can re-run it with `make bench`.

And say the hard part out loud, because it's a *feature*: **Osprey does checked arithmetic on every operation.** Rust is benchmarked with `-C overflow-checks=off` to match its release profile. Part of any gap is the cost of a real safety guarantee the others aren't paying for.

> ⚠️ **See §11 before shipping any perf claim.**

---

## 10. Channel-specific copy

**README one-liner:**
> A statically typed functional language with a complete algebraic effect system, fibers instead of async/await, pluggable memory (ARC/GC/none), and two first-class syntaxes — compiled to native code through LLVM.

**X / HN title:**
> Osprey: a functional language where async/await, DI, mocking and retries are all the same feature

**X thread opener:**
> Your language has an effect system. It has exactly one effect, it's called `async`, it's hardcoded, and it colors every function it touches.
>
> Osprey has the general version. `spawn`/`await` is just one handler. So is logging. So is your database. So is your test double. 🧵

**Newsletter ad (PyCoder's-style, ~40 words):**
> **Osprey** — typed algebraic effects that replace your DI container, your mock framework and your async runtime, in a language that compiles to native code through LLVM. Braces or offside rule, your choice. Try it in the browser: ospreylang.dev

**Elevator pitch (verbal, 15 seconds):**
> It's a functional language with a real effect system. Instead of an async keyword, a DI framework and a mocking library, you get one feature that does all three — and it compiles to a native binary.

**Objection handling:**

| "Why not just…" | Answer |
| --- | --- |
| **Rust** | You get the native output and no borrow checker to fight. `--static-memory` is Rust-class allocation discipline as an opt-in build mode, not a tax on every line. And no colored functions. |
| **Go** | Goroutines without shared mutable state, plus a real type system, exhaustive matching, and effects instead of `context.Context` threaded through everything. |
| **Haskell** | Same elegance, effects instead of transformer stacks, native output with predictable memory, and no laziness surprises. |
| **OCaml 5** | Comparable effect story, but Osprey ships two syntaxes, checked arithmetic, LLVM codegen, and effects typed in the signature from day one. |
| **Koka / Effekt** | Those proved the ideas. Osprey is aimed at shipping software: C FFI, SQLite, HTTPS, TUIs, Wasm, a VS Code extension, `brew install`. |

---

## 11. Claims audit — fix these before the messaging goes out

Your audience clicks through. Three live inconsistencies will get you shredded on HN/Lobsters, and all three are cheap to fix:

1. **The homepage says Osprey "matches C and Rust on CPU" and "beats OCaml and Haskell across an 18-case suite." The linked benchmarks page currently shows one case (`wordfreq`), zero outright wins, and 14.12× vs C.** Either the published run is a filtered/partial build, or the claim is stale. Whichever it is, the headline and the linked evidence must agree — a sceptic will click, and this is the single most damaging thing on the site right now. Re-run the full suite and let the generated numbers set the claim.
2. **The memory spec still says "Not implemented"** while the benchmark harness has an `Osprey (GC)` column and the CLI has `--memory=gc` / `--memory=arc`. Update spec 0018's status block, or every ARC/GC line in this kit is contradicted by your own docs.
3. **"Memory-safe, no GC pauses"** in the homepage comparison table is now wrong in both directions — you *have* a GC backend, and ARC is the default. Replace with: *"Choose at link time: precise ARC, tracing GC, or zero runtime memory management."* That's a better claim anyway.

Also keep visible (they build credibility rather than costing it): static effect coverage checking is incomplete — a missing handler is a runtime diagnostic today, not a compile error; no TCO yet (loops go through `range |> fold`); resumable handlers are native-only; generics, modules-with-imports, and a package manager are roadmap.

**Don't say "memory safe" unqualified.** You have a C FFI with raw handles. Say *"memory-safe by construction in Osprey code; FFI boundaries are yours to audit."* Everyone respects that sentence and nobody respects the unqualified one.
