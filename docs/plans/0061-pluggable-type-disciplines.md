# Plan 0061 — Pluggable type disciplines

Contract: [spec 0050](../specs/language/0050-TypeDisciplines.md) (the seam) · [spec 0049](../specs/language/0049-DependentTypesAndProofs.md) (dependent disciplines). The kernel, the Lean-source target and Mathlib are [plan 0041](0041-dependent-types-and-mathlib.md); this plan is only the mechanism and the disciplines that prove it.

## Goal

Any type system other than Hindley-Milner checks an Osprey module as a package, chosen per module, with no theory in the compiler. Ordinary programs keep HM unchanged. Proved by more than one discipline of more than one kind running side by side.

## Design rules

The normative rules, with their sources, are [TYPES-DISCIPLINE-PRINCIPLES](../specs/language/0050-TypeDisciplines.md#design-principles-from-the-literature-types-discipline-principles). Every item below either meets one of them or names the one it leaves open.

## Done

- [x] Marker, registry, checker process with one deadline, protocol version 2 with imported signatures and `omitted[]`, binary and result caches, erasure ascribed to the written signature ([TYPES-DISCIPLINE-SEAM], [TYPES-DISCIPLINE-PROTOCOL]); every `discipline-error` case tested in `src/toolchain/crates/osprey-project/src/discipline/tests.rs`.
- [x] Purity: `src/toolchain/crates/osprey-project/tests/discipline_purity.rs` fails if crate code names a discipline.
- [x] A disciplined module may use syntax HM rejects: it is parsed only for its outline, and only the erased module is parsed strictly ([PROOF-SOURCE-DISCIPLINE]; test `a_disciplined_source_keeps_its_outline_through_syntax_hm_rejects`).
- [x] Generic exports cross the boundary: an inferred implementation takes its signature's binders (`src/toolchain/crates/osprey-project/src/annotation.rs`), and declared binders are rigid in every function (`reject_less_general` in `src/toolchain/crates/osprey-types/src/check/functions.rs`, tests in `generics_decl_tests.rs`). This closed an existing hole where `pick<A, B> : (A, B) -> B` accepted `pick (a, _b) = a`.
- [x] Checker-declared `dependencies[]`: a cached answer is reused only while every file it read is unchanged (test `a_cached_result_is_rechecked_when_a_file_it_depended_on_changes`).
- [x] One protocol implementation shared by every line-erasing checker: `bounded`'s `Checker::serve judge`, where a judge returns problems, erased lines and files read.
- [x] Four reference disciplines, each with an accepted round trip under GC, ARC and wasm32 and a rejected twin against the compiler's own stderr, in `make _test_disciplines`:
  - `passthrough`, the identity;
  - `bounded`, interval refinements on `int` parameters;
  - `dependent`, `theorem` lines checked by the kernel (`dependent_roundtrip`: an access policy; rejected: a proof that composes the wrong way round);
  - `leanexport`, Lean theorems cited by pinned `lean4export` certificates and re-checked from an empty environment (`lean_roundtrip`: vote totals relying on `Nat.add_comm`; rejected: a counterfeit certificate).

## Open

- [ ] Render exported types and values in an inferred `hm` signature instead of naming them in `omitted[]` (`omitted_of` in `src/toolchain/crates/osprey-project/src/discipline/imports.rs`). Acceptance: a disciplined module imports an `hm` module's type and value, and the `omitted[]` field is gone.
- [ ] Markerless byte identity: every program without a marker produces the same output and diagnostics before and after the seam, measured over the whole differential corpus in CI.
- [ ] Theory identity: a checked result is keyed by discipline package and version, checker binary, rule revision, axiom policy and imported signature identities; relabelling a marker or swapping a package re-checks ([TYPES-DISCIPLINE-EVOLUTION]). Acceptance: switching a module from `dependent` to a fork with a different identity re-runs the checker even when the request text is unchanged.
- [ ] A fork restricting an axiom policy: `leanexport` with `Quot.sound` withdrawn refuses a certificate that uses it while the axiom-free `Nat.add_comm` still checks.
- [ ] Separate the three repair paths of [TYPES-DISCIPLINE-EVOLUTION]: an implementation fix replays affected certificates; an axiom change is a new policy; an inference or equality change is a new theory profile with its own fixtures.
- [ ] Editor support: the language server shows a discipline's diagnostics and stops reporting HM syntax errors in disciplined files (it already uses the same parse path; add an LSP test).
