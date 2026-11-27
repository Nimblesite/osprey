# Writing about Osprey

This brief governs every word about Osprey: the homepage, README, videos, docs, talks and posts. Pair it with the [design system](../../src/toolchain/website/design_system.md).

## The answer

> **Osprey is uncompromising: safe, elegant and fast, with no trade between them. It already runs real apps on Mac, Windows, iPhone and Android, web servers, and AI models on the GPU.**

Second sentence, when there is room:

> **Write the logic once and the compiler checks all of it. Handlers decide where it runs: SwiftUI or WinUI, CPU or GPU, the real disk or a test.**

Every longer pitch expands these two sentences and never replaces them. The products are proof, not a boundary: never present them as the full list of what Osprey is for, and name only products that run today. Lead with things everyone knows (apps, websites, AI) in everyday words; specialist work such as the DAW, audio synthesis and proofs follows, never first. Push **uncompromising**, not breadth. "General purpose", "build anything" and "one language for everything" sound watered down; never lead with them. The breadth is evidence that nothing was traded away, not the pitch.

## Uncompromising

Osprey's design defers trade-offs to you instead of making them for you. Each choice has an option that goes all the way, and you can pick another:

- **Elegance:** the ML flavor is uncompromising on elegance. Prefer braces? Choose the Default flavor; it is the same language.
- **Deterministic performance:** ARC frees memory at known points, with no collector pauses. Prefer throughput? Choose tracing GC.
- **Interop:** always uncompromising. C, Swift, Python and the platform UI toolkits are called directly, with no choice to make.

Osprey never trades one pillar for another: no safety bought with speed, no speed bought with boilerplate. Each domain gets a tool that competes with the best on its own terms. These are the domains proven so far, each by a product that runs today; add a row when a new one ships:

| You build | With Osprey | Show |
| --- | --- | --- |
| Desktop and mobile apps | Osprey Native: one app, drawn with SwiftUI, WinUI and Android widgets or on a GPU canvas | Issue Inbox, Storage, Tally |
| Backends | HTTP and WebSocket servers, SQL and storage behind handlers, fibers | Talon Bank |
| AI and ML | Python-flavor ports of `torch`, `transformers` and `numpy`; GPU kernels on Metal or OpenCL | GPT-2 from published weights |
| Audio | `@osprey/synth`: modular, FM, modal, physical and granular voices, buses, side-chain compression | Prism DAW, Light Orchestra |
| Maths and proof | Formulas written in Lean, compiled to Osprey; an independent proof kernel | Proof Workbench, Mathlib port |
| Web | WebAssembly, the browser playground, shared web clients | Playground, WebAssembly studio |
| Tooling | Compiler, language server, debugger, profiler, formatter, package manager, KLEE bug hunting | VS Code extension |

Name Windows whenever you list desktop platforms, and say which hosts run which app.

The domains hold together because their boundaries are **effects**: typed, checked by the compiler, and swappable without touching the code that uses them. Effects are the reason, not the headline.

## Safe, elegant, fast

Say all three together, in this order, on first mention. Dropping them from the homepage, README or an introduction is a regression. Tie every feature to one of them; a feature that fits none stays out of introductory copy.

- **Safe: no part can crash another.** No panics. Failures are `Result` values the caller must handle, matching is exhaustive, integer arithmetic follows an explicit `Arith` policy, and effects make outside work visible.
- **Elegant: more program, less text.** Inferred types, effects instead of plumbing, concurrency without a separate async form. Less text is cheaper for AI agents and easier for people.
- **Fast: native from the start.** LLVM native code with no VM or JIT warm-up, static handler selection, lightweight fibers, GPU kernels when a handler asks for them. Every relative figure links a published benchmark.

## Osprey Native

**Osprey Native (`@osprey/native`) is a headline feature.** One Osprey app (state, logic and UI) compiles ahead of time to native code. Each host draws it with the platform's own widgets (SwiftUI, WinUI, Android) or on a GPU canvas (GPUI). Say the difference concretely: Flutter and Compose Multiplatform draw their own copies of the widgets; React Native runs JavaScript in an engine. Show the real apps, and name which hosts run which app.

## Show one move: same logic, different handler

- **Apps:** Issue Inbox's state and screens run on iOS and Android; only the host differs.
- **AI:** GPT-2 is written once against a `Tensor` effect. One handler runs it on the CPU, another on the GPU.
- **Backends:** an install writes to the real disk; a test swaps in a full disk and checks the rollback.
- **Audio:** an effect's gain law is a Lean formula; Prism and its offline renderer call the same generated code.

Short on space? Keep the one example that fits the audience.

## By audience

- **App developers:** "Write your app once and ship it with each platform's own widgets. No JavaScript engine, no toolkit drawing copies of the widgets."
- **Python and ML:** "Keep `def`, `lambda` and f-strings; gain static types and native GPU kernels. GPT-2 runs from the published weights."
- **Backend:** "No panics, no hidden I/O. Every failure is a `Result`; every outside call is an effect you swap in tests."
- **Audio:** "Patch, mix and render in one language, with the DSP maths written as formulas. No collector pauses under ARC."
- **Mathematicians:** "Write the formula in Lean and run it as compiled Osprey."
- **Skeptics:** "It is actively evolving. Here is a real app and its test suite; try it on your workload." Then link the [limits](#qualifications-to-preserve).

## Order and form

1. **What it's for**, with one real example.
2. **Effects:** `effect` declares a need, `perform` requests it, a `handler` or `handle` block provides it. Show storage, logging, platform services or tests.
3. **Types:** inference removes annotations; exhaustive matching and `Result` keep cases and failures visible.
4. **Syntax flavors:** one language written four ways: Default (`.osp`, braces), ML (`.ospml`, indentation), Python and TypeScript. Say **syntax flavor** and define it on first mention; Default is selected first.
5. **Platforms:** Osprey Native, WebAssembly, C ABI libraries for mobile hosts.

A short intro needs point 1, proof and one next step: the playground, an example or a guide. Homepage section labels are words, never numbers. A video shows one change and narrates what is on screen.

## Wording

- Concrete verbs and runnable examples, not slogans, repeated summaries or compiler delivery logs. Keep each limit beside the claim it qualifies.
- Say "Python flavor" for Osprey, and start its samples with `# osprey: flavor=python`. "Python" means CPython. Never imply an arbitrary Python or TypeScript program or package runs unchanged, that a marker comment ports a program, or that the compiler offloads it automatically.
- A callable handler runs the zero-argument function passed to it; creating the handler runs nothing. `handle` covers the rest of its block; the old `in`/`do` forms are invalid.
- Value operations return to their callers. Declared `control` operations may resume or answer directly; resumption restores the handled scope.
- Static handler selection removes runtime effect dispatch, not every call or allocation. Never say "zero-cost".
- Compare other languages through specific semantics and executed examples. No claims of completeness, unmeasured speed, or replacing Swift, Python or Rust: Osprey hosts and interoperates with platform code.
- A specification is not proof of implementation. The [effects spec](../specs/language/0017-AlgebraicEffects.md) defines semantics, [plan 0016](../plans/0016-algebraic-effects-and-handlers.md) records limits, and the [handler examples](../../src/osprey/examples/handlers/README.md), [bank](../../src/osprey/examples/talon/README.md) and [mobile apps](../../src/osprey/examples/mobile/README.md) demonstrate them.

## Banned

- "Logic, in flight", in any form.
- "Alpha" and "experimental". Say **actively evolving**: **Osprey is actively evolving. Try it on your projects, validate it against your workload, and assess the risks before relying on it in production.**
- "A new programming language" as the first line.
- "Does everything" or "runs any Python program". Name the real products.
- Effects jargon before the reader knows what Osprey is for.

## Qualifications to preserve

- **Effects:** multi-shot (`many`) continuations run on every target, WebAssembly included, with identical output. This is an invariant ([MULTI-WASM-MANY](../specs/language/0017-AlgebraicEffects.md)): never write that WebAssembly lacks them. A `once` continuation outside a `many` region is deep, single-shot and native only; WebAssembly and mobile C ABI targets reject that one capability before linking, and support value effects, static discharge and `many` regions. Owned escaping resumptions and full scoped row polymorphism are plan 0016 work.
- **Arithmetic:** integer arithmetic follows explicit `Arith` policies called by the host runtime; device-kernel faults abort instead of calling a recovering policy, so qualify no-trap claims. Formulas generated from Lean keep IEEE float division. `checkedAdd`, `checkedSub` and `checkedMul` return `Result`. See [arithmetic effects](../specs/language/0037-ArithmeticEffects.md).
- **Memory:** native builds default to tracing GC; ARC is the alternative, and Windows native users must select ARC. Explain the GC tradeoff concretely: no reference-count updates, but delayed reclamation and collection pauses. Neither needs ownership annotations, and neither is universally faster. Compile-time ownership rejects transfers the compiler cannot prove. Tail-call optimization remains planned.
- **Mobile ABI:** mobile C ABI targets default to ARC. [Host ABI version 1](../specs/compiler/iOSCodegen.md#calling-the-host-ios-host-imports) copies host-import strings into Osprey memory; hosts copy exported text, then call `osprey_release_string` exactly once and never libc `free`. Platform networking belongs to the native host.
- **Packages:** the folder registry, publication, resolution, verified fetching and locked builds run today (plan 0020). The hosted package site, authenticated public publishing and trust services are planned. See the [package guide](../../src/toolchain/website/src/docs/package-manager.md).
- **GPU:** vendor-neutral Metal and OpenCL with software binary64; CUDA interop is separate. Buffers stay on the device; tokenization, loading and readback stay on the host; kernels are not fused. Device transcendentals can differ from host libm in the last digit. Never claim automatic offloading, universal acceleration or bit-for-bit transcendental parity. Shared tensor storage over the Python bridge is planned. See the [GPU contract](../specs/runtime/0034-GPUComputation.md).
- **Audio:** Prism renders offline and then plays; there is no real-time audio callback yet. VST3 hosting is incomplete. The full DAW runs on macOS; Windows runs the patch editor.
- **C interop:** calling C crosses Osprey's memory-safety boundary. Package publication rejects native `extern` declarations, which does not confine untrusted packages at runtime; see [supply-chain controls](../../src/toolchain/website/src/docs/package-manager.md#supply-chain-controls-and-remaining-work).

Update a claim when its implementation and verification change.

## Proof development and the Lean oracle

Lean is Osprey's development oracle for the first compatible discipline: pinned statements, proof terms, axiom reports and reference behaviour check Osprey while it builds its own kernel, frontend, automation and libraries. The relationship is cooperative. Never position Osprey as competing with Lean, and never treat benchmark ordering as a goal; performance checks stay as engineering gates.

Lean accepting a certificate is reference evidence, never permission for Osprey to accept an unsound proof. Separate a kernel bug from inconsistent assumptions or a change to the theory. The [wrong-structure projection exploit in Lean issue 14576](https://github.com/leanprover/lean4/issues/14576) is an implementation bug, kept as a rejection regression; it does not show Lean's mathematics is inconsistent. Investigate every oracle disagreement, and never copy the oracle's bug.

The compiler owns only the discipline protocol. Each theory is a versioned package that can be fixed, replaced or forked without touching Hindley-Milner rules. Changing a checker, rule or axiom policy invalidates the affected proof receipts and requires fresh checking. A fork inherits no validity from its parent, and pluggability alone proves no consistency.

Independence means Osprey authors, checks and replays the supported proofs, builds its maths library and compiles verified programs without Lean installed. Lean stays as an external oracle afterwards. Track it through the [completion milestones](../plans/0041-dependent-types-and-mathlib.md#completion-milestones-proof-completion), reporting imported certificates, source translation and locally reconstructed proofs separately.
