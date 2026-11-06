# Writing about Osprey

**Real code. Visible results. One next step.** This brief covers the homepage, videos, README, examples and documentation. Pair it with the [Osprey design system](../src/toolchain/website/design_system.md).

## What Osprey is for

Osprey is for **apps, backends, ML and AI, and mathematics**, and it is built to be good at all of them. Say this first, before any feature list, and back each area with a real example: Osprey Native apps on macOS, Windows, Linux, iOS, Android and the web; HTTP backends; language models built on the `@osprey/transformers` port and running on native GPU kernels; machine-checked proofs. Name Windows whenever you list desktop platforms.

## The three pillars

**Safe, elegant, fast.** Say all three together, in this order, on first mention; removing them from the homepage, README or an introduction is a regression.

- **Safety: fewer bugs.** No panics: expected failures are `Result` values the caller must handle, matching is exhaustive, integer arithmetic follows an explicit `Arith` policy, and effects make outside work visible and replaceable. Keep the qualifications below beside these claims.
- **Elegance: more program, less text.** Inferred types, effects instead of plumbing, concurrency without a separate async form. Less text means fewer tokens: cheaper for AI agents and easier for people.
- **Performance: native from the start.** LLVM native binaries with no VM or JIT warm-up, static handler selection, lightweight fibers, a memory backend chosen per build. Attach every relative figure to a published benchmark.

## Osprey Native

**Osprey Native (`@osprey/native`) is a headline feature.** One Osprey app (state, logic and UI) compiles ahead of time to native code, and each host draws it either with the platform's own native widgets (SwiftUI, WinUI, Android) or on a GPU canvas (the GPUI host). Say how this differs, concretely: Flutter and Compose Multiplatform draw their own widgets everywhere instead of the platform's; React Native runs JavaScript in an engine. Osprey Native does neither: native code, native widgets or a canvas, your choice per host. Show the real apps (Issue Inbox, Prism Synth, the casting player) rather than describing them. Keep host coverage honest: name which hosts run which app.

## Message order

1. **What it's for**, with one real example.
2. **Effects:** `effect` declares a need, `perform` requests it, a `handler` or `handle` block implements it. Show the benefit with storage, logging, platform services or tests.
3. **Types:** inference removes annotations; exhaustive matching and `Result` keep cases and failures visible.
4. **Three syntax flavors, one language:** Default braces, ML indentation or Python-style syntax. Say **syntax flavor** and define it on first mention. Show all three; the initial selection is Default (.osp).
5. **Platforms:** Osprey Native apps, WebAssembly, and C ABI libraries for mobile hosts.

A short introduction needs only the first point, proof, and a next step. Homepage section labels are words, never numbers. A video demonstrates one change, narrating what is on screen.

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

The [effects specification](specs/language/0017-AlgebraicEffects.md) defines semantics; [plan 0016](plans/0016-algebraic-effects-and-handlers.md) records limits; [handler examples](../src/osprey/examples/handlers/README.md) demonstrate support. The [bank](../src/osprey/examples/projects/modules/README.md) and [mobile apps](../src/osprey/examples/mobile/README.md) demonstrate host boundaries. A specification alone is not proof of implementation.

## Maturity and invitation

The phrase "Logic, in flight" is banned in every form: it says nothing about Osprey. Use **Safe, elegant, fast.** instead.

The maturity labels "alpha" and "experimental" are banned in Osprey messaging. Do not use either to describe the language or imply production readiness. Use **actively evolving**: Osprey is being used to build real applications, including the macOS storage app and Vigil, while language and tooling changes still require validation.

Preferred copy: **Osprey is actively evolving. Try it on your projects, validate it against your workload, and assess the risks before relying on it in production.** Keep concrete feature limits beside the relevant claim. Invite people to build and report failures without promising stability or support that has not been demonstrated.

## Qualifications to preserve

- Osprey is actively evolving. Native dynamic resumption is deep and single-shot; reusable continuations, owned escaping resumptions, and full scoped row polymorphism remain delivery work in plan 0016.
- WebAssembly and mobile C ABI targets support value effects and static discharge; unsupported dynamic control operations are rejected before linking.
- Integer arithmetic is checked against explicit `Arith` policies. The host runtime invokes them; device-kernel faults currently abort instead of calling a recovering policy. Qualify no-trap claims with this delivery limit. Total wrapping/saturating helpers name their behavior; `checkedAdd`, `checkedSub`, and `checkedMul` return `Result`. See [arithmetic effects](specs/language/0037-ArithmeticEffects.md).
- Explain the GC default through its concrete tradeoff: ordinary retain/release calls are no-ops, avoiding ARC reference-count updates at ownership changes, while accepting delayed reclamation and collection pauses. Both modes are automatic; do not imply GC is universally faster or ARC requires ownership annotations.
- Native builds default to tracing GC; ARC is the alternative managed backend. Unmanaged allocation requires explicit `--memory=none` and retains general allocations. The old `--memory=default` spelling is rejected. Windows native users must select ARC because tracing GC is outside the supported Windows contract. Strict static-memory checking and tail-call optimization are not implemented.
- Mobile C ABI targets default to ARC. [Host ABI version 1](specs/compiler/iOSCodegen.md#calling-the-host-ios-host-imports) copies strings returned by host imports into Osprey-managed memory. Hosts copy exported text while input borrows remain valid, then call `osprey_release_string` exactly once; they never use libc `free` on that pointer. Platform networking belongs to the native host.
- Project modules and cross-flavor imports work. The package manager is in progress under `src/osprey/packages/manager/` (plan 0020): the folder registry, folder publication, dependency resolution, verified fetching and locked builds run in Osprey. The dedicated hosted package website, authenticated public publishing and trust services remain planned. See the [package manager guide](../src/toolchain/website/src/docs/package-manager.md).
- GPU is vendor-neutral; optional CUDA interop is separate. [GPT-2](../src/osprey/examples/llms/gpt2/README.md) loads published weights; one static `Tensor` effect has host-list and GPU handlers. The handler decides where arithmetic runs. GPU operations execute on Metal or OpenCL with software binary64; buffers stay device-resident. Tokenization, loading, and readback remain on the host; kernels are not fused. Device transcendental results can differ from host libm in the last digit; sampled tests do not prove universal correct rounding. Never claim automatic offloading, universal acceleration, or bit-for-bit transcendental parity. See the [GPU contract](specs/runtime/0034-GPUComputation.md).
- Calling C crosses Osprey's memory-safety boundary. Package source scanning marks native `extern` declarations as unclassified, so source-bundle validation and local folder publication reject them. This publication rule does not establish complete runtime confinement of untrusted packages; see the [package guide](../src/toolchain/website/src/docs/package-manager.md#supply-chain-controls-and-remaining-work).

Update claims when the implementation and its verification change. A feature appearing in a specification does not by itself establish support.

## Proof development and the Lean oracle

Osprey uses Lean as a development oracle for the initial compatible discipline: pinned statements, proof terms, axiom reports and executed reference behaviour provide concrete checks while Osprey develops its own kernel, frontend, automation and mathematics libraries. This is a cooperative reference relationship. Describe Osprey's capabilities, remaining work and reproducible evidence; do not position the project as competing with Lean or treat benchmark ordering as a product objective. Performance checks remain engineering gates, with their existing requirements intact.

Lean accepting a certificate is reference evidence, not permission for Osprey to accept an unsound proof. Distinguish a kernel implementation defect from an inconsistent set of assumptions or a proposed change to the logical theory. The [wrong-structure projection exploit reported in Lean issue 14576](https://github.com/leanprover/lean4/issues/14576) is a concrete implementation counterexample to retain as a rejection regression, independently reviewed against the intended rules. Do not describe an implementation bug as proof that Lean's underlying mathematics is inconsistent. Oracle disagreements require investigation; copying the oracle's bug is never a resolution.

The compiler owns only the generic discipline protocol. Each theory remains a selectable, versioned package that can be fixed, replaced or forked without changing ordinary Osprey's Hindley-Milner rules. Changing a checker, rule or axiom policy invalidates the affected proof receipts and caches and requires fresh checking. A fork does not inherit mathematical validity from its parent, and a proof depending on a withdrawn assumption remains unavailable until reconstructed. Pluggability enables correction; it does not by itself prove consistency.

Independence means Osprey can author, elaborate, check and replay the supported proofs, build its mathematics library and compile verified programs without Lean installed or invoked. Retain Lean and other independent checkers as external development oracles after that point. Track this through the [completion milestones](plans/0041-dependent-types-and-mathlib.md#completion-milestones-proof-completion), with imported certificate coverage, source translation and locally reconstructed proofs reported separately.
