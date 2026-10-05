# Chapter 13 — Choose another flavor when it helps

**Chapter outline.** The translation walkthrough remains to be written. The selection, call-shape, and verification rules below reflect the current implementation.

## Reader outcome

Translate one understood program between Default and ML, identify the differences that affect function shape, and prove unchanged behavior.

## Flight Log state

The complete Default source remains canonical for the book. One pure file gains an optional `.ospml` twin and shares the same output and tests.

## Core sections

1. Flavor changes the source surface, not the language you have learned
2. Default remains this book's teaching surface
3. ML replaces braces with layout and defaults to currying
4. Flat and curried calls are not punctuation twins
5. One file selects one surface
6. Use an agent for translation, then check and test
7. The architecture remains open to future flavors

## Current rules to preserve

- `.osp` selects Default and `.ospml` selects ML unless an explicit override applies. A leading flavor marker may also select a flavor; without an explicit override, a marker contradicting the extension is an error. The single-file CLI supports `--flavor default|ml`.
- A project can mix the two extensions. Setting `[project].flavor` in `osprey.toml` overrides the flavor for every source in that project, so omit it when extensions should select independently.
- Default `fn f(x, y) = ...` corresponds to ML `f (x, y) = ...`: both are flat functions. ML `f x y = ...` instead defines a curried function, corresponding to a Default function returning another function. Translating only braces and parentheses can change the callable contract.
- Imported functions keep that contract across flavor boundaries. An ML caller uses `f (a, b)` for a Default flat function; a Default caller applies a curried result as `f(a)(b)`.
- Both flavors use the current effect model: callable `handler` values, rest-of-block `handle`, declared value/control operations, and explicit `perform`. Removed `handle ... in` and `handle ... do` forms are not translation alternatives. A handler receives the function whose execution it must surround.
- `osprey fmt` formats within a file's selected flavor. It is not a flavor converter. Manual or agent-assisted translation still needs checking and execution.
- Equivalent twins can share exact output and, for specifically matched shapes, identical generated IR. That evidence does not mean an arbitrary flat program and an arbitrary curried rewrite have identical allocation or code-generation behavior.

The book already contains a verified pair at `examples/chapter-01/first-flight.osp` and `examples/chapter-01/first-flight.ospml`. Both use `first-flight.expectedoutput`; the book harness checks and runs each independently. Use that pattern when adding the later Flight Log twin.

## Compiler-feedback exercise

Translate a flat two-argument function as though it were curried, observe the real mismatch, and repair the source while preserving the function's intended shape.

## Flight Log checkpoint

Create an ML twin of one pure module, run both checks, and compare stdout or test evidence byte-for-byte.

## Planned visuals

- Several source feathers converging on one checked core
- Agent translation and verification loop
- Flat versus curried application

## Source map

- `docs/specs/0023-LanguageFlavors.md`: `[FLAVOR-SELECT]`, `[FLAVOR-CURRY]`, conversion, and cross-flavor imports.
- `docs/specs/0024-MLFlavorSyntax.md`: the current ML surface.
- `docs/specs/0025-ModulesAndNamespaces.md`: project source discovery and project-wide flavor overrides.
- Executable evidence: `crates/osprey-cli/tests/cross_flavor_equiv.rs`, `crates/osprey-cli/tests/cross_flavor_ir_equiv.rs`, and the Chapter 1 twins.

## Edition note

Never say Osprey is permanently limited to two flavors. Name Default and ML as the currently available surfaces and keep future additions structurally possible.
