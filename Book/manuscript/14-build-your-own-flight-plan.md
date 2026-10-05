# Chapter 14 — Build your own flight plan

**Chapter outline.** The capstone walkthrough and planning canvas are planned. Use the current implementation boundaries below when scoping the project.

## Reader outcome

Scope a small application, design its values and failure paths, choose evidence, and plan deployment without depending on unshipped features.

## Flight Log state

The running project becomes a template. The reader keeps it or replaces its domain while preserving the same design questions.

## Core sections

1. Choose an observable outcome, not a framework
2. Sketch valid data states before functions
3. Put expected failure into the design
4. Keep outside work at named boundaries
5. Add tests before widening the feature surface
6. Measure before optimizing or changing memory mode
7. State alpha and platform limits in the project README
8. Choose a command-line, supported browser, native-service, or native-mobile route

## Current boundaries for a capstone

Begin with an observable result and a testable pure model. Name the outside operations as effects, choose callable handlers at application entry points, and keep expected failures in `Result`. Install a handler before its callback runs. A browser event or native host callback is a later entry point and must handle its own effect requirements; a handler installed only during startup is not still active afterwards.

Project modules and cross-flavor imports are available. Use logical namespaces and exported module members to separate the model, operation declarations, adapters, and entry points. Do not make the plan depend on the package manager or registry: their specification describes intended behavior, not available install or publish commands.

Choose the deployment route with its host and test environment:

| Route | Concrete starting point | Evidence the plan should require |
|---|---|---|
| Native command-line tool | The Flight Log file boundary | Exact output, explicit failures, isolated filesystem tests, and a runnable binary. |
| Browser application | The bank's shared Osprey client and browser bridge | Portable-target checks, browser events, and end-to-end browser behavior. |
| Native HTTP service | The bank's Osprey server and storage handler | Request/response tests, storage failure cases, and explicit resource cleanup. |
| Android or iOS application | The bank mobile host or Issue Inbox | Shared Osprey tests plus platform builds and emulator/simulator tests; device behavior where relevant. |

Mobile handlers may describe commands which their host executes, then receive completion events. Plan that protocol explicitly instead of assuming a synchronous file or HTTP adapter can be copied unchanged. iOS app builds and simulator tests need macOS and Xcode; an Android or shared-language test does not establish that the SwiftUI application builds.

Keep the scope within current runtime capabilities. WebAssembly and mobile use the default memory backend and have host-service boundaries. Native fibers use pthreads; cancellation, scopes, and deadlines are planned. Strict static-memory checking, tail-call optimization, and GPU device execution are not available. Current GPU kernels execute on the host, so a capstone cannot claim accelerator performance from that syntax alone.

Write these constraints next to the relevant feature in the project README. Osprey remains alpha software; record the compiler build and the checks actually run. A skipped platform build is an evidence gap to report, not a successful test.

## Compiler-feedback exercise

Turn one vague capstone requirement into a concrete type or test, then use the compiler or test runner to expose what the plan had left unspecified.

## Flight Log checkpoint

Produce a one-page build plan with domain states, pure functions, failure routes, outside effects, tests, target, and explicit non-goals.

## Planned visuals

- Capstone planning canvas
- Claim-to-evidence path
- Next-step learning map

## Source map

- Working application boundaries: `examples/projects/modules/README.md`, `examples/projects/modules/mobile/README.md`, and `examples/mobile/README.md`.
- Project and target contracts: `docs/specs/0025-ModulesAndNamespaces.md`, `0022-WebAssemblyTarget.md`, `0038-iOSTarget.md`, and `0039-AndroidTarget.md` in the same directory.
- Delivery boundaries: `docs/plans/0016-algebraic-effects-and-handlers.md`, `0020-package-manager.md`, `0023-gpu-computation.md`, and `0026-structured-concurrency.md` in the same directory.
- `website/src/status.md` provides orientation; confirm a chosen capability against its current specification and runnable evidence before promising it in the capstone.
