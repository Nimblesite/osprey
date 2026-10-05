# Writing about Osprey

Osprey is a functional language built around algebraic effects, inferred types, and native compilation. This document guides the README, website, examples, and contributor documentation.

## Lead with effects

Show a small program that requests an operation and two handlers that provide different implementations. Explain the practical use: database access, logging, platform services, and deterministic tests without passing service objects through every helper.

Use this feature order on introductory pages:

1. **Algebraic effects:** declare an operation, request it with `perform`, and choose its implementation with a callable `handler` value or a block-scoped `handle`.
2. **Inferred types and explicit data:** pattern matching covers every case; `Result` keeps expected failures visible.
3. **Two syntax flavors:** Default braces and ML layout share the compiler and can coexist in a project.
4. **Deployment:** LLVM native binaries, WebAssembly, and C ABI libraries for iOS and Android.
5. **Concurrency and memory choices:** isolated fibers, tracing GC, and reference counting where the target supports them.

Not every page needs the full list. Give the reader code, its result, and a next step; avoid repeating the same feature summary across sections.

## Describe the current effects implementation

- `handler E { ... }` creates a callable value. `h(work)` installs it while calling the zero-argument function `work`; creating the handler runs no work. Passing the function delays execution until its handler is active.
- `handle E { ... }` installs a handler for the remaining statements and final expression in its block. The removed `handle ... in/do ...` forms are invalid.
- A value operation returns a result to its caller. A declared `control` operation can use `resume` or answer without continuing. Operation declarations determine the mode, never a search for `resume` in handler arms.
- Arms execute outside their own activation, so the same operation can forward to an outer handler. Resumption restores the handled scope.
- Static handler selection removes runtime effect dispatch. Ordinary calls, allocations, and runtime values may remain; do not call it “no runtime cost.”
- The compiler checks operation types and required handlers. The open-row callback prototype is not independently quantified effect rows in function types.

The [effects specification](specs/0017-AlgebraicEffects.md) defines the contract. [Plan 0016](plans/0016-algebraic-effects-and-handlers.md) owns delivery status and limitations. [Runnable handler examples](../examples/handlers/README.md) demonstrate current behavior in both flavors and compare it with other languages. Keep intended semantics and demonstrated support distinct.

Use the [bank](../examples/projects/modules/README.md) and [mobile apps](../examples/mobile/README.md) to explain application boundaries. Mobile handlers describe SQL and HTTP commands; native hosts execute them and send completion events. This does not imply that mobile continuations or the planned staged reactive runtime are implemented.

## Keep the copy concrete

- Lead with what the code does. Explain a specialist term when first used.
- Prefer runnable examples and links to their source over slogans and repeated feature lists.
- Keep limitations beside the claims they qualify. Do not describe a specified feature as shipped without compiler evidence.
- Explain a callback's purpose when introducing it: the handler must be installed before the callback runs.
- Present both flavors as ways to write Osprey. ML currying and Default flat calls differ; do not promise identical code generation for unrelated programs.
- Compare Koka, OCaml, Eff, and Effekt through specific semantics or executed examples, without claiming a winner.
- Keep compiler implementation details in reference material. Do not make the introduction a delivery log or a list of old defects.
- Avoid unmeasured performance claims, “zero-cost,” “complete,” and claims that effects replace every framework.
- Maintain a single source of truth for each contract; link to it instead of copying a support matrix everywhere.

## Qualifications to preserve

- Osprey is alpha software. Native dynamic resumption is deep and single-shot; reusable continuations, owned escaping resumptions, and full scoped row polymorphism remain delivery work in plan 0016.
- WebAssembly and mobile C ABI targets support value effects and static discharge; unsupported dynamic control operations are rejected before linking.
- Integer arithmetic uses explicit `Arith` policies. Wrapping/saturating helpers name their behavior; `checkedAdd`, `checkedSub`, and `checkedMul` return `Result`. See [arithmetic effects](specs/0037-ArithmeticEffects.md).
- Native memory modes are default, GC, and ARC. The default allocator retains general allocations. Strict static-memory checking and tail-call optimization are not implemented.
- Mobile C ABI targets use the default allocator and do not expose a stable returned-string release API. Platform networking belongs to the native host.
- Project modules and cross-flavor imports work; the package manager remains roadmap work.
- GPU kernels currently execute as host loops. Device code generation remains roadmap work in [plan 0023](plans/0023-gpu-computation.md).
- Calling C crosses Osprey's memory-safety boundary.

Update claims when the implementation and its verification change. A feature appearing in a specification does not by itself establish support.
