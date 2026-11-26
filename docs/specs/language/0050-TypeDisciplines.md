# Pluggable type disciplines [TYPES-DISCIPLINE]

**Status:** **implemented and gated except where marked**: the marker, the registry, the checker process with one deadline over build, spawn, request and response, protocol version 2 with imported signatures, `omitted[]` and checker-declared `dependencies`, keyed binary and result caches, outline-only parsing of disciplined sources, erasure ascribed to the written signature, and four reference disciplines (`passthrough`, `bounded`, `dependent`, `leanexport`) under GC, ARC and wasm32 in `make _test_disciplines`. Open: exported types and values rendered in an inferred `hm` signature (named in `omitted[]` today), the markerless byte-identity gate, and theory evolution below. Delivery: [plan 0061](../../plans/0061-pluggable-type-disciplines.md). Dependent disciplines: [spec 0049](0049-DependentTypesAndProofs.md). This contract stands on its own: it is the mechanism by which any type system other than Hindley-Milner checks an Osprey module, and it is complete before any such system exists.

## Aim `[TYPES-DISCIPLINE-AIM]`

Osprey has one type system today, the Hindley-Milner discipline of [spec 0004](0004-TypeSystem.md), and it stays the type system of every Osprey program. This contract lets a **module** opt into a different checker, called a **discipline**, without the compiler learning what that checker does. The flavors of [spec 0023](0023-LanguageFlavors.md) made syntax pluggable above one canonical AST; disciplines make typing pluggable above the same AST and the module signatures of [spec 0025](0025-ModulesAndNamespaces.md). Three consequences:

1. A dependent type theory, a refinement-type checker over an SMT solver, a higher-order logic, a separation logic or a linear type system is a **package**, written in Osprey, published like any other, and chosen per module.
2. Modules under different disciplines link into one program. They meet at the signature boundary; nothing below the canonical AST knows which discipline checked a module.
3. The compiler never grows a second type system. `src/toolchain/crates/osprey-types` gains one dispatch point and no theory.

## Design principles from the literature `[TYPES-DISCIPLINE-PRINCIPLES]`

These rules are normative. Each states the research it comes from, how Osprey meets it, and where it is tested or still open.

1. `[TYPES-DISCIPLINE-PRINCIPLES-OPTIONAL]` **An optional type system never changes what a program means.** Pluggable types are layered over a language whose run-time semantics do not depend on them [Bracha 2004]; practical checkers run as separate plug-ins over one host type system [Papi et al. 2008]. Osprey: a discipline only accepts or rejects and returns an erased module, and a program with no marker is compiled exactly as before ([TYPES-DISCIPLINE-NO-HARM]). Open: the markerless byte-identity gate ([plan 0061](../../plans/0061-pluggable-type-disciplines.md#open)).
2. `[TYPES-DISCIPLINE-PRINCIPLES-KERNEL]` **One small trusted checker; everything else proposes.** Only a small kernel's acceptance counts, and its results should be re-checkable by an independent implementation [Gordon, Milner and Wadsworth 1979; Barendregt and Wiedijk 2005]. Lean 4 keeps a small kernel and exports terms for external checkers [de Moura and Ullrich 2021], and an independent checker for Lean 4's kernel now exists [Carneiro 2024]. Osprey: a checker is a separate process; proof disciplines share one kernel ([spec 0055](0055-ProofKernelAndLeanBridge.md)); the erased module is re-checked by HM, so a faulty checker can accept a false theorem but cannot hand the backend an ill-typed program.
3. `[TYPES-DISCIPLINE-PRINCIPLES-CHECK]` **Check against declared types at the boundary; do not infer across it.** Dependent types lose decidable inference and principal types [Wells 1999; Huet 1975]; the standard remedy is bidirectional checking against stated types [Pierce and Turner 2000; Dunfield and Krishnaswami 2013, 2021]. Osprey: modules of different disciplines meet only at declared HM signatures, and the erased module is checked against the signature its checker wrote.
4. `[TYPES-DISCIPLINE-PRINCIPLES-RIGID]` **Declared type variables are rigid.** An annotation's type variables are skolems, so a body that fixes or merges one is less general than its declaration [Peyton Jones, Vytiniotis, Weirich and Shields 2007; Dunfield and Krishnaswami 2013]. Without this a boundary signature can promise more than its code delivers. Osprey: [TYPE-GENERICS-FN](0004-TypeSystem.md), tested in `src/toolchain/crates/osprey-types/src/generics_decl_tests.rs` and `src/osprey/examples/failscompilation/generic_binder_less_general.ospo`.
5. `[TYPES-DISCIPLINE-PRINCIPLES-ERASE]` **Erase only what cannot be used at run time, and say why.** Extraction removes proofs and types [Paulin-Mohring 1989; Letouzey 2002]; indices need not be stored [Brady, McBride and McKinna 2003]. Modern dependent languages make run-time relevance part of the type, with quantity 0 marking what is erased [McBride 2016; Atkey 2018; Brady 2021]. Osprey: an erased item vanishes from the erased module, so code that still names it fails the HM check of that module; an erased item can never be silently used. A discipline whose erasure is not line-based must state its relevance rule.
6. `[TYPES-DISCIPLINE-PRINCIPLES-PURE]` **Proofs are pure; effects stay on the HM side.** Dependent types and effects meet safely only when specifications cannot perform effects [Swamy et al. 2016]. Osprey: proofs are erased before effect checking, and the erased module is effect-checked by HM against its signature's rows ([TYPES-DISCIPLINE-BOUNDARY]).
7. `[TYPES-DISCIPLINE-PRINCIPLES-SPECTRUM]` **Refinement and dependent types are different points on one line, and both must plug in.** Refinement types over a decidable logic keep inference automatic [Freeman and Pfenning 1991; Rondon, Kawaguchi and Jhala 2008; Vazou et al. 2014]; full dependent types need proofs. Osprey: `bounded` (refinement) and `dependent` and `leanexport` (dependent) use the same seam ([TYPES-DISCIPLINE-REFERENCE]).
8. `[TYPES-DISCIPLINE-PRINCIPLES-IDENTITY]` **A theory has an identity, and changing it re-checks.** A fork that changes axioms or rules is a different theory and inherits no verdicts ([TYPES-DISCIPLINE-EVOLUTION]). Osprey: results are cached by request, checker binary and every file the checker read; keying by theory identity is open ([plan 0061](../../plans/0061-pluggable-type-disciplines.md#open)).

## Definitions `[TYPES-DISCIPLINE-DEF]`

A **discipline** is a checker that, given a module and the signatures of everything it imports, either rejects the module with diagnostics or accepts it and produces two artifacts:

- an **export signature**: a `signature` block in the sense of [MODULES-SIGNATURE](0025-ModulesAndNamespaces.md#signatures-modules-signature), typed entirely in HM types, describing every item the module exposes to other modules;
- an **erased module**: Osprey source, in any flavor, that implements that signature and contains nothing the HM pipeline does not already accept.

The compiler ships one discipline and knows the shape of all of them:

| Discipline | Marker | Checker | Export signature | Erased module |
| --- | --- | --- | --- | --- |
| `hm` (default) | none, or `// osprey: types=hm` | `osprey_types::check_program`, in process | inferred, as today | the module itself |
| any other | `// osprey: types=<name>` | the package registered under `<name>`, as a separate process | written by the checker | written by the checker |

`hm` is not special-cased anywhere except that it has no process boundary and no erasure step; it is the identity discipline.

## Theory identity and correction `[TYPES-DISCIPLINE-EVOLUTION]`

This is a delivery target, expanded in [plan 0041 milestone 2](../../plans/0041-dependent-types-and-mathlib.md#milestone-2-replace-and-fork-disciplines-safely-proof-completion-disciplines). The generic seam is implemented; the complete identity, fork and invalidation acceptance described here remains required evidence.

A discipline can be replaced or forked through the registry without embedding its theory in the compiler. Its artifact identity includes package/version, checker binary, rule revision, axiom policy and dependency signatures. Changes invalidate affected caches and proof receipts transitively. Rebuilding under a different discipline must actually recheck the module; an old Checked result cannot be relabelled or reused across incompatible identities.

A checker implementation fix, an axiom-policy change and a change of logical rules are distinct events. The latter two require explicit profiles or separately named forks, with recorded migration results and fresh validation. Proofs using a withdrawn assumption remain unavailable until reconstructed. Not implemented yet — no restricted-axiom fork exists; its concrete controls are: removing `Quot.sound` must refuse the current `OAI.PrefixFlows.Interspersed.push_inj` certificate, while the axiom-free `Nat.add_comm` certificate still checks. Cross-discipline proof use needs a checked translation or an explicit interface contract; the HM boundary does not transfer unexpressed guarantees.

Lean is the reference oracle for the first compatible theory, not a mandatory dependency of every discipline or an authority that overrides Osprey's soundness rules. Known unsound oracle behaviour must have a rejection regression. Every fork documents its intended rules and assumptions; switching disciplines is a correction mechanism, not evidence by itself that a theory is consistent. See the [proof messaging contract](../../messaging.md#proof-development-and-the-lean-oracle).

## Registry `[TYPES-DISCIPLINE-REGISTRY]`

A discipline is a package ([spec 0029](../ecosystem/0029-PackageManagement.md)) whose manifest declares one table:

```toml
[package]
name = "@osprey/lean4"

[discipline]
name = "lean4"                   # the word after `types=`
checker = "src"                  # an Osprey program: one source file, or a project directory
                                 # with its own osprey.toml whose entry is the checker
```

- The compiler holds no list of discipline names, no knowledge of any theory, and no code that runs only for one discipline. A CI test (`src/toolchain/crates/osprey-project/tests/discipline_purity.rs`) scans the code under `src/toolchain/crates/` for every registered discipline name in a string literal and for the words `tactic`, `theorem`, `refinement` and `lean4` anywhere, and fails on a hit outside comments and test modules. `kernel` is not searched because it is the keyword of the `kernel` handler region of spec 0035 and names the GPU pipeline throughout; the proof kernel lives under `src/osprey/examples/`, never under `src/toolchain/crates/`.
- A project uses a discipline by listing its package under `[dependencies]`; nothing else is configured. Two dependencies registering the same `name` is a `discipline-error` at project load, naming both packages.
- The checker program is built once by the ordinary build pipeline, under the project's memory backend, and cached beside the package by content hash. A discipline is therefore itself an Osprey program under every rule of the language: no panics, typed `Result`, both backends, wasm32 where the package claims it.

## Selection `[TYPES-DISCIPLINE-SELECT]`

The marker `// osprey: types=<name>` (or `# osprey: types=<name>` in Python flavor) sits in the file header beside the flavor marker and is read by the same scan as [FLAVOR-SELECT](0023-LanguageFlavors.md#flavor-selection). Precedence, highest first:

1. the file marker;
2. `[project].types = "<name>"` in `osprey.toml`, applied to every project source without a marker;
3. `hm`.

A name no listed package provides is one `discipline-error` diagnostic on the marker, and the module is not checked by anything else. A marker in a file with no namespace or in a file selected as a Python source is read the same way; discipline and flavor are orthogonal, and every pair is legal.

## Seam `[TYPES-DISCIPLINE-SEAM]`

Project assembly ([MODULES-PROJECT](0025-ModulesAndNamespaces.md#project-assembly-modules-project)) already orders modules by their import graph and checks them with signatures of their imports in hand. The seam is one function on that path, called once per module, in front of `check_program`:

```text
check_module(module, imported_signatures) ->
    Accepted { signature, erased_program }
  | Rejected { diagnostics }
```

- For `hm` it calls `check_program` and returns the inferred signature and the module unchanged.
- A disciplined module is parsed only for its outline (namespaces, modules, imports), which orders the seam and gathers import signatures; its own syntax belongs to its checker, so the HM frontend never reports errors in it ([PROOF-SOURCE-DISCIPLINE](0049-DependentTypesAndProofs.md#rules)).
- For any other discipline it runs the registered checker process, parses the erased module through the ordinary frontend of the flavor it declares, and checks the erased module **with `check_program` against the signature the checker wrote**, under the signature-ascription rules of [MODULES-SIGNATURE]. A checker whose erased output does not satisfy its own signature is reported as a `discipline-error` naming the checker, never as a type error in the user's module.
- The result is consumed exactly where `check_program`'s result is consumed today. Inference, effect checking, redundancy analysis, monomorphisation, resolution and code generation never see the discipline name; a reviewer who finds a branch on it below the seam moves the logic into the checker before merge. This extends [FLAVOR-BOUNDARY](0023-LanguageFlavors.md#canonical-ast-boundary) from syntax to typing.

Because every erased module passes `check_program`, a discipline cannot introduce an ill-typed program to the backend: the worst a wrong checker can do is reject a correct module or accept a module whose erased form is HM-correct but means something else, and the second is the discipline's trust problem, documented by that discipline ([PROOF-TRUST](0055-ProofKernelAndLeanBridge.md#trust-proof-trust) for `lean4`).

## Checker protocol `[TYPES-DISCIPLINE-PROTOCOL]`

The checker is a separate process so that the compiler links no discipline, a checker fault cannot take the compiler down, and a discipline can be any size. One request on standard input, one response on standard output, JSON, in the style of the language server of [spec 0020](../tooling/0020-LanguageServerAndEditors.md):

```json
{ "version": 1,
  "module": { "path": "src/Group/Basic.ospml", "flavor": "ml", "source": "..." },
  "imports": [ { "name": "Core.Nat", "discipline": "hm", "signature": "signature ... " } ],
  "options": { "project_root": "...", "cache_dir": "..." } }
```

```json
{ "version": 1,
  "result": "accepted",
  "signature": "signature Group.Basic\n    mul_inv_cancel_left : ...",
  "erased": { "flavor": "ml", "source": "..." },
  "diagnostics": [] }
```

- `imports[].signature` is the export signature of each imported module in ML signature syntax, whatever discipline produced it; a checker reads HM signatures and nothing else about other modules. Imports are supplied in dependency order; the checker never reads source files itself.
- `diagnostics[]` carry `code`, `severity`, `range` and `message`; the compiler prefixes `code` with `<name>/` and prints them through the ordinary diagnostic channel, so editors and `osprey check` show a discipline's error exactly as they show a type error.
- A checker that exits non-zero without a well-formed response, exceeds the per-module time limit of `[discipline].timeout_seconds` (default 600), or writes an erased module the frontend cannot parse, yields one `discipline-error` with the exit status and the captured stderr tail. The compiler never panics on checker output.
- The protocol is versioned; a checker that answers with an unsupported `version` is a `discipline-error`.
- A checker may answer `"capabilities"` to a probe request listing optional services it offers to the language server (hover, goal state at a position, code actions); each is a separate request type in the same protocol and the language server forwards them unchanged. A discipline without them has plain file-level diagnostics and nothing else, which is complete.

- A response may list `dependencies[]`: files outside the request the checker read to decide (a proof certificate, say), relative to the project root. A reply that names none read nothing else.

Results are cached by the hash of the request and the checker binary, beside a digest of every listed dependency; an unchanged module with unchanged imports and unchanged dependencies is not re-checked, and a dependency that changes, appears or vanishes is a miss.

## Boundary `[TYPES-DISCIPLINE-BOUNDARY]`

Modules of different disciplines meet only through export signatures.

- **Into a discipline:** a checker receives the HM signatures of its imports. How it interprets HM types, effect rows and unions is its own rule, documented by the discipline ([PROOF-BOUNDARY](0049-DependentTypesAndProofs.md#rules) for `lean4`); it must accept every HM signature or reject the import with a diagnostic, never silently narrow it.
- **Out of a discipline:** only items in the export signature exist to other modules, and every one of them has an HM type. Anything the discipline's theory can express but HM cannot (a proof, a universe, a refinement predicate, a linearity annotation) is either erased from the signature or kept private. An importer that names a private item gets the ordinary unknown-export error from the HM checker.
- **Effects:** the export signature declares effect rows like any signature; the erased module is effect-checked by `check_program` against them, so a discipline cannot smuggle an undeclared effect.
- **Entry point:** a discipline module may be the entry source if its erased module contains the `main` that [MODULES-ENTRYPOINT](0025-ModulesAndNamespaces.md#entry-point-modules-entrypoint) requires.
- **Cycles** between modules are forbidden as today, whatever their disciplines.

## The existing language is untouched `[TYPES-DISCIPLINE-NO-HARM]`

The rules of [PROOF-NO-HARM](0049-DependentTypesAndProofs.md#rules) are the rules of this contract, restated for the seam alone:

1. A file with no `types=` marker and a project with no `[project].types` are lexed, parsed, inferred, effect-checked and lowered exactly as before; the differential corpus and `src/osprey/examples/` change no `.expectedoutput`.
2. No new token, keyword or operator in `hm` modules. The marker is a comment, as the flavor marker is.
3. No change to `Type`, unification, effect rows, redundancy analysis, monomorphisation or any diagnostic text in `src/toolchain/crates/osprey-types`; the diff of phase 1 is the seam function, the marker scan, the manifest table and their tests.
4. A program that uses no discipline runs no checker process, builds no checker and links nothing new.
5. Deleting the seam and the `[discipline]` table restores the compiler as it was.

## Reference disciplines `[TYPES-DISCIPLINE-REFERENCE]`

Four packages prove the seam, two of them dependent:

- **`passthrough`** (`src/osprey/examples/disciplines/passthrough/`): the checker runs `check_program`'s own rules by shelling out to `osprey check` on the module, then returns the module's inferred signature and the module itself as its erased form. It checks nothing new and demonstrates the round trip: an `hm` module importing a `passthrough` module, and the reverse, in the differential corpus under GC, ARC and wasm32, with output identical to the same program under `hm` alone.
- **`bounded`** (`src/osprey/examples/disciplines/bounded/`): a deliberately small discipline that reads `// @bound n in 0..100` annotations on `int` parameters, checks every call site inside the module against them by interval arithmetic, and erases to plain `int`. It is the template for a real discipline: a theory the HM checker cannot express, diagnostics through the protocol, erasure to HM, and about two hundred lines. The refinement-type discipline over the KLEE/Z3 tooling of [plan 0034](../../plans/0034-symbolic-execution.md) grows from this template, outside `src/toolchain/crates/`.
- **`dependent`** (`src/osprey/examples/disciplines/dependent/`): the first dependent discipline ([PROOF-DISCIPLINE](0049-DependentTypesAndProofs.md#rules)). `theorem` lines inside the module are read by the proof kernel's surface reader, checked by the kernel in file order, and erased; the rest of the module is ordinary HM code. Its checker is a judge function handed to `bounded`'s protocol (`Checker::serve`) with the kernel compiled in, so the protocol and the theory each exist once.
- **`leanexport`** (`src/osprey/examples/disciplines/lean/`): a second dependent discipline over the same kernel with a different source of proofs. `uses lean <Name> from <certificate>` lines cite Lean theorems; each pinned `lean4export` certificate is re-checked from an empty environment and listed as a dependency ([PROOF-SOURCE-CITATIONS](0049-DependentTypesAndProofs.md#rules)).

Further disciplines, including the Lean-source `lean4` target of [spec 0055](0055-ProofKernelAndLeanBridge.md), are built on this seam and add nothing to it.

## Verification `[TYPES-DISCIPLINE-VERIFICATION]`

[Plan 0061](../../plans/0061-pluggable-type-disciplines.md) is complete when, in CI:

- `passthrough`, `bounded`, `dependent` and `leanexport` build as packages, and the corpus programs that use them pass under both memory backends and on wasm32;
- a program with no marker produces byte-identical output and diagnostics before and after the seam, measured over the whole differential corpus;
- every `discipline-error` case has a test: unknown name, duplicate name, checker crash, checker timeout, malformed response, unparsable erased module, erased module failing its own signature, unsupported protocol version;
- the grep test over `src/toolchain/crates/` is green;
- the benchmark suite shows no change for `hm`-only projects.

## References

- Atkey. "Syntax and semantics of quantitative type theory." LICS 2018.
- Barendregt and Wiedijk. "The challenge of computer mathematics." Philosophical Transactions of the Royal Society A 363, 2005.
- Bracha. "Pluggable type systems." OOPSLA Workshop on Revival of Dynamic Languages, 2004.
- Brady. "Idris 2: Quantitative type theory in practice." ECOOP 2021.
- Brady, McBride and McKinna. "Inductive families need not store their indices." TYPES 2003.
- Carneiro. "Lean4Lean: Towards a verified typechecker for Lean, in Lean." arXiv:2403.14064, 2024.
- de Moura and Ullrich. "The Lean 4 theorem prover and programming language." CADE 2021.
- Dunfield and Krishnaswami. "Complete and easy bidirectional typechecking for higher-rank polymorphism." ICFP 2013.
- Dunfield and Krishnaswami. "Bidirectional typing." ACM Computing Surveys 54(5), 2021.
- Freeman and Pfenning. "Refinement types for ML." PLDI 1991.
- Gordon, Milner and Wadsworth. *Edinburgh LCF*. LNCS 78, 1979.
- Huet. "A unification algorithm for typed λ-calculus." Theoretical Computer Science 1(1), 1975.
- Letouzey. "A new extraction for Coq." TYPES 2002.
- McBride. "I got plenty o' nuttin'." In *A List of Successes That Can Change the World*, LNCS 9600, 2016.
- Papi, Ali, Correa, Perkins and Ernst. "Practical pluggable types for Java." ISSTA 2008.
- Paulin-Mohring. "Extracting Fω's programs from proofs in the calculus of constructions." POPL 1989.
- Peyton Jones, Vytiniotis, Weirich and Shields. "Practical type inference for arbitrary-rank types." JFP 17(1), 2007.
- Pierce and Turner. "Local type inference." ACM TOPLAS 22(1), 2000.
- Rondon, Kawaguchi and Jhala. "Liquid types." PLDI 2008.
- Swamy et al. "Dependent types and multi-monadic effects in F*." POPL 2016.
- Vazou, Seidel, Jhala, Vytiniotis and Peyton Jones. "Refinement types for Haskell." ICFP 2014.
- Wells. "Typability and type checking in System F are equivalent and undecidable." Annals of Pure and Applied Logic 98, 1999.
