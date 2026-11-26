# Proof kernel and Lean bridge [PROOF-KERNEL-SPEC]

The trusted kernel that dependent disciplines share, and the bridge that lets Osprey check proofs written in Lean. What dependent types are, why they sit beside Hindley-Milner rather than inside it, and which disciplines exist today is [spec 0049](0049-DependentTypesAndProofs.md); how a discipline plugs in is [spec 0050](0050-TypeDisciplines.md). Delivery of the Lean-compatible target and the Mathlib port: [plan 0041](../../plans/0041-dependent-types-and-mathlib.md).

## First goal: soundness, trust, verifiability [PROOF-GOAL]

The only acceptable standard is soundness: no closed term of type `False` is derivable in the empty context, and a proposition is accepted as proved only when a kernel derivation of it exists. There is no tolerance for inconsistency, however small or obscure. Every accepting judgement is reproducible by an independent checker from the kernel's exported derivation, so trust rests on verification and never on the implementation.

Where Lean's theory or its implementation admits an inconsistency, Osprey must not inherit it. The kernel is written against a theory stated as inference rules ([PROOF-KERNEL]), not against Lean's code; any rule, reduction or extension whose soundness is in question is isolated behind an abstraction so it can be disabled, replaced or proved sound without touching the rest ([PROOF-TRUST]). Compatibility with Lean is a feature of the bridge; soundness is a property of the kernel, and when the two conflict, soundness wins.

**Status:** normative target; **implementation in progress with checked library exports already running**. The 2026-10-30 evidence includes 175 selected Lean-core statements and seventeen selected OpenAI supporting lemmas under GC, ARC and wasm32, with fresh Lean replay and proof/statement mutation refusals. The historical 325-case kernel checkpoint predates the retained nested-inductive failure; it does not establish that the current expanded suite is green. Three compatible core proofs preserve their original statements while the original nested-inductive certificates remain refused. M0 through M2 and recorded M3 inductive cases are delivered; the full Mathlib port, remaining inductive support and broader frontend remain open. See the [current port evidence and videos](https://www.ospreylang.dev/docs/mathlib-port/). Delivery: [plan 0041](../../plans/0041-dependent-types-and-mathlib.md). Nothing in this contract changes the Hindley-Milner type system of [spec 0004](0004-TypeSystem.md); every rule here adds a second, selectable type discipline beside it, through the pluggable-discipline mechanism of [spec 0050](0050-TypeDisciplines.md).

## Lean as a development oracle [PROOF-ORACLE]

Lean supplies the executable reference for the initial compatible discipline: pinned source, statements, certificates, axiom reports and fresh kernel replay. Osprey implements and runs its own checker. The capability milestones lead to independent proof authoring, automation, library reconstruction and verified programs; subsequent disciplines can add different theories or axiom policies. The [messaging contract](../../messaging.md#proof-development-and-the-lean-oracle) defines this cooperative relationship, and [plan 0041](../../plans/0041-dependent-types-and-mathlib.md#completion-milestones-proof-completion) defines the completion evidence.

Compatibility concerns intended logical rules and valid certificates, not reproduction of implementation defects. Lean acceptance alone cannot authorize acceptance of a known unsound proof. Distinguish a kernel bug from an inconsistent assumption set or a deliberate theory change. Retain each known unsound case as a rejection test, investigate oracle disagreements and preserve independently reviewed evidence instead of copying the defective behaviour.

The compatible theory remains a versioned package under spec 0050. An implementation repair requires fresh validation of affected proofs. Removing an axiom or changing inference/equality rules creates an explicit policy/theory revision or separately named fork; its certificates, caches and library dependencies must be rechecked under that identity. Such a fork does not inherit its parent's proof authority. Unsupported proofs remain refused, and pluggability alone establishes no consistency theorem.

### Translation validation of compiler output [PROOF-ORACLE-COMPILER]

For a theorem about compiled code, record the compiler input, the emitted function, the quantified inputs and the contracts assumed for external calls. Derive the theorem input from the output produced during the proof run. A reflector must account for every reachable instruction that can affect the stated property. Unrecognized operations on that dependency path must prevent certification. At a control-flow join, check all feasible predecessors or prove the property over an overapproximation that includes them all.

Check the theorem in pinned Lean and replay its export with the Osprey kernel under the declared axiom policy. Compare the exact set of reported axiom names for each theorem independently of export order. Require a semantic mutation that survives translation and fails at the theorem. Record Lean execution proofs and Osprey-replayed operand proofs separately when the two check different claims. Record the boundary of each result, including trusted extraction and runtime semantics. The ownership spec identifies the functions, properties, mutation controls and unfinished proof obligations. [Ownership compiler proofs](../compiler/0018-MemoryManagement.md#proofs-of-compiled-ownership-code-mem-ownership-static-verification-compiler)

## Kernel [PROOF-KERNEL]

The kernel is the only trusted component. It is a type checker for terms of Lean 4's type theory, written in Osprey, compiled through the native backend, and small enough to audit: under 5,000 lines of Osprey, with a published prose specification of every judgement. The kernel's coverage is defined by an inventory of every declaration form, typing judgement and reduction rule required by the pinned Lean core and `Std`, each with an implementing function, a positive example and a rejection control. The inventory is not complete yet; it states the completion requirement, not what the kernel checks today.

`[PROOF-KERNEL-THEORY]` The compatible profile implements the specified rules of Lean 4's type theory so valid exported proof terms check without translation. Erroneous implementation behaviour is excluded by [PROOF-ORACLE]:

- a cumulative-free universe hierarchy `Sort u` with level expressions (`zero`, `succ`, `max`, `imax`, parameters) and universe-polymorphic constants;
- `Prop` as `Sort 0`, impredicative, with definitional proof irrelevance;
- dependent function types, application, lambda and `let`, with β, δ (unfolding by reducibility hint height), ζ, η for functions and for structures, and ι reduction;
- inductive types in the kernel's primitive form (parameters, indices, constructors, the generated recursor and its reduction rules), with the nested and mutual encodings Lean's front end produces, and the positivity and universe checks Lean performs;
- quotient types as the four primitives `Quot`, `Quot.mk`, `Quot.lift`, `Quot.ind` and the `Quot.sound` reduction;
- natural-number and string literals with the accelerated `Nat` operations Lean's kernel special-cases (`add`, `sub`, `mul`, `div`, `mod`, `gcd`, `beq`, `ble`, `land`, `lor`, `xor`, `shiftLeft`, `shiftRight`, `pow`, `log2`), computed on arbitrary-precision integers;
- declarations: axioms, definitions, theorems, opaque constants, with the environment checks on names, levels and free variables that Lean applies before adding each.

When the head of an application reduces through a transparent definition or a projection, the reducer keeps the pending arguments attached. An arithmetic method reached through a type class can thereby use the same guarded numeric reduction as a direct call. The native worklist growth theorem exercises ordinary multiplication at the 64-bit allocation boundary. [Compiled ownership proofs](../compiler/0018-MemoryManagement.md#proofs-of-compiled-ownership-code-mem-ownership-static-verification-compiler)

Under [PROOF-KERNEL-THEORY], declaration metadata is compared structurally, never through pretty-printed descriptions, joined names or hashes alone. Name lists preserve their length, order and exact elements, so one identifier containing a comma cannot stand for two identifiers. Family and constructor counts, flags and ownership are compared as fields; recursor rules retain their order, constructor identity and field counts, with their types and right-hand sides checked by the existing derivation rules. The shared `DeclarationBodies` comparison in `src/osprey/examples/proof/kernel/src/env.ospml` serves both import validation and numeric/notation identity guards. Rendered text is diagnostic output only. Genuine identifiers containing commas remain legal; rejecting punctuation is not a substitute for comparing structure. The adversarial cases in `src/osprey/examples/proof/kernel/tests/inductive_adversarial.ospml` must reject forged constructor inventories and recursor-rule lists even when their printed descriptions collide.

`[PROOF-KERNEL-TOTAL]` The kernel is an Osprey program and so obeys [FAILURE-EXPLICIT](0013-ErrorHandling.md): every rejection is a typed `Result` naming the declaration, the judgement and the two terms that failed to be definitionally equal. It never panics, aborts or exits.

`[PROOF-KERNEL-AXIOMS]` Each checked theorem records the axioms it depends on. The report of [PROOF-TRUST] lists them; Mathlib's are `propext`, `Quot.sound` and `Classical.choice`.

## Trust [PROOF-TRUST]

Trust claims name the checker, intended rules, assumptions and independently replayable evidence. Lean is a development oracle under [PROOF-ORACLE], not a source of unquestionable acceptance.

1. **Specification.** The kernel's judgements are written as prose and inference rules in this contract's companion document before the code, and every kernel function names the rule it implements.
2. **Cross-checking.** Every release re-checks the same Lean export with the Osprey kernel and with at least one independent checker of Lean's theory (`lean4lean` or `nanoda`), with agreement required on the intended compatible fragment. A disagreement blocks the release until explained in the report. Known unsound certificates remain mandatory rejections even when an affected reference release accepts them; their original reproduction, corrected rule and Osprey rejection must remain explicit tests. No declaration is silently skipped.
3. **Report.** The [evidence section of plan 0041](../../plans/0041-dependent-types-and-mathlib.md#evidence) records the Mathlib commit, the export hash, declarations checked, wall time, the axioms used, and the cross-check result. The website's claim is a link to the latest report.
4. **No trusted tactics.** A tactic, an SMT solver or a learned guide proposes terms; only the kernel accepts them ([PROOF-TACTICS-CERT]).

`[PROOF-TRUST-LESSONS]` The [wrong-structure projection exploit in Lean issue 14576](https://github.com/leanprover/lean4/issues/14576) supplies a concrete rejection fixture. Each additional published exploit requires its own primary report, affected/fixed revision and reproducer before attribution. The following are requirements, not claims that every regression or isolation mechanism is already implemented:

- **Projections are checked against their structure.** A projection `S.i` type-checks only when its argument's whnf is an application of `S` itself, including inside nested and mutual inductive declarations. Pin the published axiom-free proof of `False` and require its rejection in `tests/proof/adversarial/`; existing projection controls alone do not close that fixture requirement.
- **A hash is never an identity.** The term store of [PROOF-STDLIB] resolves a hash collision by structural comparison; no kernel judgement reads two terms as equal because their hashes agree. A test plants a collision and proves the kernel still distinguishes the terms.
- **No metaprogram touches the kernel's state.** The kernel is a pure function from (environment, declaration) to `Result`; the elaborator, tactics and any plugin hold no reference to its data and cannot add a declaration except through that function. The discipline process boundary isolates the compiler, but does not itself establish kernel soundness: the published projection exploit used Lean's checked declaration path. Test every declaration entry path against the same judgement.
- **No counter can overflow into unsoundness.** Allocation, reference counts and arithmetic used by the checker require explicit overflow tests and failure behaviour. Check the kernel under GC and ARC; a managed backend or an [ARITH-EFFECT](0037-ArithmeticEffects.md) policy alone does not establish that the runtime and all checker counters are safe.
- **Cross-checkers are pinned and reviewed.** [PROOF-TRUST] step 2 records the exact independent checker revision and its applicable known defects. Upgrade and replay when a relevant soundness fix lands. Agreement between two checkers is evidence, not a consistency proof.
- **Adversarial testing is continuous.** `tests/proof/adversarial/` grows with every published kernel exploit of any proof assistant, and an AI-driven fuzzing job proposes malformed declarations against the kernel on a schedule, reporting any acceptance as a showstopper.

## Measurement [PROOF-SPEED]

Timing is a record, never a gate, and never a claim. There is no "faster than Lean" requirement anywhere in this contract or in CI.

- **File to verdict.** A measurement runs one unmodified `.lean` file from source to verdict on both sides: the pinned Lean toolchain (`lake build` or `lean`), and `osprey prove check` on the same file. Both are cold processes on the same machine in the same run; the report records wall time, CPU time and peak memory for each, with the hashes of the file and both binaries. Nothing else is comparable and nothing else is reported.
- **Not in CI.** `src/toolchain/benchmarks/proof/` runs by hand. Reports under `target/proof/reports/` state the numbers and the inputs and nothing more; public prose reports scope and measurements, not positioning.
- **What is measured later.** Term-store hash-consing, memoised definitional equality, accelerated `Nat` literals and any GPU offload are line items in the same report once they exist. A handler that is not faster than its CPU twin is not used.

## Lean export bridge [PROOF-EXPORT]

`[PROOF-EXPORT-NAMES]` The current reader preserves each name's component boundaries and distinguishes string components from numeric components. It joins components with dots, omits the anonymous prefix and JSON-quotes ambiguous string components, including dotted, empty and decimal strings. Thus namespace `A.B`, single-component `«A.B»`, numeric `5` and string `"5"` stay distinct. The anonymous root is permitted as a prefix or binder name, never as a declaration name. The current JSON decoder bounds numeric name components to 0 through 9,223,372,036,854,775,807 and refuses NUL in strings. Larger numeric components are an explicit unsupported input; the independent corpus validator applies the same limits.

`[PROOF-EXPORT-SAFETY]` The reader refuses an explicit unsafe declaration flag or a definition safety mode other than `safe`. Universe `max` and `imax` arrays require exactly two operands; additional operands are errors. Assumption receipts use `none` only for an empty list. Nonempty entries that are empty, named `none`, or contain commas or quotes are JSON-quoted, so one identifier cannot impersonate an empty list or several assumptions. Ordinary assumption names retain their existing receipt spelling.

`osprey prove import <export-file>` reads the `lean4export` NDJSON format (version 3.1.0 as the pinned exporter writes it: names, levels, expressions, declarations, inductive specifications and quotient initialisation), adds each declaration to an Osprey environment through the kernel, and writes a checked environment file. The importer is the only code that parses Lean's format; it adds nothing to the theory.

`[PROOF-EXPORT-SYNC]` A scheduled job imports the current Mathlib master on a fixed cadence, records the result as a trust report, and opens a pull request that updates the ported library of [PROOF-MATHLIB]. A failing import is a red report, never a silent skip.

## Lean source [PROOF-SOURCE-LEAN-FILES]

The Lean-compatible target, `lean4`: a dependent discipline whose source is Lean itself. Not implemented yet.

`[PROOF-SOURCE-LEAN]` **The `lean4` discipline reads `.lean`.** Its proof file is a Lean 4 source file exactly as the pinned toolchain (Lean v4.25.0, Lake, Mathlib at its pin) accepts it: same syntax, same elaboration, same tactics, same imports, same `lakefile`. Osprey adds no keyword, marker, header or dialect to a `.lean` file. A file Lean accepts, `lean4` accepts; a file Lean rejects, `lean4` rejects, with the same error at the same position. Lean's own test suite, `Init`, `Std` and Mathlib are the acceptance corpus, in that order, and the verdicts must match declaration for declaration. Until it exists, an Osprey module relies on a Lean theorem through the `leanexport` discipline of [spec 0049](0049-DependentTypesAndProofs.md#what-exists-today), which re-checks the theorem's pinned `lean4export` certificate rather than reading `.lean` source.

`[PROOF-SOURCE-STAGES]` The `lean4` discipline arrives in three stages, each a shipped product and each gated on the corpus above ([plan 0041](../../plans/0041-dependent-types-and-mathlib.md)):

1. **Toolchain stage.** `osprey prove check Foo.lean` runs the pinned Lean toolchain on the file, exports it through the pinned `lean4export`, and checks the export with Osprey's kernel ([PROOF-KERNEL]). Goals, hovers and errors come from Lean's language server. The toolchain is a bundled or required dependency; without it there is no verdict, and the tool says so.
2. **Front-end stage.** Osprey reads `.olean` files and runs Lean's compiled front end (parser, elaborator, tactics, Mathlib's metaprograms) itself through an interpreter for Lean's IR, with no Lean executable installed. Verdicts and exported terms are byte-identical to stage 1.
3. **Native stage.** Hot paths (the kernel already, then the elaborator, then individual tactics) are replaced by Osprey implementations one at a time. Each replacement must give identical results on the whole corpus or it is not switched on.

`[PROOF-SOURCE-EXTENSION]` Anything that is not a proof (lesson text, media, prompts, hints, progress) lives in one extension format beside the `.lean` file ([spec 0052](../applications/0052-WorkbenchLessons.md)) and refers to declarations in the `.lean` file by name. The extension never contains proof text of its own.

## Elaboration and tactics [PROOF-ELAB]

Elaboration and tactics are Lean's. At stage 1 the toolchain runs them; at stage 2 Osprey runs Lean's compiled front end; at stage 3 an Osprey replacement for a unifier, type-class search, a coercion rule, a macro or a tactic may be switched on only when it reproduces Lean's result on the whole corpus. The kernel re-checks every produced term before it enters the environment; nothing above the kernel is trusted.

`[PROOF-TACTICS-CERT]` External solvers (SMT through Z3 or cvc5, superposition through E or Vampire) are called through a `hammer` tactic. A solver answer is reconstructed into a kernel term; an answer that cannot be reconstructed is reported as a hint, and the goal stays open.

`[PROOF-TACTICS-GUIDE]` A learned premise-selection and tactic-suggestion model may propose steps. It is a proposer under [PROOF-TACTICS-CERT]; the trust report never counts a proposal as a proof.

`[PROOF-TACTICS-BENCH]` Automation is measured, not asserted: `src/toolchain/benchmarks/proof/` runs miniF2F and PutnamBench under a fixed time budget, by hand, and records the scores with the commit hashes. The website quotes only recorded results.

## Mathlib [PROOF-MATHLIB]

`[PROOF-RUNNABLE-LIBRARY]` Delivery proceeds through runnable library theorems. Each selected theorem has a pinned Lean export containing its complete dependency closure, checked from an empty Osprey environment without skipping or assuming unsupported declarations. Acceptance requires the intended statement checked structurally, its actual proof checked by the Osprey kernel, its axiom dependencies reported, and corrupted-proof and changed-statement controls rejected. The exact command and output run under GC, ARC and wasm32. A successful common theorem is evidence for that theorem and its dependencies; larger targets such as Navier–Stokes retain their own unmet requirements.

Mathlib is used as Mathlib: the pinned commit's `.lean` files, Apache-2.0 with its notices, built through Lake at stage 1 and by Osprey at stage 2. There is no translation, no `@osprey/mathlib` rewrite and no re-proving. The kernel-checked environment of the pinned commit (`[PROOF-MATHLIB-CHECKED]`) is the one deliverable: every declaration available by its Lean name, every proof Osprey-checked, every axiom reported. Two ledgers close that deliverable, neither implemented yet: checked-export coverage, closed only when every original required declaration and dependency in the pinned export passes the compatible kernel; and source build, closed only when every module of the pinned Mathlib tree builds from its unmodified `.lean` sources. Only the first ledger establishes full imported-certificate coverage; both are required to close the library completion milestone. Counts always use the fixed denominator; no sample, excluded directory or alternate body silently shrinks it.

`[PROOF-MATHLIB-UPSTREAM]` The bridge is two-way. A defect found in a Mathlib statement, proof or notation during checking, syncing or adversarial testing is reported to Mathlib with the Osprey evidence and fixed upstream; a local patch is carried only while the upstream fix is pending, listed in the trust report with its upstream link. A fork of a Mathlib file is never a feature. Kernel findings about Lean itself ([PROOF-TRUST-LESSONS]) are disclosed to the Lean maintainers before publication, with the dated report following the fix.

## Standard-library prerequisites [PROOF-STDLIB]

The kernel, elaborator and tactics are Osprey programs, so the standard library of [spec 0012](../runtime/0012-Built-InFunctions.md) must first provide, for every target and under GC and ARC:

- `bigint`: arbitrary-precision integers with the operations of [PROOF-KERNEL-THEORY], total, with `Result`-returning division;
- `Map<K, V>` with structural keys beyond `string`, and a content-addressed term store so that equal terms share one representation (hash-consing);
- a mutable `Array<T>` and a `Cell<T>` with the ownership rules of [MODULES-STATE](0025-ModulesAndNamespaces.md#state-ownership-modules-state), for unification tables and caches;
- a `Nat`-literal and `String`-literal representation shared with the kernel's accelerated operations.

These are ordinary library additions, available to every Osprey program, and are specified and tested in spec 0012 before the kernel uses them.

## Tooling [PROOF-TOOLS]

- `osprey prove check <file.lean | Lake project>` gives Lean's verdict on the file through the current stage of [PROOF-SOURCE-STAGES] and Osprey's kernel verdict on its export; exit 0 when both accept, 1 with Lean's diagnostics otherwise. Imports and `lakefile` dependencies resolve as Lake resolves them.
- `osprey prove import <export>` reads a `lean4export` file into a checked environment, as above. `osprey prove bench` records a file-to-verdict measurement ([PROOF-SPEED]).
- The language server of [spec 0020](../tooling/0020-LanguageServerAndEditors.md) forwards `.lean` files to Lean's language server at stage 1 and serves them itself from stage 2: goal state at the cursor, hover types, `exact?` as code actions.

### Proof workbench [PROOF-WORKBENCH]

The workbench's full contract is [spec 0051](../applications/0051-ProofWorkbench.md); this section states what the proof system owes it and what it may not have.

The workbench is an Osprey Native application ([`@osprey/native`](../../../src/osprey/packages/native/README.md)) that opens, edits, checks, reads and saves `.lean` files. The document is the `.lean` file and nothing else is saved. Goals, errors and verdicts come from the services of [PROOF-TOOLS]; the kernel verdict comes from Osprey's kernel in-process over the file's export. Lessons ([spec 0052](../applications/0052-WorkbenchLessons.md)) teach Lean itself, with their challenges and solutions as declarations in `.lean` files.

- **Nothing special for the workbench.** Every service it uses is a capability of [PROOF-TOOLS] or a host capability of `@osprey/native`; a second editor gets the same services.
- **Reads like a paper.** A rendered view lays the file out as a page: statements and goals typeset from Lean's pretty-printer output, proofs collapsible, the verdict beside each declaration. The rendered view is the export.
- **Every host.** One Osprey application over `@osprey/native`: native macOS and iOS, VS Code through `@osprey/vscode`, and the browser at wasm32. A `.lean` file opened in any host is the same file `osprey prove check` checks.
- **Beautiful, or not shipped.** Typeset mathematics in a maths face; light and dark through [UI-THEME]; nothing that flickers while checking; a screenshot beside a printed page in every release review.

The workbench is measured by one test: a mathematician who has never used Osprey opens a fresh `.lean` file, writes and checks `and_swap` in it in under ten minutes without reading documentation, recorded in the road-test report.

## Verification [PROOF-VERIFICATION]

A phase of plan 0041 or 0044 is complete only with the evidence named there; no milestone is complete because a plan, demonstration, benchmark result or badge exists on its own. The gates, in order: Lean's own test suite giving identical verdicts through `osprey prove check`; Lean's core library checked; Mathlib checked and cross-checked; `Mathlib/Algebra/Group/Basic.lean`, unmodified, checked at stage 2 with no Lean executable present; the automation benchmark recorded; a dependent module calling into and being called from an HM module in the differential corpus under both memory backends and on wasm32.
