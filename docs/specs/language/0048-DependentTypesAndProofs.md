# Dependent types, proofs and the Mathlib port [PROOF]

**Status:** normative target; **no implementation started.** Delivery: [plan 0041](../../plans/0041-dependent-types-and-mathlib.md). Nothing in this contract changes the Hindley-Milner type system of [spec 0004](0004-TypeSystem.md); every rule here adds a second, selectable type discipline beside it, through the pluggable-discipline mechanism of [spec 0049](0049-TypeDisciplines.md).

## Aim [PROOF-AIM]

Osprey becomes a proof assistant whose checked library is Mathlib, without changing the type system that Osprey programs use today. Three promises:

1. **Osprey programs keep Hindley-Milner.** Inference, effect rows, monomorphisation and the lowering pipeline of [spec 0023](0023-LanguageFlavors.md) are untouched. A program that compiles today compiles unchanged after every phase of this contract.
2. **Mathlib checks in Osprey.** An Osprey kernel implements Lean 4's type theory exactly, re-checks every exported Mathlib declaration, and ships the library as an Osprey package that mathematicians read, extend and build on.
3. **The type system is pluggable.** Each module chooses its discipline: Hindley-Milner (the default) or any registered alternative. Disciplines meet at the existing module signature boundary of [spec 0025](0025-ModulesAndNamespaces.md), and nothing below the canonical AST learns which discipline checked a module. The Lean-compatible dependent discipline of this contract is the first alternative, not the only one: other solvers and logics (higher-order logic, refinement types over SMT, separation logic, a second dependent theory) register the same way and share no code path that is specific to this one.

## The existing language is untouched [PROOF-NO-HARM]

This contract adds; it never alters. The rules below are testable and each has a gate:

1. **Every program that compiles today compiles unchanged, with identical output.** The differential corpus and `examples/` are the oracle: no `.expectedoutput` changes because of this contract, under GC, ARC and wasm32.
2. **No new keyword, operator, token or marker in `hm` modules.** A file without `// osprey: types=` is lexed, parsed, inferred, effect-checked and lowered exactly as before; `theorem`, `∀`, `by` and the rest remain ordinary identifiers or syntax errors there, as today.
3. **No change to `Type`, unification, effect rows, redundancy analysis, monomorphisation or any diagnostic text** in `crates/osprey-types`. The only addition is the single dispatch of [TYPES-DISCIPLINE-SEAM](0049-TypeDisciplines.md#seam-types-discipline-seam); a CI test fails if `crates/` mentions a discipline name, a theory or a tactic.
4. **No new runtime obligation.** A program that uses no discipline links no kernel, no elaborator and no proof library; binary size, startup and the memory backends are unchanged, measured by the existing benchmarks.
5. **No inference regression.** Hindley-Milner annotations stay redundant where they are redundant today ([TYPE-ANNOTATION-REDUNDANT](0004-TypeSystem.md#redundant-annotations--type-annotation-redundant)); a discipline cannot demand an annotation in an `hm` module.
6. **Removal is a deletion.** Deleting the discipline packages and the one seam leaves the compiler as it was; nothing in `hm` depends on them.

## Discipline [PROOF-DISCIPLINE]

`lean4` is a discipline under [spec 0049](0049-TypeDisciplines.md): a package declaring `[discipline] name = "lean4"`, selected per module by `// osprey: types=lean4`, run as a checker process through [TYPES-DISCIPLINE-PROTOCOL](0049-TypeDisciplines.md#checker-protocol-types-discipline-protocol), and meeting every other module at an HM export signature. Everything about registration, selection, the seam, the protocol, caching and the no-harm rules lives in spec 0049 and is not restated here; this contract adds only what is specific to Lean 4's theory.

### Boundary with Hindley-Milner [PROOF-BOUNDARY]

How the `lean4` checker reads and writes the signatures of [TYPES-DISCIPLINE-BOUNDARY](0049-TypeDisciplines.md#boundary-types-discipline-boundary):

- **HM into lean4:** total. Every HM type is a simple type of the dependent theory (`int` is `Int`, `List<T>` is `List T`, `fn(a) -> b` is `a → b`, a record is a structure, a union is an inductive). An effect row `!{E}` becomes a monad-like effect type the elaborator understands; a `lean4` module may call any HM export.
- **lean4 into HM:** by erasure. A `lean4` export is in the signature only when its erased type is an HM type ([PROOF-ERASE]). Proof arguments and propositions erase to nothing; type indices erase to their underlying simple type; an export whose erasure is not an HM type is private to `lean4` modules.
- An HM importer never sees a dependent term, a universe or a proof.

## Kernel [PROOF-KERNEL]

The kernel is the only trusted component. It is a type checker for terms of Lean 4's type theory, written in Osprey, compiled through the native backend, and small enough to audit: under 5,000 lines of Osprey, with a published prose specification of every judgement.

`[PROOF-KERNEL-THEORY]` The kernel implements, exactly as Lean 4 does, so that Lean's exported proof terms check without translation:

- a cumulative-free universe hierarchy `Sort u` with level expressions (`zero`, `succ`, `max`, `imax`, parameters) and universe-polymorphic constants;
- `Prop` as `Sort 0`, impredicative, with definitional proof irrelevance;
- dependent function types, application, lambda and `let`, with β, δ (unfolding by reducibility hint height), ζ, η for functions and for structures, and ι reduction;
- inductive types in the kernel's primitive form (parameters, indices, constructors, the generated recursor and its reduction rules), with the nested and mutual encodings Lean's front end produces, and the positivity and universe checks Lean performs;
- quotient types as the four primitives `Quot`, `Quot.mk`, `Quot.lift`, `Quot.ind` and the `Quot.sound` reduction;
- natural-number and string literals with the accelerated `Nat` operations Lean's kernel special-cases (`add`, `sub`, `mul`, `div`, `mod`, `gcd`, `beq`, `ble`, `land`, `lor`, `xor`, `shiftLeft`, `shiftRight`, `pow`, `log2`), computed on arbitrary-precision integers;
- declarations: axioms, definitions, theorems, opaque constants, with the environment checks on names, levels and free variables that Lean applies before adding each.

`[PROOF-KERNEL-TOTAL]` The kernel is an Osprey program and so obeys [FAILURE-EXPLICIT](0013-ErrorHandling.md): every rejection is a typed `Result` naming the declaration, the judgement and the two terms that failed to be definitionally equal. It never panics, aborts or exits.

`[PROOF-KERNEL-AXIOMS]` Each checked theorem records the axioms it depends on. The report of [PROOF-TRUST] lists them; Mathlib's are `propext`, `Quot.sound` and `Classical.choice`.

## Trust [PROOF-TRUST]

"Most trusted" is a measured claim or it is not made.

1. **Specification.** The kernel's judgements are written as prose and inference rules in this contract's companion document before the code, and every kernel function names the rule it implements.
2. **Cross-checking.** Every release re-checks the same Lean export with the Osprey kernel and with at least one independent checker of Lean's theory (`lean4lean` or `nanoda`), and the two accept exactly the same set of declarations. A disagreement blocks the release until explained in the report.
3. **Report.** `docs/reports/proof-trust-<date>.md` records the Mathlib commit, the export hash, declarations checked, wall time, the axioms used, and the cross-check result. The website's claim is a link to the latest report.
4. **No trusted tactics.** A tactic, an SMT solver or a learned guide proposes terms; only the kernel accepts them ([PROOF-TACTICS-CERT]).

`[PROOF-TRUST-LESSONS]` The 2026 Lean kernel soundness bugs (issue #14576, CVE-2026-72844, and the three found by adversarial testing before 4.34.0) are the design input for the kernel, and each becomes a rule with a pinned regression:

- **Projections are checked against their structure.** A projection `S.i` type-checks only when its argument's whnf is an application of `S` itself, including inside nested and mutual inductive declarations. The published `0 = 1` proof is checked into `tests/proof/adversarial/` and must be rejected.
- **A hash is never an identity.** The term store of [PROOF-STDLIB] resolves a hash collision by structural comparison; no kernel judgement reads two terms as equal because their hashes agree. A test plants a collision and proves the kernel still distinguishes the terms.
- **No metaprogram touches the kernel's state.** The kernel is a pure function from (environment, declaration) to `Result`; the elaborator, tactics and any plugin hold no reference to its data and cannot add a declaration except through that function. Lean's bug was reachable because a metaprogram ran inside the kernel's process with access to its structures; Osprey's kernel runs as a separate process with an environment it alone writes.
- **No counter can overflow into unsoundness.** Lean's reference-counting overflow became a use-after-free and then a proof of False. The kernel runs under Osprey's [ARITH-EFFECT](0037-ArithmeticEffects.md) policy with overflow as a typed fault, and under the GC backend by default; an ARC run is a cross-check, never the only check.
- **Cross-checkers are pinned forward.** The Collatz episode passed `nanoda` because a week-old version was used. [PROOF-TRUST] step 2 runs the independent checker at its current release, records its version, and treats "both accept" as evidence only when both are current.
- **Adversarial testing is continuous.** `tests/proof/adversarial/` grows with every published kernel exploit of any proof assistant, and an AI-driven fuzzing job proposes malformed declarations against the kernel on a schedule, reporting any acceptance as a showstopper.

## Lean export bridge [PROOF-EXPORT]

`osprey prove import <export-file>` reads the `lean4export` text format (declarations, levels, expressions, inductive specifications and quotient initialisation), adds each declaration to an Osprey environment through the kernel, and writes a checked environment file. The importer is the only code that parses Lean's format; it adds nothing to the theory.

`[PROOF-EXPORT-SYNC]` A scheduled job imports the current Mathlib master on a fixed cadence, records the result as a trust report, and opens a pull request that updates the ported library of [PROOF-MATHLIB]. A failing import is a red report, never a silent skip.

## Proof surface [PROOF-SURFACE]

`[PROOF-SURFACE-FLAVORS]` The proof language is one canonical proof AST with a frontend per flavor, on the model of [FLAVOR-BOUNDARY](0023-LanguageFlavors.md#canonical-ast-boundary): the elaborator, kernel, tactics and Mathlib port see the proof AST and never the flavor. **ML ships first**, because Lean 4's own syntax is ML-shaped (offside layout, curried application `f a b`, `fun x =>`, `(x : T)` binders, `∀`, `by` blocks) and the people the port is for already read it. Default and Python frontends follow once the ML frontend elaborates the port's acceptance module; a theorem written in any flavor must elaborate to the same kernel term, checked by the cross-flavor equivalence tests of [spec 0023](0023-LanguageFlavors.md#cross-flavor-equivalence-tests). Tactics are named identically in every flavor.

The ML frontend extends the layout of [spec 0024](0024-MLFlavorSyntax.md) with these forms, so that a mathematician moving from Lean reads familiar shapes in Osprey's spelling:

```ospml
// osprey: types=lean4
module Group.Basic

theorem mul_inv_cancel_left (G : Type u) [Group G] (a b : G) : a⁻¹ * (a * b) = b
    by rw [← mul_assoc, inv_mul_cancel, one_mul]

def order (G : Type u) [Fintype G] : Nat = Fintype.card G
```

- `theorem`, `def`, `abbrev`, `opaque`, `axiom`, `structure`, `inductive`, `class`, `instance`, `namespace`, `open`, `notation`, `universe` and `variable` declarations;
- binders `(x : T)`, `{x : T}` implicit, `[C T]` instance, `⦃x : T⦄` strict implicit;
- `by` tactic blocks and term-mode proofs;
- Unicode and ASCII spellings for every operator (`→`/`->`, `∀`/`forall`, `λ`/`fun`, `⁻¹`/`inv`).

`[PROOF-SURFACE-DOCS]` Documentation comments, executable examples and the HTML documentation site of [spec 0026](0026-DocumentationComments.md) apply to dependent modules unchanged.

## Elaborator [PROOF-ELAB]

The elaborator turns surface declarations into kernel terms. It is untrusted: every term it produces is re-checked by the kernel before it enters the environment.

- `[PROOF-ELAB-UNIFY]` higher-order pattern unification with metavariables, postponed constraints and delayed assignments, matching Lean 4's behaviour on Mathlib's sources;
- `[PROOF-ELAB-CLASSES]` type-class resolution by tabled search over instances with priorities and outParams, with a per-declaration instance cache; Mathlib's algebraic hierarchy is the acceptance corpus;
- `[PROOF-ELAB-COERCE]` coercions, `↑`, numeric literals through `OfNat`, and `@[simp]`/`@[ext]`-style attributes carried in the environment;
- `[PROOF-ELAB-MACROS]` hygienic notation and macro expansion in Osprey, so Mathlib's notations are declared in the ported library and not in the compiler.

## Tactics and automation [PROOF-TACTICS]

Tactics are Osprey programs over a goal state, written against a metaprogramming API that exposes the environment, metavariables and the kernel.

`[PROOF-TACTICS-CORE]` Shipped before the port claims any file: `intro`, `apply`, `exact`, `refine`, `rw`, `simp`, `cases`, `induction`, `rcases`, `obtain`, `constructor`, `use`, `exists`, `ext`, `funext`, `calc`, `have`, `show`, `by_contra`, `by_cases`, `omega`, `decide`, `norm_num`, `ring`, `linarith`, `nlinarith`, `positivity`, `field_simp`, `aesop`, `tauto`, `gcongr`, `abel`, `group`, `push_cast`, `norm_cast`, `exact?`, `apply?`, `simp?`. A proof in the ported library that names a tactic not on this list is a port blocker, not a reason to add a stub.

`[PROOF-TACTICS-CERT]` External solvers (SMT through Z3 or cvc5, superposition through E or Vampire) are called through `hammer`. A solver answer is reconstructed into a kernel term; an answer that cannot be reconstructed is reported as a hint, and the goal stays open.

`[PROOF-TACTICS-GUIDE]` A learned premise-selection and tactic-suggestion model may propose steps. It is a proposer under [PROOF-TACTICS-CERT]; the trust report never counts a proposal as a proof.

`[PROOF-TACTICS-BENCH]` Automation is measured, not asserted: `benchmarks/proof/` runs miniF2F and PutnamBench under a fixed time budget against the current Lean release with its standard automation, and records both scores with the commit hashes. The website quotes only recorded results.

## Mathlib port [PROOF-MATHLIB]

The port is the package `@osprey/mathlib`, Apache-2.0 with Mathlib's notices, built in three layers that each remain valid on their own:

1. `[PROOF-MATHLIB-CHECKED]` the imported, kernel-checked environment of the pinned Mathlib commit. Every declaration is available to dependent modules by its Lean name from the first release;
2. `[PROOF-MATHLIB-SOURCE]` statements, definitions, structures, instances and notations translated by `osprey prove port` into readable `.ospml` files mirroring Mathlib's module tree, with each theorem's proof initially the imported term, marked `by imported`;
3. `[PROOF-MATHLIB-REPROVED]` proofs re-elaborated from translated tactic scripts, or found by automation, replacing `by imported`. The report records the fraction re-proved per module.

`[PROOF-MATHLIB-UPSTREAM]` The bridge is two-way. The port depends on Mathlib remaining Apache-2.0 and exportable through `lean4export`, and the sync job and the re-proved fraction both move faster when fixes land upstream. So: a defect found in a Mathlib statement, proof or notation during porting, syncing or adversarial testing is reported to Mathlib with the Osprey evidence, and fixed upstream first where Mathlib accepts it; the port carries a local patch only while the upstream fix is pending, listed in the trust report with its upstream link. A port-only fork of a Mathlib file is a drift under [PROOF-MATHLIB-DRIFT], not a feature. Kernel findings about Lean itself ([PROOF-TRUST-LESSONS]) are disclosed to the Lean maintainers before publication, with the dated report following the fix.

`[PROOF-MATHLIB-DRIFT]` The port never diverges in a statement: a translated statement must elaborate to a term definitionally equal to the imported one, checked by the kernel on every sync. A statement that cannot be translated stays available under layer 1 and is listed as untranslated.

## Erasure and code generation [PROOF-ERASE]

A dependent module's runnable definitions compile through the existing backend. Erasure maps kernel terms to the canonical AST:

- propositions, proofs, universe levels and type arguments erase;
- `Nat` and `Int` erase to the arbitrary-precision integer type of [PROOF-STDLIB]; a definition whose result is proved to fit `int` may erase to `int` under an explicit `@[erase int]` attribute, and the proof obligation is checked, never assumed;
- structures erase to records, inductives to unions, recursors to `match`;
- a definition using an axiom (including `Classical.choice`) is not runnable and has no erasure.

The erased program obeys every rule of the HM pipeline, including [FAILURE-EXPLICIT] and the memory backends of [spec 0018](../compiler/0018-MemoryManagement.md).

## Standard-library prerequisites [PROOF-STDLIB]

The kernel, elaborator and tactics are Osprey programs, so the standard library of [spec 0012](../runtime/0012-Built-InFunctions.md) must first provide, for every target and under GC and ARC:

- `bigint`: arbitrary-precision integers with the operations of [PROOF-KERNEL-THEORY], total, with `Result`-returning division;
- `Map<K, V>` with structural keys beyond `string`, and a content-addressed term store so that equal terms share one representation (hash-consing);
- a mutable `Array<T>` and a `Cell<T>` with the ownership rules of [MODULES-STATE](0025-ModulesAndNamespaces.md#state-ownership-modules-state), for unification tables and caches;
- a `Nat`-literal and `String`-literal representation shared with the kernel's accelerated operations.

These are ordinary library additions, available to every Osprey program, and are specified and tested in spec 0012 before the kernel uses them.

## Tooling [PROOF-TOOLS]

- `osprey prove check <file|project>` elaborates and kernel-checks dependent modules; exit 0 on success, 1 with diagnostics naming the declaration and goal otherwise.
- `osprey prove import`, `osprey prove port`, `osprey prove bench` as above.
- The language server of [spec 0020](../tooling/0020-LanguageServerAndEditors.md) shows the goal state at the cursor inside a `by` block, hovers the type of any term, and offers `exact?` results as code actions.

## Verification [PROOF-VERIFICATION]

A phase of plan 0041 is complete only with the evidence named there. The gates, in order: `bigint` and the term store benchmarked; Lean's own core library checked; Mathlib checked and cross-checked; the surface language elaborating Mathlib's `Mathlib/Algebra/Group/Basic.lean` from translated source; the core tactic list passing its Mathlib-derived test corpus; the automation benchmark recorded; a dependent module calling into and being called from an HM module in the differential corpus under both memory backends and on wasm32.
