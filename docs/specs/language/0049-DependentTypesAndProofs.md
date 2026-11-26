# Dependent types [PROOF]

**Status:** two dependent disciplines run today (`dependent` and `leanexport`), each a package, each checked by the same Osprey kernel, each tested end to end under GC, ARC and wasm32 by `make _test_disciplines`. No Osprey function's type mentions a value yet. The kernel and the Lean bridge they share are [spec 0055](0055-ProofKernelAndLeanBridge.md); the plug-in mechanism is [spec 0050](0050-TypeDisciplines.md).

## What a dependent type is

An ordinary type describes a kind of value: `List<int>`. A dependent type may also mention a value: `Vector Int 3` is a list of exactly three integers, and `head : Vector α (n + 1) → α` cannot be called on an empty vector, because no empty vector has that type.

Taken further, a type can state a fact and a value of that type is its proof (the Curry–Howard correspondence [Howard 1980]). `∀ (p q : Prop), p ∧ q → q ∧ p` is a type; a function of that type is a proof that "and" is symmetric. A dependent type checker is therefore also a proof checker [Martin-Löf 1984].

## Why Hindley-Milner cannot have them

Hindley-Milner (HM) is the type system every Osprey program uses [Hindley 1969; Milner 1978; Damas and Milner 1982]. It gives a promise Osprey depends on: every expression has one most general type, the compiler always finds it, and the programmer never has to write a type down.

Dependent types break that promise in three ways:

1. **Checking needs evaluation.** To check that `append xs ys : Vector α (2 + 1)` fits where `Vector α 3` is expected, the checker must compute `2 + 1`. Types now contain programs, and type equality is equality of programs up to evaluation (definitional equality).
2. **Inference becomes undecidable.** Even System F, a fragment far smaller than dependent types, has undecidable type inference [Wells 1999]; unifying types that contain functions is undecidable in general [Huet 1975]. There is no algorithm that recovers every dependent type from unannotated code.
3. **No principal types.** A term can have many incomparable dependent types, so there is no single "best" type to infer.

Dependent systems therefore ask for annotations and proofs, and check them bidirectionally: the programmer states the type, the checker verifies the term against it [Pierce and Turner 2000; Dunfield and Krishnaswami 2021]. Adding that to Osprey's HM checker would make annotations mandatory where today they are redundant ([TYPE-ANNOTATION-REDUNDANT](0004-TypeSystem.md#redundant-annotations--type-annotation-redundant)). So Osprey keeps HM for every program and puts dependent types beside it, one module at a time.

## How Osprey adds them: pluggable disciplines

A module opts in with a header comment, `// osprey: types=<name>`. The compiler hands that module to the checker of the package that registers `<name>` ([spec 0050](0050-TypeDisciplines.md)). The checker uses its own theory, then returns an **erased** module (proofs and dependent indices removed) and an HM export signature. The erased module goes through the ordinary HM checker against that signature, so other modules see plain HM types and never a proof. Erasure of computationally irrelevant parts is the standard route from a proof assistant to running code [Paulin-Mohring 1989; Letouzey 2002; Brady, McBride and McKinna 2003]; current dependent languages go further and make run-time relevance part of the type, erasing exactly what is used zero times [McBride 2016; Atkey 2018; Brady 2021]. The rules every discipline follows, and the research behind each, are [TYPES-DISCIPLINE-PRINCIPLES](0050-TypeDisciplines.md#design-principles-from-the-literature-types-discipline-principles).

Any number of disciplines coexist, and the compiler names none of them: a test fails if `src/toolchain/crates/` mentions one. A refinement-type discipline [Freeman and Pfenning 1991; Rondon, Kawaguchi and Jhala 2008], a dependent theory and a Lean bridge all plug in the same way.

## What exists today

| Discipline | A module may contain | Checked by | Status |
| --- | --- | --- | --- |
| `hm` | ordinary Osprey | HM inference | default |
| `bounded` | `// @bound p in 0..100` on `int` parameters | interval arithmetic at each call site | shipped; a refinement discipline, not a dependent one |
| `dependent` | `theorem name (x : A) ... : T := proof` lines | the Osprey kernel | shipped; propositional and Π-types only |
| `leanexport` | `uses lean Name from <certificate>` lines | the Osprey kernel, over a pinned `lean4export` certificate | shipped; certificates are produced by Lean beforehand |
| `lean4` | a `.lean` source file | Lean's front end, then the Osprey kernel | target ([PROOF-SOURCE-LEAN](0055-ProofKernelAndLeanBridge.md)) |

The examples are `src/osprey/examples/disciplines/dependent_roundtrip` (an access-control policy whose rules are theorems) and `src/osprey/examples/disciplines/lean_roundtrip` (vote totals that rely on Lean's `Nat.add_comm`, re-checked from an empty environment). Each has a rejected twin: a wrong proof, and a counterfeit certificate.

**What this does not do yet.** The theorems sit beside the code; nothing checks that the code is the thing the theorem is about, and the examples say so in their documentation. The `dependent` surface has no inductive types, no numbers and no tactics. The length-indexed vector in the films illustrates the idea; it is not an Osprey API.

**What this sets up.** A theory is a package with its own identity, replaceable and forkable ([TYPES-DISCIPLINE-EVOLUTION](0050-TypeDisciplines.md#theory-identity-and-correction-types-discipline-evolution)). Every discipline shares one audited kernel ([PROOF-KERNEL](0055-ProofKernelAndLeanBridge.md)). Because every erased module must pass HM, a faulty checker can accept a false theorem but cannot hand the backend an ill-typed program. The next steps are the `lean4` source discipline, inductive types in the `dependent` surface, and checked links from a theorem to the code it describes ([plan 0041](../../plans/0041-dependent-types-and-mathlib.md)).

## Rules

`[PROOF-NO-HARM]` A file without a `types=` marker is lexed, parsed, inferred and lowered exactly as before: no new keyword, no annotation demanded, no `.expectedoutput` changed, no checker built or linked. The full list is [TYPES-DISCIPLINE-NO-HARM](0050-TypeDisciplines.md#the-existing-language-is-untouched-types-discipline-no-harm).

`[PROOF-DISCIPLINE]` A dependent discipline is a package declaring `[discipline] name = "<name>"`, run as a checker process through [TYPES-DISCIPLINE-PROTOCOL](0050-TypeDisciplines.md#checker-protocol-types-discipline-protocol). Disciplines that check proofs use the kernel of [spec 0055](0055-ProofKernelAndLeanBridge.md) and add nothing to it.

`[PROOF-BOUNDARY]` Disciplines meet HM only at export signatures. Into a discipline, every HM type is a simple type of the theory (`int` is `Int`, `fn(a) -> b` is `a → b`). Out of a discipline, an export exists only when its erased type is an HM type; proofs, propositions and universes are never visible to an HM importer.

`[PROOF-SOURCE-DISCIPLINE]` The source a discipline reads is its own choice. The compiler parses a disciplined module only for its outline (namespaces, modules, imports), never reports its HM syntax errors, and parses the erased module strictly ([TYPES-DISCIPLINE-SEAM](0050-TypeDisciplines.md#seam-types-discipline-seam)).

`[PROOF-SOURCE-SURFACE]` `dependent` reads one theorem per line: `theorem name (x y : A) ... : T := term`, with `Prop`, `Type`, names, application, `∀ (x : A), B`, `fun (x : A) => e` and `A → B` (also `forall`, `->`, `λ`). Tokens are separated by spaces. Theorems are checked in file order, each seeing the earlier ones; a refusal is a `dependent/kernel` diagnostic at its line.

`[PROOF-SOURCE-CITATIONS]` `leanexport` reads `uses lean <Name> from <certificate>`, the certificate a `lean4export` file relative to the project root. Each certificate is checked from an empty environment and must declare `<Name>`. The checker lists every certificate as a dependency, so changing one re-checks the module ([TYPES-DISCIPLINE-PROTOCOL](0050-TypeDisciplines.md#checker-protocol-types-discipline-protocol)).

`[PROOF-ERASE]` Propositions, proofs, universe levels and type arguments erase. Structures erase to records, inductives to unions, recursors to `match`. A definition that uses an axiom such as `Classical.choice` has no erasure and does not run. The erased program obeys every rule of the HM pipeline, including [FAILURE-EXPLICIT] and both memory backends.

`[PROOF-SOURCE-CONVERGE]` A discipline sharing the kernel may grow toward Lean's surface (implicit binders, `by` blocks, `inductive`) so that its modules and `.lean` files can one day convert into each other. That is a direction, not a precondition: each discipline is judged by its own theory and tests.

## References

- Atkey. "Syntax and semantics of quantitative type theory." LICS 2018.
- Brady. "Idris 2: Quantitative type theory in practice." ECOOP 2021.
- Brady, McBride and McKinna. "Inductive families need not store their indices." TYPES 2003, LNCS 3085.
- Damas and Milner. "Principal type-schemes for functional programs." POPL 1982.
- Dunfield and Krishnaswami. "Bidirectional typing." ACM Computing Surveys 54(5), 2021.
- Freeman and Pfenning. "Refinement types for ML." PLDI 1991.
- Hindley. "The principal type-scheme of an object in combinatory logic." Transactions of the AMS 146, 1969.
- Howard. "The formulae-as-types notion of construction." In *To H. B. Curry*, 1980.
- Huet. "A unification algorithm for typed λ-calculus." Theoretical Computer Science 1(1), 1975.
- Letouzey. "A new extraction for Coq." TYPES 2002, LNCS 2646.
- McBride. "I got plenty o' nuttin'." In *A List of Successes That Can Change the World*, LNCS 9600, 2016.
- Martin-Löf. *Intuitionistic Type Theory*. Bibliopolis, 1984.
- Milner. "A theory of type polymorphism in programming." Journal of Computer and System Sciences 17(3), 1978.
- Paulin-Mohring. "Extracting Fω's programs from proofs in the calculus of constructions." POPL 1989.
- Peyton Jones, Vytiniotis, Weirich and Shields. "Practical type inference for arbitrary-rank types." Journal of Functional Programming 17(1), 2007.
- Pierce and Turner. "Local type inference." ACM TOPLAS 22(1), 2000.
- Rondon, Kawaguchi and Jhala. "Liquid types." PLDI 2008.
- Wells. "Typability and type checking in System F are equivalent and undecidable." Annals of Pure and Applied Logic 98, 1999.
