# Plan: math to code

Contract: [MATH-CODE](../specs/tooling/MathToCode.md). Related: [plan 0041](0041-dependent-types-and-mathlib.md) (proofs in `.lean`), [plan 0042](0042-proof-workbench.md) (the workbench that displays generated modules).

Mathematics is written in Lean and Osprey modules are generated from it. This plan replaces the formula-generation phases formerly in plan 0042, which were built on a custom definition format that [MATH-CODE-DELETED] removes.

## Constraints

- **Lean's format is the target.** Work on an Osprey-specific format for mathematics is prohibited unless Lean's format cannot express the functionality, and then only with the specification entry [MATH-CODE-LEAN] requires. A Lean construct the reader does not accept yet is fixed by extending the reader.
- A math source is accepted by the pinned Lean, or it is not a math source.
- A Lean operation is emitted only where the Osprey operation computes the same value; otherwise it is refused by name.
- Generated modules are untracked by default and never edited [MATH-CODE-UNTRACKED].
- One generator, in the toolchain. No host, package or application holds a second reader or emitter.
- No existing gate, test, threshold or recorded output is relaxed to fit a conversion.

## Phase 1 — contract

- [x] [MATH-CODE](../specs/tooling/MathToCode.md) replaces `docs/specs/applications/FormulaCodeGeneration.md`; plans 0041 and 0042 point at it.
- [x] The prohibition on Osprey-specific math formats is in the specification, this plan and `AGENTS.md`.

## Phase 2 — fragment 1: `Float` and `Bool` definitions [MATH-CODE-FRAGMENT]

- [x] `src/toolchain/crates/osprey-math`: lexer, reader with the checks of [MATH-CODE-FRAGMENT], emitter per [MATH-CODE-EMIT], the closed refusals of [MATH-CODE-REFUSALS].
- [x] Goldens for every fragment row; one near-miss per refusal code with its line, column and fix; byte-identical regeneration.
- [ ] `osprey math <file.lean>` [MATH-CODE-COMMANDS], in `make help`.

## Phase 3 — projects build from `.lean` [MATH-CODE-SOURCES]

- [x] The project loader discovers `.lean` under `source_roots`, generates into `.osprey/math/`, writes only changed bytes, and writes the `.gitignore` [MATH-CODE-UNTRACKED].
- [ ] A refusal fails the project at the `.lean` path; a fixture project proves the command line and the test runner build it, and that every generated golden compiles.

## Phase 4 — conversions

Hand-written mathematics moves to `.lean`; the package's recorded outputs stay byte-identical.

- [x] `@osprey/synth`: `src/Waves.lean` holds the oscillator shapes (`wrap`, `blep`, `blamp`, `sine`, `saw`, `pulse`, `triangle`) and the performance timbres (`harmonic`, `glass`, `bass`, `hollow`, `modulated`, `ring`); recorded output byte-identical.
- [x] `@osprey/micrograd`: `src/Libm.lean` holds `exp_of` and `tanh_of`; recorded output byte-identical.
- [ ] Next candidates, each when its fragment exists: the `@osprey/synth` filter coefficients and envelopes (records), `@osprey/torch` learning-rate schedules (integers, folds), `@osprey/scipy` logistic and Box-Cox (calls into hand-written series).

## Phase 5 — Lean acceptance and the oracle [MATH-CODE-ORACLE]

- [ ] `make -C src/osprey/examples/proof math`: every tracked math source is accepted by the pinned Lean.
- [ ] The oracle corpus: Lean evaluates each definition at recorded points and prints the bits; the generated module's bits are equal under GC and ARC.
- [ ] Every literal of every tracked source: Lean's bits equal the literal reader's [MATH-CODE-LITERALS].
- [ ] The proof job in CI runs it.

## Phase 6 — follow-on fragments [MATH-CODE-LIMITS]

Each is a specification change first: meaning table, refusals, oracle.

- [ ] Integer types: `Int64` with Lean's wrapping semantics, then `Nat` and `Int` under a stated bound.
- [ ] `structure` values against typeDiagram-declared records.
- [ ] `match`, structural recursion and `where`.
- [ ] Imports between math sources.
- [ ] Definitions over Mathlib's `ℝ` under a declared binary64 approximation profile.

## Phase 7 — workbench [MATH-CODE-WORKBENCH]

- [ ] The `.lean` document of [plan 0042](0042-proof-workbench.md) phase 1 shows the generated module read-only beside it, through the toolchain.
- [ ] The four statuses, each from its own evidence; an edit clears them before a new one is granted; `EquivalenceProved` is shown as unavailable.
- [ ] Native, VS Code and browser hosts complete one accepted and one refused flow each.
