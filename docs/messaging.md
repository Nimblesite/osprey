# Writing about Osprey

**Effects first. Real code. Visible results. One next step.** This is the brief for the homepage, videos, README, examples, and documentation. Pair it with the [Osprey design system](../website/design_system.md) before writing or rendering.

Osprey is a functional language with algebraic effects, inferred types, and native compilation. Lead with what a program does: it requests storage; one handler writes a file; another simulates being offline. The function stays the same. Show the output before explaining the machinery.

## The three pillars

Osprey stands on three pillars: **safety, elegance and performance**. They are the point of the language. Every feature is evidence for one of them, and every page, video, README and example states them before the feature inventory. Say all three together on first mention, in this order.

- **Safety: fewer bugs and failures.** Osprey is algebraic and has no panics. Expected failures are `Result` values the caller must handle; data models list every case and matching is exhaustive; integer arithmetic follows an explicit `Arith` policy instead of silently wrapping; effects make outside work visible and replaceable. Keep the qualifications below (device-kernel faults, the C boundary) beside these claims.
- **Elegance: more program for less text.** Inferred types, effects instead of plumbing, and direct concurrent code without a separate async form. Less text means fewer tokens: Osprey programs are cheaper for AI agents to read, write and review, and easier for people to hold in their heads. Say this plainly; it is a measurable consequence, not a slogan.
- **Performance: native from the start.** LLVM native binaries with no VM or JIT warm-up, static handler selection that removes runtime effect dispatch, lightweight fibers, and a memory backend chosen per build. Performance is always critical for a language; state it as a design commitment and attach any relative figure to a published benchmark.

Short form: **Safe, elegant, fast.** Longer form: **Osprey is a functional language built for safety, elegance and performance: algebraic effects and `Result` instead of panics, inferred types that say more with less text, and native LLVM compilation.** Removing the pillars from the homepage, README or an introduction is a regression, not a simplification.

## Message order

1. **Effects:** declare a need with `effect`, request it with `perform`, choose its implementation with a callable `handler` or block-scoped `handle`. Explain the benefit through storage, logging, platform services, or deterministic tests.
2. **Types:** inference removes repeated annotations; exhaustive matching and `Result` keep cases and failures visible.
3. **Three syntax flavors. One language:** A syntax flavor is a way to write Osprey: Default braces, ML indentation or Python-style syntax. Say **syntax flavor** on first mention and define it; do not assume “flavor” explains itself. Each keeps its idioms while sharing Osprey’s types, effects and compiler. Choose per file and import across flavors in one project. Show all three in introductory code controls; Python flavor belongs on the homepage; the initial selection is Default (.osp).
4. **Deployment:** LLVM native programs, WebAssembly, and C ABI libraries for native mobile hosts.
5. **Concurrency and memory:** isolated fibers, tracing GC, and reference counting where supported.

A short introduction needs only the first point, proof, and a next step. Headlines carry the idea; supporting text earns its place by explaining the example. A video demonstrates one change, with narration describing the code or result currently visible. Do not read a feature inventory over decorative footage.

## Python to GPU

Invite Python users to port computation into Osprey’s Python flavor: keep familiar `def`, `lambda`, and f-strings; gain inferred static types and checked effects. Show the GPT-2 port and its real source. One `Tensor` effect has CPU and GPU handlers; the handler decides where the arithmetic runs. Metal and OpenCL execute the GPU operations. Retain Python tooling and libraries through explicit interop.

Do not imply an arbitrary Python program or package runs unchanged, that a marker comment ports a program, or that the compiler automatically offloads it. Python flavor is Osprey, shared tensor storage over the Python bridge is planned, and acceleration requires measurements. Link to the porting example and place those limits beside the claim.

## Wording and proof

- Connect each feature to one of the three pillars (safety, elegance, performance). A feature that connects to none of them does not belong in introductory copy.
- Prefer concrete verbs and runnable examples over slogans, repeated summaries, or compiler delivery logs. Keep limitations beside the claims they qualify.
- A callable handler runs the zero-argument function passed to it; creating the handler runs no work. Explain that the callback delays execution until the handler is active. `handle` covers the rest of its block; the removed `in/do` forms are invalid.
- Value operations return to their callers. Declared `control` operations may resume or answer directly. Arms run outside their own activation and can forward outward; resumption restores the handled scope.
- Static handler selection removes runtime effect dispatch, not every call or allocation. Never say “zero-cost.” Operation types and required handlers are checked; open-row prototypes are not independently quantified effect rows in function types.
- Say “Python flavor” for Osprey and start its samples with `# osprey: flavor=python`. Reserve “Python” for CPython programs. ML curried calls and Default flat calls differ; do not promise identical code generation for unrelated programs.
- Compare other languages through specific semantics and executed examples. Avoid claims of completeness, unmeasured performance, or replacing every framework.

The [effects specification](specs/language/0017-AlgebraicEffects.md) defines semantics; [plan 0016](plans/0016-algebraic-effects-and-handlers.md) records limits; [handler examples](../examples/handlers/README.md) demonstrate support. The [bank](../examples/projects/modules/README.md) and [mobile apps](../examples/mobile/README.md) demonstrate host boundaries. A specification alone is not proof of implementation.

## Maturity and invitation

The maturity labels "alpha" and "experimental" are banned in Osprey messaging. Do not use either to describe the language or imply production readiness. Use **actively evolving**: Osprey is being used to build real applications, including the macOS storage app and Vigil, while language and tooling changes still require validation.

Preferred copy: **Osprey is actively evolving. Try it on your projects, validate it against your workload, and assess the risks before relying on it in production.** Keep concrete feature limits beside the relevant claim. Invite people to build and report failures without promising stability or support that has not been demonstrated.

## Qualifications to preserve

- Osprey is actively evolving. Native dynamic resumption is deep and single-shot; reusable continuations, owned escaping resumptions, and full scoped row polymorphism remain delivery work in plan 0016.
- WebAssembly and mobile C ABI targets support value effects and static discharge; unsupported dynamic control operations are rejected before linking.
- Integer arithmetic is checked against explicit `Arith` policies. The host runtime invokes them; device-kernel faults currently abort instead of calling a recovering policy. Qualify no-trap claims with this delivery limit. Total wrapping/saturating helpers name their behavior; `checkedAdd`, `checkedSub`, and `checkedMul` return `Result`. See [arithmetic effects](specs/language/0037-ArithmeticEffects.md).
- Explain the GC default through its concrete tradeoff: ordinary retain/release calls are no-ops, avoiding ARC reference-count updates at ownership changes, while accepting delayed reclamation and collection pauses. Both modes are automatic; do not imply GC is universally faster or ARC requires ownership annotations.
- Native builds default to tracing GC; ARC is the alternative managed backend. Unmanaged allocation requires explicit `--memory=none` and retains general allocations. The old `--memory=default` spelling is rejected. Windows native users must select ARC because tracing GC is outside the supported Windows contract. Strict static-memory checking and tail-call optimization are not implemented.
- Mobile C ABI targets require explicit `--memory=none` and do not expose a stable returned-string release API. Platform networking belongs to the native host.
- Project modules and cross-flavor imports work. The package manager is in progress under `packages/manager/` (plan 0020): the folder registry, folder publication, dependency resolution, verified fetching and locked builds run in Osprey. The dedicated hosted package website, authenticated public publishing and trust services remain planned. See the [package manager guide](../website/src/docs/package-manager.md).
- GPU is vendor-neutral; optional CUDA interop is separate. [GPT-2](../examples/gpt2/README.md) loads published weights; one static `Tensor` effect has host-list and GPU handlers. The handler decides where arithmetic runs. GPU operations execute on Metal or OpenCL with software binary64; buffers stay device-resident. Tokenization, loading, and readback remain on the host; kernels are not fused. Device transcendental results can differ from host libm in the last digit; sampled tests do not prove universal correct rounding. Never claim automatic offloading, universal acceleration, or bit-for-bit transcendental parity. See the [GPU contract](specs/runtime/0034-GPUComputation.md).
- Calling C crosses Osprey's memory-safety boundary. Package source scanning marks native `extern` declarations as unclassified, so source-bundle validation and local folder publication reject them. This publication rule does not establish complete runtime confinement of untrusted packages; see the [package guide](../website/src/docs/package-manager.md#supply-chain-controls-and-remaining-work).

Update claims when the implementation and its verification change. A feature appearing in a specification does not by itself establish support.
