# Plan 0014 - Modules, Namespaces, and Multi-File Apps

## Summary

Implement [spec 0025 - Modules and Namespaces](../specs/0025-ModulesAndNamespaces.md):
.NET-style logical namespaces for path-independent names, ML-style modules and
signatures for abstraction, and explicit state modules for centralising mutable
state. Namespaces are flat-first opaque labels; module/member qualification uses
`::`, not a promoted dot hierarchy.

The compiler now has one shared module AST, both surface projections, a
compiler-facing project resolver, deterministic flattening into the existing
checker/backend, project CLI commands, project-aware editor diagnostics, and
module-aware same-document navigation.
The remaining work is listed explicitly below rather than hidden behind a
generic "modules planned" status.

## Current State

- `osprey-ast` has structured namespaces, symbol paths, imports, modules,
  signatures, visibility, opacity, and state-kind metadata.
- Default and ML parse their deliberately different surfaces into that one AST.
- `osprey-project` scans mixed-flavor source roots, merges logical namespace
  contributions, checks imports/signatures/privacy/state ownership, and emits a
  resolved flat program for the existing type checker and backend.
- `osprey build`, directory/manifest inputs, and module-aware single files work;
  ordinary scripts still bypass assembly unchanged.
- LSP diagnostics assemble the saved mixed-flavor project graph and overlay the
  current document buffer; formatter/navigation understand module syntax within
  a document. Incremental cross-file indexing remains part of plan 0009.
- Opaque record/union boundaries retain nominal structure. Opaque manifest
  aliases are rejected loudly because the flat checker cannot yet expose their
  representation only to the owner without leaking its ABI to clients.
- `docs/specs/0011-LightweightFibersAndConcurrency.md` has an older
  fiber-isolated module sketch. Spec 0025 supersedes it.
- Cross-file LSP is already planned in
  [plan 0009](0009-lsp-context-and-cross-file.md), but it needs the module graph
  from this plan.

## Non-Goals

- No package manager yet.
- No recursive modules in the first implementation.
- No higher-order parameterised modules before basic signatures work.
- No wildcard imports in library code by default.
- No implicit path-to-namespace mapping.
- No default `Company.Product.Feature` namespace convention.
- No semantic hierarchy from namespace separators; quoted slash labels are
  opaque names.

## Phase 0 - Spec And Parser Contract

TODO:

- [x] Add spec 0025.
- [x] Update `docs/specs/0003-Syntax.md` so `import` and `module` point to spec
      0025 for semantics.
- [x] Update `docs/specs/0011-LightweightFibersAndConcurrency.md` to mark the
      old fiber-isolated module paragraph as superseded by spec 0025.
- [x] Add the comparative language-practice survey and flat-first namespace
      style rules to spec 0025.
- [x] Decide exact surface grammar for ML-flavor `namespace`, `module`,
      `signature`, `export`, and punctuation-free `state Name`.
- [x] Reserve new keywords in both flavors: `namespace`, `signature`, `export`,
      `opaque`, `state`, `as`.

## Phase 1 - AST And Project Model

TODO:

- [x] Add `NamespaceName(String)` and `SymbolPath(Vec<String>)` to `osprey-ast`.
- [x] Replace string-only `Stmt::Module { name }` with symbol paths and
      visibility/export metadata.
- [x] Add `Stmt::Namespace { name, body }`.
- [x] Add `Stmt::Signature { name, items }`.
- [x] Add import forms: namespace/module import, member import list, alias, and
      wildcard.
- [x] Add `Visibility::{Private, Exported}` on module items.
- [x] Add opaque/manifest type export metadata.
- [x] Add `ModuleKind::{Plain, State}`. There is deliberately no annotation
      escape hatch in the initial module system.
- [x] Preserve source positions on every new declaration and rebase positions
      into collision-free per-project ranges for diagnostics and LSP.

## Phase 2 - Frontend Lowering

TODO:

- [x] Default flavor: parse block-scoped and file-scoped `namespace`.
- [x] Default flavor: parse `module A::B { ... }`, `state module`, `signature`,
      `export`, `opaque type`, import aliases, member lists, and wildcards.
- [x] Default flavor: parse flat namespace labels plus quoted slash labels in
      namespace/import declarations.
- [x] Default flavor: parse `::` symbol paths in expressions and types.
- [x] ML flavor: add the same constructs in layout form and lower to the same
      canonical AST.
- [x] Interpolation re-entry must parse `::` symbol paths using the current file's
      flavor.
- [x] Add parser tests for path-independent namespace declarations, flat
      namespaces, quoted slash labels, duplicate namespace blocks, exports,
      signatures, aliases, and `::` calls.
- [x] Add formatter/lint fixtures that prefer flat labels in docs/examples but
      continue accepting quoted slash labels and reverse-domain labels.

## Phase 3 - Project Loader And Namespace Graph

TODO:

- [x] Add `ProjectGraph` and assembled-project metadata in the compiler-facing
      `osprey-project` crate, consumed by the CLI and LSP diagnostics.
- [x] Read `osprey.toml` with `source_roots`, `default_namespace`, and module
      policy. Single-file mode remains unchanged.
- [x] Scan `.osp` and `.ospml` files under source roots.
- [x] Resolve flavor per file using the existing precedence rules.
- [x] Parse every file independently, then merge namespace declarations by
      namespace label.
- [x] Build an import table that maps imports to namespaces/modules, never file
      paths.
- [x] Detect duplicate declarations/exports in one namespace/module.
- [x] Detect ambiguous imports and require explicit qualification or aliases.
- [ ] Add style diagnostics as warnings only: folder drift, deep hierarchy in app
      code, and reverse-domain labels outside published-library config.
- [x] Enforce one project entry point: designated entry file or `fn main()`.
- [x] Reject executable top-level statements in non-entry project files.

## Phase 4 - Name Resolution And Type Checking

TODO:

- [x] Add a resolver pass before type checking: local scope, module private
      scope, imported aliases, imported members, namespace/symbol-path lookup,
      builtins.
- [x] Store deterministic resolved linkage names plus a source-name side table.
- [x] Make type inference consume resolved linkage names instead of unresolved
      `::` paths.
- [x] Enforce module privacy, including private intermediate module boundaries.
- [x] Check explicit exports and structural signature ascriptions.
- [ ] Implement opaque exported types: representation available inside the owning
      module, abstract outside. Opaque manifest aliases currently fail loudly
      instead of leaking; transparent aliases are implemented.
- [x] Check effect declarations and operation shapes through signatures.
- [ ] Allow separate type checking of importers against signatures.
- [x] Add tests for cross-file values, functions, effects, signatures, and
      Default/ML flavor pairs. Full imported opaque-alias tests remain blocked
      by the item above.

## Phase 5 - State Ownership Rules

TODO:

- [x] Reject namespace-level `mut` while preserving ordinary function-local
      `mut` and reassignment.
- [x] Reject direct state-cell reads and writes everywhere except the owning
      state module's algebraic-effect handler arms.
- [x] Reject exported `mut` cells, including signature-based export attempts.
- [ ] Reject state-cell escape through exported pointers/references once pointer
      escape analysis exists; until then, reject direct export of `Ptr` derived
      from a state cell.
- [x] Require every state owner with cells to export its owned algebraic effect
      and a fused handler installer; ordinary functions are never a state access
      path.
- [x] Enforce one `state module` per namespace with no initial-stage annotation
      escape hatch.
- [x] Instantiate fresh private cells inside each handler installer rather than
      emitting module-global storage.
- [ ] Add LSP warnings and docs metadata that list all project state boundaries.
- [x] Add negative tests for scattered state, direct/qualified access,
      wrong-effect handlers, escaping lambda/spawn factories, nested-module
      handlers, wildcard state imports, impure initialization, and exported
      mutable cells.

## Phase 6 - Codegen And Runtime

TODO:

- [x] Mangle namespace labels and `::` symbol paths deterministically and
      collision-free.
- [x] Update function, extern, effect, handler, type-constructor, and qualified
      generated lambda names to use resolved symbol IDs.
- [x] Ensure imports do not emit runtime initialization.
- [x] Lower pure namespace/module constants without hidden order dependence.
- [x] Lower state module instances through explicit handler/instance
      construction.
- [x] Reject cyclic/impure constant and state initialization before codegen.
- [ ] Preserve source-level names in debug info and stack traces.
- [ ] Add IR equivalence tests for cross-flavor modules with identical canonical
      project graphs.

## Phase 7 - CLI, LSP, Formatter, Docs

TODO:

- [x] CLI: `osprey build` / project mode reads `osprey.toml`; existing single-file
      commands keep working.
- [ ] CLI: diagnostics show namespace labels, `::` symbol paths, and ranked
      import candidates. Physical source mapping and qualified paths work;
      ranked candidate suggestions remain.
- [ ] LSP: maintain an incremental project graph across open files and source
      roots.
- [x] LSP: run diagnostics through the mixed-flavor project assembler, overlay
      the current unsaved document, and map project/type errors back to its
      physical local lines.
- [x] LSP: same-document go-to-definition, references, hover, completion, and
      fully-qualified document symbols understand namespaces/modules/imports.
      Cross-file behavior depends on the incremental graph item above.
- [ ] LSP: show state-boundary warnings and quick fixes for aliases and explicit
      `::` paths.
- [x] Formatter: preserve file-scoped namespace and format module/signature
      blocks in both flavors.
- [ ] Docs generator: create namespace/module reference pages from exported
      signatures.

## Phase 8 - Tests And Examples

TODO:

- [x] Add `examples/projects/modules/` as a real multi-file build/run project
      (the single-file differential harness cannot represent project manifests).
- [x] Include path-independent namespaces where file paths intentionally do not
      match namespace names.
- [x] Include flat namespace examples and a quoted slash-label example
      to lock in separator neutrality.
- [x] Include Default imports ML and ML imports Default.
- [x] Include explicit import lists and aliases in the runnable project, plus
      ambiguous-import and wildcard-policy failures in project tests.
- [ ] Include signatures with opaque and manifest types.
- [x] Include a state module that exposes an effect handler and a pure fake for
      tests.
- [x] Add compile-fail project tests for namespace/plain-module/exported `mut`,
      duplicate/private boundaries, state scatter, and initializer failures.
- [x] Add LSP regressions proving the runnable mixed-flavor project has no false
      cross-file diagnostics and real project/type errors map to the open file.
- [ ] Add LSP integration tests for cross-file completion/hover/definition.
- [ ] `make ci` green.

## Rollout Order

1. AST + parser support with no project mode, covered by unit tests.
2. Project graph and resolver in check-only mode.
3. Type checker on resolved symbol paths.
4. Exports/signatures/opaque types.
5. State rules.
6. Codegen for multi-file project mode.
7. LSP and formatter polish.
8. Parameterised modules after the basic module system is stable.

## Risks

- Resolver churn will touch type checking and codegen. Keep a raw-name fallback
  only temporarily and remove it before project mode is declared complete.
- Opaque types need careful interaction with existing union/record constructors.
- State modules overlap with existing handler-owned state. Treat state modules as
  a disciplined way to define handlers and access paths, not as process-global
  mutable singletons.
- The old top-level-script model is useful for examples. Preserve it in
  single-file mode while project mode enforces one entry point.
