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
- [x] `osprey math <file.lean>` [MATH-CODE-COMMANDS].

## Phase 3 — projects build from `.lean` [MATH-CODE-SOURCES]

- [x] The project loader discovers `.lean` under `source_roots`, generates into `.osprey/math/`, writes only changed bytes, and writes the `.gitignore` [MATH-CODE-UNTRACKED].
- [x] A refusal fails the project at the `.lean` path; a fixture project proves the loader builds it and that the generated golden type-checks.

## Phase 4 — conversions

Hand-written mathematics moves to `.lean`; the package's recorded outputs stay byte-identical.

- [x] `@osprey/synth`: `src/Waves.lean` holds the oscillator shapes (`wrap`, `blep`, `blamp`, `sine`, `saw`, `pulse`, `triangle`) and the performance timbres (`harmonic`, `glass`, `bass`, `hollow`, `modulated`, `ring`); recorded output byte-identical.
- [x] `@osprey/micrograd`: `src/Libm.lean` holds `exp_of` and `tanh_of`; recorded output byte-identical.
- [x] `@osprey/math`, the standard library of well-known formulas, is Lean only: `Real` (`pi`, `sinh`, `cosh`, `tanh`, `arsinh`, `logb`, `sigmoid`, `negMulLog`, `binEntropy`, `gaussianPDFReal`), `Quadratic` (`discrim`, `rootAdd`, `rootSub`) and `AffineMap` (`lineMap`). Each restates a named Mathlib declaration at the pinned commit over `Float` and quotes its source line; `open Float (exp log sqrt)` keeps the text Mathlib's.
- [x] `@osprey/scipy`: `src/Sigmoid.lean` holds `expit`; recorded output byte-identical.
- [x] `@osprey/synth`: `src/VoiceMath.lean` holds the state-variable filter (`warped`, `damping`, the three gains, the band, low and high outputs, `integrated`) and the reference voice (`midiKey`, `swept`, `butterworth`, `triangle`, `saw`, `knobCutoff`), reusing `Waves` for `wrap`, `sine` and `midiFrequency`. Package tests 39/39 under GC and ARC.
- [x] The daw: `ArrangeMath.lean` holds the piece length (`pieceFrames`). A formula moves only when the Osprey gets shorter or more precise; one-operator sums stay inline.
- [ ] Every remaining Float formula in `@osprey/synth/src/*.ospml` moves into its domain's `*Math.lean` (physical, spectral, tables, nonlinear, phase, grains, fitting, control).
- [ ] Next candidates, each when its fragment exists: `@osprey/torch` learning-rate schedules (integers, folds), `@osprey/scipy` logistic and Box-Cox (calls into hand-written series).

## Phase 5 — Lean acceptance and the oracle [MATH-CODE-ORACLE]

- [x] `make -C src/osprey/examples/proof math` (`scripts/math/math-oracle.py`): every tracked math source is accepted by the pinned Lean.
- [x] Lean evaluates each exported definition over a grid of points and prints the bits; the generated module's bits are equal under GC and ARC. A control whose two sides compute different functions must be caught.
- [x] Every literal of every tracked source: Lean's bits equal the decimal's [MATH-CODE-LITERALS].
- [x] The proof job in CI runs it.
- [ ] wasm32 joins GC and ARC.

## Phase 6 — follow-on fragments [MATH-CODE-LIMITS]

Each is a specification change first: meaning table, refusals, oracle.

- [x] [MATH-CODE-EMIT-ELEGANT] The generated Osprey reads as cleanly as the Lean source: emit no `name : type` header when inference derives the same type (today every export gets one, e.g. `export twiddle : (float, float, float) -> float` above a body that `floatDiv` already pins to float), and write float division as `/` rather than `floatDiv (a, b)`. Acceptance: `osprey --check` with exported members treated as non-contract reports no redundant annotation on any generated module, and the [`osprey-math` fixtures](../../src/toolchain/crates/osprey-math/tests/fixtures/Kernels.ospml) show it. Done: `osprey-math` `pin.rs` proves which types the body fixes and annotates only the open ones inline; no generated module in the repository has a header left; `heads_annotate_only_what_inference_cannot_derive` and `a_definition_called_before_pins_its_arguments_to_its_lean_types` cover it; synth suite 55/55 under ARC on the generated modules. `floatDiv` stays (it is IEEE division, not verbosity).
- [ ] [MATH-CODE-EMIT-ELEGANT] The analyser misses redundant signatures in modules: a script module with `export twiddle : (float, float, float) -> float` over a `floatDiv` body draws no `redundant-annotation` warning, while the same pair at top level does for `power`. Add the failing case to `redundant_tests.rs`, then fix.
- [ ] Integer types: `Int64` with Lean's wrapping semantics, then `Nat` and `Int` under a stated bound.
- [ ] `structure` values against typeDiagram-declared records.
- [ ] `match`, structural recursion and `where`.
- [ ] Imports between math sources.
- [ ] Definitions over Mathlib's `ℝ` under a declared binary64 approximation profile, so `@osprey/math` reads Mathlib's own text: `ℝ`, `x⁻¹`, `√x`, `π` and Greek names.

### Native Osprey formulas [MATH-CODE-NATIVE-NOTATION]

The comparison is between the production synth modules introduced in `4f8ad8a14` and the Lean sources in its parent. [ML notation requirements](../specs/language/0024-MLFlavorSyntax.md#mathematical-notation-flavor-ml-math-notation) apply to the native source and its proofs. Recheck each example through parsing, formatting, the native backends and the source-to-proof translation when it changes.

- [ ] **[MATH-CODE-NATIVE-CONDITIONAL]** Flatten the soft-knee cases in `DynamicsMath.curve`. Lean reads top to bottom; the converted body repeats `match`, `true` and `false` and indents the third case twice. Accept a chained conditional whose canonical AST and lazy branch behavior match the existing implementation.
- [ ] **[MATH-CODE-NATIVE-DIVISION]** Make the two variable-denominator ratios inside the compressor knee readable as infix IEEE division. Check ordinary results, division by signed zero and zero divided by zero. Preserve the current checked division operator’s handlers.
- [ ] **[MATH-CODE-NATIVE-POWER]** Restore the cube notation from `Waves.blamp` so the exponent stays beside its base. Check nested powers, a negative base, a negative exponent, and the difference between a negated power and a power of a negative base.
- [ ] **[MATH-CODE-NATIVE-LITERAL]** Let `DynamicsMath.rmsLevel` spell its floor as `1e-18`. Confirm the literal parses as a single float and survives formatting and proof extraction with identical bits.
- [ ] **[MATH-CODE-NATIVE-NEGATION]** Replace converter artifacts such as `(-1.0) * knee` with native `-knee` when their scalar behavior agrees. The native parser already accepts prefix negation. Compare signed zeros, finite values, infinities and NaNs before changing the emitter or the migrated sources.

## Phase 7 — workbench [MATH-CODE-WORKBENCH]

- [ ] The `.lean` document of [plan 0042](0042-proof-workbench.md) phase 1 shows the generated module read-only beside it, through the toolchain.
- [ ] The four statuses, each from its own evidence; an edit clears them before a new one is granted; `EquivalenceProved` is shown as unavailable.
- [ ] Native, VS Code and browser hosts complete one accepted and one refused flow each.
