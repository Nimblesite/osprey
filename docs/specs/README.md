# Osprey specifications

The **language** and **compiler** contracts are separate from runtime libraries, host platforms, developer tooling, package infrastructure and application frameworks. Choose the subject you are changing before editing a contract.

## Core language and compiler

- [Language](language/README.md): Syntax, types, expressions, effects and module semantics.
- [Compiler](compiler/README.md): Target lowering, generated ABIs and debug artifacts.

## Supporting systems

- [Runtime libraries](runtime/README.md): Built-in library contracts and execution services.
- [Platform integration](platforms/README.md): WASI/browser hosts, iOS and Android applications.
- [Developer tooling](tooling/README.md): Language server, editor, documentation and test tools.
- [Packages and automation](ecosystem/README.md): Package registry, evidence and GitHub automation.
- [Application frameworks](applications/README.md): Reactive mobile applications, the proof workbench and host protocols.

## Contract and delivery

A normative specification defines required behavior; it does not establish implementation or passing tests. [Active plans](../plans/README.md) track unfinished delivery. Keep specifications, implementation, tests and public claims coherent. Preserve hierarchical contract identifiers when moving a rule so comments and assertions continue to name the same requirement.

Mixed documents split at their implementation boundary. iOS and Android application integration live under `platforms/`; their option checking, lowering, C ABI and symbol rules live under `compiler/`. Documentation syntax and test built-ins live under `language/`; export tools, test discovery, coverage and editor workflows live under `tooling/`. Debug artifacts belong to `compiler/`, while debugger/editor launch belongs to `tooling/`.

The [website generator](../../website/scripts/copy-spec.js) reads these subject folders and [section metadata](sections.json). It publishes separate indexes and labels every contract by subject. Existing per-document URLs stay stable. The effects contract remains [Algebraic Effects](language/0017-AlgebraicEffects.md); [plan 0016](../plans/0016-algebraic-effects-and-handlers.md) owns its delivery status.
