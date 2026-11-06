# Plan 0054 — JavaScript and .NET targets

**Status: not accepted.** Feasibility work only; nothing here is scheduled. The answer so far: both targets are possible and neither is too lossy, provided Osprey owns the control-flow transform and treats host async features as scheduling primitives, not as the meaning of effects.

Neither is a linker change like wasm32, which reuses the LLVM pipeline. Both need a new emitter and a non-C runtime.

## The dividing line

Value operations, closures, ADTs, records, pattern matching, strings, lists, maps and `Result` lower without semantic loss on both: a value operation is a lookup in a handler stack and a call.

Declared control operations, explicit `resume` and direct-style fibers are the hard part. Neither platform can capture an arbitrary live stack segment, so every frame between a handler and its `perform` must be resumable. The options:

| Lowering | Fit |
|---|---|
| CPS | Portable and cloneable, but invasive |
| Compiler-emitted state machines | Best fit: both ecosystems already use this shape for async and iterators |
| Generators (JS) | Simplest prototype, but every effectful call chain needs `yield*`, a second calling convention the effect rows can drive |
| Thread as continuation | Matches native. Impossible in a browser agent, and spends real threads on .NET: a prototype at most |

Mapping `resume` directly onto promises, `Task` or plain calls is the one approach that is too lossy. Multi-shot stays out of scope unless continuations become cloneable heap data; host stacks, generators and `Task` can't be cloned.

## Per target

| | JavaScript | .NET |
|---|---|---|
| MVP difficulty, value operations only | Medium-high | High, with lower semantic risk |
| `int` | `BigInt` for full i64; `number` is faster and lossy above 53 bits | `System.Int64` |
| Handler context | Explicit runtime context; `AsyncLocalStorage` on Node only, as TC39 Async Context is Stage 2 | `AsyncLocal<T>`/`ExecutionContext`, or the same explicit context for parity |
| Fibers | Cooperative promise or generator tasks with no parallelism; workers are parallel but heavy, with no shared heap | `Task<T>` or `ValueTask<T>` first; compiler state machines long term |
| Channels | Async queues, not blocking | `System.Threading.Channels` |
| Memory | Host GC | CLR GC |

Rough sizing after the backend split, in weeks:

| Scope | JavaScript | .NET |
|---|---|---|
| Core backend | 4-8 | 6-10, plus 6-10 for the assembly emitter and runtime package |
| Value-operation effects | 1-2 | 1-2 |
| Fibers | 2-4 | 2-4 |
| Channels and select | 2-4 | With fibers |
| Single-shot `resume` by state machines or CPS | 6-12+ | 6-12+ |
| Worker parallelism (JS), PDB and source mapping (.NET) | 4-8+ | 3-6+ |

## Checklist

- [ ] Decide: Node only, or browsers too.
- [ ] Decide: .NET emits an IL assembly, C# source, or C# first and IL later.
- [ ] Decide: JavaScript `int` is `BigInt` or `number`.
- [ ] Decide: whether `resume` and blocking fibers are out of the MVP.
- [ ] Split the backend: typed program → target-neutral lowered IR → LLVM, JavaScript and IL emitters. The IR makes explicit closure conversion, control flow, ADT layout, the effect operation table, fiber and channel operations, resumable regions, per-function calling convention and each target's runtime imports. Check: LLVM output stays byte-identical or golden-equivalent.
- [ ] JavaScript MVP: ES modules and a runtime for values, ADTs, closures, `Result`, lists, maps and effect context. Check: the portable corpus matches its goldens.
- [ ] .NET MVP: the same surface, with `Task`-backed fibers and channels. Check: the portable corpus matches its goldens.
- [ ] Shared resumable lowering: mark functions that cross suspension points (`perform`, `resume`, `yield`, `await`, `send`, `recv`, `select`), lower them to state machines or CPS, and implement single-shot `resume` and blocking channels on top. Check: control-operation programs give the native answers on both targets.

## References

[OCaml effects](https://ocaml.org/manual/5.5/effects.html) · [Koka](https://koka-lang.github.io/koka/doc/index.html) · [JS execution model](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Execution_model) · [Node AsyncLocalStorage](https://nodejs.org/api/async_context.html#class-asynclocalstorage) · [TC39 Async Context](https://github.com/tc39/proposal-async-context) · [.NET AsyncLocal](https://learn.microsoft.com/en-us/dotnet/api/system.threading.asynclocal-1?view=net-10.0) · [.NET Channels](https://learn.microsoft.com/en-us/dotnet/core/extensions/channels) · [ECMA-335](https://ecma-international.org/publications-and-standards/standards/ecma-335/)
