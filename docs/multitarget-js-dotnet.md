# Multi-Targeting Osprey To JavaScript And .NET IL

This is a feasibility note, not an accepted target spec. It answers two
questions:

- How hard would it be to add JavaScript and/or .NET IL backends?
- Can Osprey's algebraic effects and fibers be preserved there, or would the
  result be too lossy?

## Short Answer

JavaScript and .NET IL are both possible targets, but neither is a small
"change the linker" job like the existing WebAssembly target. Osprey's current
backend emits textual LLVM IR directly while walking the AST, then links against
a C runtime. A JavaScript or IL target needs either a second full backend, or an
intermediate representation shared by the LLVM, JS, and IL emitters.

The portable Osprey core is feasible on both targets: functions, closures,
ADTs/records, pattern matching, strings, lists/maps, `Result`, and the current
tail-resume effect handlers can be compiled without semantic loss.

The hard line is explicit effect `resume` and true direct-style fibers. Neither
JavaScript nor .NET IL gives a portable primitive to capture an arbitrary live
call stack segment as a resumable continuation. That does not make Osprey
impossible on those targets; it means Osprey must own the suspension machinery:
CPS, generators, or compiler-emitted state machines for any code that can
perform/resume/yield.

Recommended order:

1. Add a target-neutral lowered IR and runtime ABI boundary.
2. Implement a JavaScript MVP for the portable core plus tail-resume effects.
3. Implement a .NET IL MVP for the same surface.
4. Add one shared "resumable lowering" pass for explicit `resume`, `yield`,
   blocking channel operations, and async host interop.

## Current Osprey Baseline

Osprey today is a Rust compiler that emits textual LLVM IR and links it with C
runtime archives. The CLI already has two targets, but both still use the LLVM
path:

- `native`: LLVM IR -> `clang` -> native executable linked with the C runtime.
- `wasm32`: same Osprey LLVM IR -> `clang --target=wasm32-wasip1` ->
  `wasm-ld` -> WASI module linked with a wasm-portable C runtime subset.

The WebAssembly target is instructive. It reused the existing LLVM pipeline and
only needed target selection, a wasm runtime archive, and link-driver changes.
JavaScript and IL cannot reuse the current textual LLVM output the same way.
They need new emitters and a non-C runtime surface.

The relevant existing semantic status:

- Effects: declarations, `perform`, `handle ... in`, source annotations, and
  operation type checking work today. Missing handlers and undeclared rows are
  not yet rejected in every case; runtime lookup guards a missing handler.
  Handler arms without explicit `resume` are lowered as normal functions.
- Explicit `resume`: native code generation implements single-shot, deep,
  thread-as-continuation semantics. The Wasm runtime excludes that pthread path.
- Fibers: `spawn`, `await`, `yield`, and channels exist. The current runtime uses
  pthread-backed fibers in concurrent mode, with deterministic sequential
  execution in test mode. This is closer to "thread-backed task" than a green
  stack-copying fiber.
- The wasm target deliberately excludes fibers, HTTP/WebSocket, FFI, random, and
  some system calls because the current runtime uses pthreads, sockets, OpenSSL,
  `dlopen`, and host syscalls.

## Research Baseline

The external research and platform docs point in the same direction:

- OCaml 5 effect handlers show the ideal direct-style model: `perform` captures a
  delimited continuation; handlers can `continue`; and user-level threads,
  coroutines, generators, and async I/O can be expressed with handlers. OCaml's
  runtime implements this with runtime-managed stack segments/fibers, not with
  JavaScript or CLR stack capture.
- Koka is closer to Osprey's type story: it has effect types and handlers, tracks
  effects in function types, and compiles advanced control abstractions through
  compiler/runtime support.
- JavaScript has a run-to-completion event-loop model per agent. Generators can
  suspend their own execution context, and `async`/`await` suspends at `await`,
  returning promises. There is no standard API to capture an arbitrary caller
  stack as a continuation.
- Node has `AsyncLocalStorage`, which can propagate dynamic context through
  callbacks and promise chains. Browser JavaScript does not have a standardized
  equivalent yet; TC39's Async Context proposal is Stage 2.
- .NET has Tasks, async/await, async state machines, `ExecutionContext`,
  `AsyncLocal<T>`, the managed thread pool, and `System.Threading.Channels`.
  These are strong building blocks for fibers/channels and dynamic handler
  context, but they still do not expose arbitrary stack capture in IL.
- ECMA-335 defines the Common Language Infrastructure and CIL instruction set.
  Targeting IL is viable as a compiler backend, and Microsoft exposes
  `System.Reflection.Emit.ILGenerator` for emitting method bodies, but generating
  verifiable async/state-machine IL is still real compiler work.

Sources are listed at the end.

## JavaScript Target

### Difficulty

MVP without explicit `resume`: **medium-high**.

Full direct-style effects/fibers: **high**.

Rough sizing after a backend split:

| Scope | Estimate | Fidelity |
| --- | ---: | --- |
| Core expression/function/ADT backend | 4-8 weeks | Good |
| Tail-resume effects | 1-2 weeks | Good |
| Basic cooperative fibers via promises/generators | 2-4 weeks | Usable, not parallel |
| Browser-compatible channels/select | 2-4 weeks | Async, not blocking |
| Explicit single-shot `resume` via state machines/CPS | 6-12+ weeks | Good if compiler-owned |
| True CPU-parallel fibers in browser JS | 4-8+ weeks extra | Worker-based and heavier |

### What Maps Cleanly

The portable core maps well:

- Osprey `int` can use `BigInt` for full `i64` fidelity. Using JS `number` would
  be faster but lossy above 53 bits.
- Closures map to JS closures or explicit closure cells.
- ADTs and records map to tagged objects.
- Lists/maps can start as JS arrays/maps or persistent structures, depending on
  how strictly the target wants to preserve Osprey allocation and structural
  sharing behavior.
- Tail-resume handlers can be a dynamic handler stack:
  `perform` looks up `(effect, operation)` in a context and calls the handler
  function with the operation args.

For Node, handler context can ride on `AsyncLocalStorage`. For browser JS, pass
an explicit runtime context through compiled functions until Async Context is
standard enough to rely on.

### Where It Gets Hard

JavaScript generators are useful but not enough on their own. A generator only
suspends the generator frame. Osprey `perform` can appear in ordinary functions
several calls below the handler. To resume the whole delimited computation,
every frame between the handler and performer must be represented in resumable
form. That implies one of:

- compile all effectful code to generators and use `yield*` through every
  effectful call;
- compile effectful/resumable code to explicit state machines;
- compile effectful/resumable code to CPS/trampolines.

The first option is simplest to prototype, but it creates "function coloring":
effectful functions have a different calling convention from pure functions.
The type checker already knows effect rows, so this is manageable, but it must
be designed deliberately.

`async`/`await` is also not a complete answer. It can suspend at `await`, but
`perform` is not necessarily an async host operation. Turning every effectful
function into `async` makes all callers promise-returning and still does not
provide a general non-tail `resume` unless the compiler rewrites the code around
`perform`.

### Fibers In JS

A JS target can emulate Osprey fibers in two tiers:

- **Cooperative fibers:** represent each fiber as a promise-backed task or
  generator/state-machine scheduled by the Osprey runtime. `yield` queues the
  continuation. Channels are async queues. This is portable and reasonably
  lightweight, but there is no CPU parallelism in one JS agent.
- **Worker-backed fibers:** use Web Workers or Node `worker_threads`. This gives
  real parallelism but is much heavier, requires message passing or
  `SharedArrayBuffer`/`Atomics`, and does not share the heap or handler stack
  naturally. It fits isolated Osprey fibers, but every spawn becomes a
  serialization and module-loading problem.

Browser workers copy or transfer data by message; they cannot directly touch the
DOM and have separate global contexts. That aligns with Osprey's stated
fiber-isolation direction, but not with cheap in-process fibers.

### Lossiness Verdict For JS

Not too lossy for the portable core, tail-resume effects, and cooperative
concurrency.

Too lossy if the target tries to map Osprey `resume` or blocking fibers directly
onto ordinary JS calls/promises without a compiler-owned resumable lowering.

Multi-shot continuations should remain out of scope for JS unless Osprey lowers
continuations to cloneable heap state. Native JS generator/async frames are
single logical executions, not cloneable delimited stacks.

## .NET IL Target

### Difficulty

MVP without explicit `resume`: **high but lower semantic risk than JS**.

Full direct-style effects/fibers: **high**.

Rough sizing after a backend split:

| Scope | Estimate | Fidelity |
| --- | ---: | --- |
| CIL assembly emitter and runtime package | 6-10 weeks | Good |
| Core expression/function/ADT backend | 6-10 weeks | Good |
| Tail-resume effects with `AsyncLocal`/explicit context | 1-2 weeks | Good |
| Task/Channel-backed fibers | 2-4 weeks | Good, not identical to pthreads |
| Explicit single-shot `resume` via state machines/CPS | 6-12+ weeks | Good if compiler-owned |
| PDB/debug/source mapping | 3-6+ weeks | Workable |

### What Maps Cleanly

.NET is a strong target for most of Osprey:

- `int` maps to `System.Int64`.
- `string` maps to `System.String`.
- Records and ADTs can be emitted as classes/structs with tags, or as a compact
  generic runtime representation for a faster MVP.
- Closures map to generated classes/delegates or explicit closure structs.
- `Result<T, E>` can be a generic struct/class.
- Garbage collection is provided by the CLR, so Osprey's current default
  allocate-until-exit runtime model can be improved rather than duplicated.
- Fibers can initially map to `Task<T>` or `ValueTask<T>`.
- Channels can map to `System.Threading.Channels`.
- Dynamic handler context can use `AsyncLocal<T>` and `ExecutionContext`, or an
  explicit runtime context parameter if we want target parity with JS.

### Where It Gets Hard

Generating IL is not conceptually hard for straight-line code, but a production
target needs metadata, assemblies, method bodies, exception regions, generics or
generic-like runtime representations, and tooling integration. ECMA-335 and
Reflection.Emit make this possible; they do not remove the compiler work.

The same continuation problem remains: .NET async and iterator methods are
implemented as state machines, and C# compiler-emitted IL includes a stub plus a
state-machine type. Osprey can emit similar IL, but there is no portable CIL
instruction that says "capture the current stack up to this handler and resume
it later."

Possible strategies:

- **Use Tasks for fibers and async host effects.** This is the pragmatic MVP.
  `spawn` returns an Osprey `Fiber<T>` wrapping `Task<T>`, `await` awaits or
  blocks on it, and channels use `System.Threading.Channels`.
- **Emit Osprey state machines for effectful functions.** This gives faithful
  `resume`, `yield`, and nonblocking channels, but it is essentially writing an
  async/iterator compiler.
- **Use thread-as-continuation.** This matches Osprey's planned native
  `resume` design, but on .NET it is heavy if every resuming handler blocks a
  thread. It must use dedicated `Thread`s or very carefully avoid starving the
  thread pool. It is acceptable as a prototype, not as the end state.

### Fibers In .NET

.NET is better than browser JS for Osprey fibers because `Task`, the thread
pool, `ExecutionContext`, and channels are first-class runtime facilities.

There are still choices:

- **Task-backed fibers:** easiest and idiomatic. Good for I/O and moderate
  concurrency. Not a one-to-one match for a custom cooperative scheduler.
- **Dedicated-thread fibers:** closer to Osprey's current pthread runtime, but
  not lightweight.
- **Compiler state-machine fibers:** best long-term model for lightweight
  Osprey fibers. More compiler work, but it composes with effect `resume`.

### Lossiness Verdict For .NET

Not too lossy for a practical backend. .NET can preserve more of Osprey's
library/runtime shape than JavaScript, especially channels and task scheduling.

It is still lossy for explicit `resume` if implemented only with Tasks or
ordinary method calls. A faithful implementation needs compiler-emitted
state machines/CPS or a heavy thread-as-continuation bridge.

## Algebraic Effects: What Can Be Preserved?

### Effect Checking Boundary

Operation signature checking is target-independent, but full effect coverage is
not implemented yet. JS and IL targets should share the same eventual effect-row
propagation and missing-handler checks rather than relying on host-language
effect typing.

### Tail-Resume Handlers

Tail-resume handlers are easy on both targets:

```text
handle E
  op x => expr
in body
```

can lower to:

1. create a handler frame mapping `E.op` to a function;
2. evaluate `body` with that frame installed;
3. `perform E.op(v)` looks up the function and calls it;
4. the returned value becomes the result of the `perform`.

This is how current Osprey codegen works conceptually. JS and .NET can preserve
it with a runtime handler stack/context.

### Explicit `resume`

Explicit `resume` is the real dividing line:

```text
op x => {
  before()
  let answer = resume(x + 1)
  after(answer)
  answer
}
```

The handler must suspend the performer at `perform`, run handler code, resume
the performer, then continue handler code after the resumed computation yields
or returns. That requires a delimited continuation.

Faithful options:

- **CPS:** transform effectful code so continuations are explicit function
  values. Powerful, portable, and cloneable if designed that way, but invasive.
- **State machines:** represent each effectful function as a resumable object
  with program counters and lifted locals. Good fit for JS and .NET because both
  ecosystems already use state-machine-shaped async/iterator machinery.
- **Generators:** practical subset of state machines for JS, but they require
  every effectful call chain to use generator calling conventions.
- **Thread-as-continuation:** matches the native plan for single-shot resume,
  but too heavy as the primary JS/.NET design. JS cannot block a browser agent
  this way; .NET can, but it spends real threads.

Current Osprey already rejects multi-shot resume as a follow-up. Keep that
restriction for JS and .NET. Multi-shot resume becomes feasible only if
continuations are heap data that can be cloned; it is not feasible by reusing
host stacks, JS generators, or .NET Tasks.

## Backend Architecture Recommendation

Do not bolt JS and IL emitters directly onto the existing AST walk. Split the
backend first:

```text
Parsed AST
  -> typed program/effect rows
  -> target-neutral lowered IR
       - explicit closure conversion
       - normalized blocks/control flow
       - ADT/record layout decisions
       - effect operation table
       - fiber/channel operations
       - resumable-region annotation
  -> target emitters
       - LLVM text emitter
       - JavaScript emitter
       - .NET IL emitter
```

The lowered IR should expose:

- pure functions versus effectful/resumable functions;
- call convention per function;
- value representation choices;
- handler push/pop/lookup operations;
- suspension points: `perform`, explicit `resume`, `yield`, `await`, `send`,
  `recv`, and eventually `select`;
- runtime imports required by each target.

This keeps the existing LLVM backend from becoming the source of truth for
semantics, and it prevents JavaScript and IL from each inventing separate
versions of effect/fiber lowering.

## Suggested Milestones

### Phase 0: Decision Record

- Define whether the JS target is Node-only first, or browser-compatible too.
- Define whether the .NET target emits an IL assembly, C# source, or both.
- Define whether JS `int` can use `number`, or must use `BigInt` to preserve
  full i64 behavior.
- Decide whether `resume` is out of MVP.
- Decide whether fibers are cooperative tasks in MVP.

### Phase 1: Backend Split

- Introduce a target-neutral lowered IR.
- Keep LLVM output byte-identical or golden-test equivalent.
- Move handler/fiber ABI decisions into target-neutral lowering where possible.

### Phase 2: JavaScript Core MVP

- Emit ES modules.
- Runtime: values, ADTs, closures, `Result`, lists/maps, effect handler context.
- Support tail-resume effects.
- Support cooperative `spawn`/`await` only if compiled functions are already
  async/generator-shaped; otherwise defer.

### Phase 3: .NET Core MVP

- Emit a .NET assembly or C# source first, then direct IL later if speed matters.
- Runtime package: Osprey value helpers, effect context, fibers, channels.
- Support tail-resume effects and Task-backed fibers.

### Phase 4: Shared Resumable Lowering

- Mark functions that can cross suspension points.
- Lower those functions to CPS/state machines.
- Implement explicit single-shot `resume`.
- Implement real `yield`/blocking channel semantics on top of the same machinery.

## Final Verdict

JavaScript target: worthwhile for reach and embedding, but start with the
portable core. It becomes high-risk only if we demand non-tail `resume` and
blocking fiber/channel semantics before the compiler has a resumable lowering
pass.

.NET IL target: technically cleaner for Osprey's runtime model than JavaScript,
especially for tasks, channels, context propagation, and managed memory. It is
still not a free lunch: direct IL emission plus async/effect state machines is
compiler-scale work.

Algebraic effects and fibers are possible on both targets. They are too lossy
only if mapped naively to host async/generator/task features. They are not too
lossy if Osprey owns the control-flow transform and treats host async features
as scheduling/runtime primitives rather than as the semantics of effects.

## Sources

Local Osprey context:

- [README.md](../../README.md)
- [Algebraic Effects spec](../specs/0017-AlgebraicEffects.md)
- [Fibers and Concurrency spec](../specs/0011-LightweightFibersAndConcurrency.md)
- [WebAssembly target spec](../specs/0022-WebAssemblyTarget.md)
- [Algebraic effects roadmap](../plans/0016-algebraic-effects-and-handlers.md)
- [`crates/osprey-codegen/src/effects.rs`](../../crates/osprey-codegen/src/effects.rs)
- [`crates/osprey-codegen/src/fiber.rs`](../../crates/osprey-codegen/src/fiber.rs)
- [`compiler/runtime/effects_runtime.c`](../../compiler/runtime/effects_runtime.c)
- [`compiler/runtime/fiber_runtime.c`](../../compiler/runtime/fiber_runtime.c)

External references:

- [OCaml manual: Effect handlers](https://ocaml.org/manual/5.5/effects.html)
- [Koka language: effect types and handlers](https://koka-lang.github.io/koka/doc/index.html)
- [MDN: JavaScript execution model](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Execution_model)
- [MDN: async function](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Statements/async_function)
- [MDN: Iterators and generators](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Guide/Iterators_and_generators)
- [MDN: Using Web Workers](https://developer.mozilla.org/en-US/docs/Web/API/Web_Workers_API/Using_web_workers)
- [Node.js: AsyncLocalStorage](https://nodejs.org/api/async_context.html#class-asynclocalstorage)
- [TC39: Async Context proposal](https://github.com/tc39/proposal-async-context)
- [.NET TAP overview](https://learn.microsoft.com/en-us/dotnet/standard/asynchronous-programming-patterns/task-based-asynchronous-pattern-tap)
- [.NET AsyncStateMachineAttribute](https://learn.microsoft.com/en-us/dotnet/api/system.runtime.compilerservices.asyncstatemachineattribute?view=net-10.0)
- [.NET ExecutionContext](https://learn.microsoft.com/en-us/dotnet/api/system.threading.executioncontext?view=net-10.0)
- [.NET AsyncLocal<T>](https://learn.microsoft.com/en-us/dotnet/api/system.threading.asynclocal-1?view=net-10.0)
- [.NET managed thread pool](https://learn.microsoft.com/en-us/dotnet/standard/threading/the-managed-thread-pool)
- [.NET Channels](https://learn.microsoft.com/en-us/dotnet/core/extensions/channels)
- [.NET managed code and IL](https://learn.microsoft.com/en-us/dotnet/standard/managed-code)
- [ECMA-335: Common Language Infrastructure](https://ecma-international.org/publications-and-standards/standards/ecma-335/)
- [.NET ILGenerator](https://learn.microsoft.com/en-us/dotnet/api/system.reflection.emit.ilgenerator?view=net-10.0)
