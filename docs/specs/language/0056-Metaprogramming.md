# Compile-time metaprogramming [META]

**Status:** **specified, not implemented.** Nothing in this document is built. Delivery is [plan 0062](../../plans/0062-metaprogramming.md), which starts only when stabilization mode is lifted for it. The engine that runs expansions is [the expansion engine](../compiler/ExpansionEngine.md); editor and diagnostic behaviour is [macro tooling](../tooling/MacroTooling.md); the sources are the [research basis](../../research/metaprogramming/README.md).

## Aim `[META-AIM]`

Osprey programs compute at compile time, generate code and extend the language's notation, with the safety of the rest of the language and without choosing one technique for every job. Compile-time evaluation, typed staging, templates, syntax macros, declaration macros and schema providers are different tools. Each is a **mechanism**: a package-level way to produce code. Beneath them is one **kernel** that every mechanism must pass through and that the compiler trusts instead of trusting any mechanism.

Three consequences, matching [type disciplines](0050-TypeDisciplines.md):

1. A mechanism is added, replaced or forked without changing the kernel. The compiler knows the shape of an expansion, not what produced it.
2. Code produced by any mechanism is checked by the ordinary Hindley-Milner and effect pipeline of [spec 0004](0004-TypeSystem.md). No mechanism can vouch for its own output.
3. A program that uses none of this compiles exactly as it does today ([META-NO-HARM]).

## Design principles from the literature `[META-PRINCIPLES]`

These rules are normative. Each names its source and the rule that implements it.

1. `[META-PRINCIPLES-BINDING]` **Hygiene is decided by binding structure.** Every identifier carries a set of scopes, and a reference resolves to the binding with the largest subset of them; ambiguity is an error [Flatt 2016; Adams 2015]. Osprey: [META-KERNEL-HYGIENE].
2. `[META-PRINCIPLES-PHASE]` **Compile time and run time never share state.** Imports name their phase, and compile-time code is instantiated afresh per compilation [Flatt 2002; Xie et al. 2023]. Osprey: [META-PHASE].
3. `[META-PRINCIPLES-TYPED]` **A typed generator cannot produce ill-typed code.** Code is a typed value and levels are checked [Taha and Sheard 2000; Davies and Pfenning 2001; Stucki et al. 2018]. Osprey: [META-STAGE]. Because typed analysis of code is still research [Jang et al. 2022], it is never the only safeguard: [META-KERNEL-RECHECK].
4. `[META-PRINCIPLES-ONE]` **Macros and staging are one model.** A macro is a compile-time function over code [Ganz et al. 2001; Stucki et al. 2018]; the compile-time level leaves nothing behind [Kovács 2022]. Osprey: one level rule ([META-STAGE-LEVEL]) and one kernel for every mechanism.
5. `[META-PRINCIPLES-INPUT]` **A macro validates its input and reports in the user's terms.** [Culpepper and Felleisen 2010; Pombrio and Krishnamurthi 2014]. Osprey: [META-SYNTAX-CLASS], [LSP-MACRO-DIAG](../tooling/MacroTooling.md#diagnostics-lsp-macro-diag).
6. `[META-PRINCIPLES-PARSE]` **A source file parses without running anything.** Languages whose grammar depends on imports [Erdweg et al. 2011] cannot be read by an editor until code has run. Osprey: [META-KERNEL-PARSE].
7. `[META-PRINCIPLES-INCREMENTAL]` **Expansion is a cached query with recorded dependencies.** [Mokhov et al. 2018]. Macros that inspect arbitrary semantics defeat it, which ended Dart's macros. Osprey: [META-ORDER], [EXPAND-CACHE](../compiler/ExpansionEngine.md#cache-expand-cache).
8. `[META-PRINCIPLES-ADD]` **Macros add code; they do not rewrite code they were not given.** Swift macros and C# source generators both rest on this. Osprey: [META-KERNEL-ADDITIVE].
9. `[META-PRINCIPLES-EFFECT]` **What a compile-time program may do is an effect.** Generation-time effects must not let code escape its binder [Kiselyov 2014; Kameyama et al. 2011], and compiler services are best offered as a typed effect [Christiansen and Brady 2016]. Osprey: [META-EFFECTS].

## Definitions `[META-DEF]`

- **Level.** Run time is level 0. Compile time is level 1. A macro that defines macros runs at level 2, and so on.
- **Syntax object.** A tree of the canonical AST ([FLAVOR-BOUNDARY](0023-LanguageFlavors.md#canonical-ast-boundary)) in which every node has a source span, a scope set and an origin ([META-KERNEL-ORIGIN]).
- **Invocation.** A place in a source file where a mechanism is asked for code: an expression, a declaration attribute, a block or a delimited literal ([META-SYNTAX-SHAPES]).
- **Expander.** The program that answers an invocation. It may be an Osprey function at level 1, or an external engine behind the [worker protocol](../compiler/ExpansionEngine.md#protocol-expand-protocol).
- **Expansion.** The syntax objects an expander returns, with its diagnostics and the dependencies it used.
- **Mechanism.** A documented way of writing expanders, defined in [Mechanisms](#mechanisms-meta-mechanisms).

## Phases `[META-PHASE]`

- An import states its level. A plain import is level 0. A level-1 import makes a module's exports available to compile-time code only. The provisional spelling is `import static`, sharing the word the effect system already uses for compile-time handlers ([STAGE](0017-AlgebraicEffects.md#stage--stage-axis)); the grammar is settled per flavor in [plan 0062](../../plans/0062-metaprogramming.md#surface-syntax).
- A binding is usable only at the level it was defined, except by the two crossings of [META-STAGE-LEVEL].
- Each compilation instantiates level-1 modules afresh. No level-1 value survives into the program, into another compilation or into another module's compilation except through an expansion.
- A module compiled for level 1 is the same source compiled the ordinary way. There is no second language for macros: every flavor writes them, and every Osprey rule applies to them, including [FAILURE-EXPLICIT](0013-ErrorHandling.md).

## Kernel `[META-KERNEL]`

Every expansion, from every mechanism, meets these rules. They are enforced by the compiler, not by convention.

- `[META-KERNEL-PARSE]` **Static parse.** Every source file parses to a complete tree with no expander run and no import read. Invocations use fixed shapes the grammar already knows ([META-SYNTAX-SHAPES]); a macro never changes how the rest of a file is read.
- `[META-KERNEL-HYGIENE]` **Hygiene.** An expansion is syntax objects. The kernel adds a fresh introduction scope to everything the expander created and resolves names by scope sets. An expander that returns text ([EXPAND-TEXT](../compiler/ExpansionEngine.md#text-expanders-expand-text)) has it parsed and scoped by the kernel under the same rule. Deliberate capture is explicit: the expander asks for an identifier in the invocation's scope, and the name must be declared ([META-DECL-NAMES]).
- `[META-KERNEL-RECHECK]` **Recheck.** The expanded program is parsed as canonical AST and checked by `check_program` like hand-written code. A program with errors in expanded code is red ([COMPILER-RED-FAILS]).
- `[META-KERNEL-ADDITIVE]` **Additive.** An expansion replaces its own invocation or adds declarations beside or inside the declaration it is attached to. It never changes or removes code outside its invocation.
- `[META-KERNEL-ORIGIN]` **Origin.** Every node of an expansion records where it came from: a span in the invocation's arguments, or a span in the expander's definition, plus the chain of invocations that led to it. Diagnostics, the debugger, coverage and documentation read this record ([LSP-MACRO-ORIGIN](../tooling/MacroTooling.md#origin-lsp-macro-origin)).
- `[META-KERNEL-PURE]` **Pure, deterministic, bounded.** An expander's result depends only on its invocation, the answers to its recorded queries and its declared inputs. It has a step limit and a memory limit. It reads no clock, network, environment or undeclared file. The same inputs give the same bytes on every host.
- `[META-KERNEL-EXPLICIT]` **No panics.** An expander that cannot answer returns diagnostics. Exhausting a limit, a crash in an external engine and a malformed answer are each one diagnostic at the invocation; the compiler never panics on expander output and never skips an invocation silently.
- `[META-KERNEL-TERMINATE]` **Expansion terminates.** An expansion may contain invocations, expanded in turn to a fixed depth limit. Exceeding it is a diagnostic that prints the chain.

## Compile-time effects `[META-EFFECTS]`

What an expander may do is its effect row. The compiler supplies the handlers; an operation with no compile-time handler cannot be performed at level 1, and the ordinary effect checker reports it.

| Effect | Gives | Recorded as a dependency |
| --- | --- | --- |
| `Diagnose` | Report an error, warning or note at a span of the invocation | no |
| `Fresh` | Create a hygienic name | no |
| `Reflect` | Ask for a declaration's shape, a type's structure or an argument's type ([META-REFLECT]) | yes, per question |
| `Input` | Read a file the invocation or its package manifest declares | yes, by content hash |

- A level-1 function with an empty row is a pure compile-time function and may also be called at run time unchanged.
- Run-time effects (`Http`, file system, fibers, FFI) have no level-1 handler. A macro cannot open a socket because the program that would do so does not type-check.
- New compile-time services are new effects with new handlers. This is the extension point for mechanisms nobody has designed yet.

## Reflection `[META-REFLECT]`

`Reflect` answers questions about the program as immutable values: the fields of a record, the variants of a union, the operations of an effect, the parameters of a type, a declaration's documentation and attributes, and the inferred type of an invocation's argument.

- A reflective value describes a declaration's **signature**, never the body of a function in another module.
- Each question is a recorded dependency. An expansion is reused while every answer it received is unchanged.
- A type is a compile-time value: it can be passed, matched on and stored at level 1. It has no run-time representation unless the program builds one.

## Mechanisms `[META-MECHANISMS]`

Six mechanisms are specified. They share the kernel and nothing else, and a seventh needs no kernel change ([META-OPEN]).

### Compile-time evaluation `[META-EVAL]`

An expression marked `static` is evaluated at level 1 and replaced by its value.

- Any Osprey function whose effect row is empty, or contains only compile-time effects, can be evaluated at compile time. There is no restricted sub-language.
- `[META-EVAL-AGREE]` A function evaluated at compile time returns what it returns at run time, for every memory mode and target. Integer arithmetic follows the [`Arith`](0037-ArithmeticEffects.md) policy of the call site at both levels.
- A value crosses to level 0 only if its type can be written as a literal: numbers, strings, booleans, records, unions, lists and maps of those, and `Code<T>`. A compile-time function value or a reference cell does not cross.
- A failed evaluation is a `Result`, and an `Error` that reaches the boundary is a compile error at the `static` expression carrying the error's value.

### Typed staging `[META-STAGE]`

`Code<T>` is the type of a program fragment that produces a `T` when run. A **quote** builds one; a **splice** inserts one into a quote or, at the top level, into the program.

- `[META-STAGE-LEVEL]` A variable is used at the level it was bound. The two crossings are: a level-0 name mentioned inside a quote refers to the same binding in the generated code; a level-1 value is inserted by lifting it to a literal, under the rule of [META-EVAL]. This is the phase consistency principle [Stucki et al. 2018].
- A well-typed quote is checked once, where it is written. A `Code<T>` carries its effect row: `Code<T ! E>` splices only where `E` is permitted, and row variables are level-checked like constraints [Xie et al. 2022].
- `[META-STAGE-EXTRUSION]` Code that mentions a binder must not be used outside that binder's quote. Where the checker can prove it, the program is rejected; otherwise the kernel detects it during expansion [Kiselyov 2014]. Either way it is a diagnostic naming the binder and the escape, and no ill-scoped code is emitted.
- `[META-STAGE-ANALYSE]` Matching on a `Code<T>` against quoted patterns is permitted. A typed match keeps the guarantee of [META-PRINCIPLES-TYPED]. The untyped view of a fragment as a syntax object is also available; code built from it has no static guarantee and relies on [META-KERNEL-RECHECK].

### Templates `[META-TEMPLATE]`

A template is a level-1 function that takes types and compile-time values and returns code. Calling it with level-1 arguments specialises it.

- A parameter marked `static` must be known at compile time. Branching on it is ordinary `match`; there is no separate conditional-compilation form.
- `[META-TEMPLATE-GONE]` Nothing of level 1 remains in the generated program: no closure, tag or dictionary introduced only to pass a compile-time argument [Kovács 2022]. A test compares the emitted IR of a specialised template with the hand-written equivalent.
- Specialisations are shared: the same template with equal arguments expands once per program.
- Generic functions keep their meaning. Templates do not replace Hindley-Milner generics; they cover what generics cannot express, such as a function generated per field of a record.

### Syntax macros `[META-SYNTAX]`

A syntax macro maps the syntax of its invocation to syntax.

- `[META-SYNTAX-SHAPES]` Four invocation shapes exist in every flavor, each parsed by the flavor's fixed grammar: an **expression** invocation with arguments, a **declaration attribute**, a **block** invocation whose body is ordinary flavor syntax, and a **delimited literal** whose body is a token tree. The spelling of each shape is fixed per flavor by [plan 0062](../../plans/0062-metaprogramming.md#surface-syntax).
- `[META-SYNTAX-TREE]` A token tree is a sequence of tokens in which brackets balance and, in layout flavors, indentation groups. The grammar reads it without interpreting it; the macro receives it with spans [Flatt et al. 2023].
- `[META-SYNTAX-CLASS]` A macro declares the shape of its input as patterns over **syntax classes**: expression, pattern, type, identifier, literal, or a class the package defines. Input that does not match is reported at the term that failed, with the class's description, before any expansion [Culpepper and Felleisen 2010].
- `[META-SYNTAX-LITERAL]` A delimited literal's macro parses the body itself and declares the type of the result before expansion, so an embedded language has a type the checker can use without running the macro [Omar and Aldrich 2018]. It reports spans inside the literal and may offer editor services for it ([LSP-MACRO-DSL](../tooling/MacroTooling.md#embedded-languages-lsp-macro-dsl)).
- `[META-SYNTAX-FLAVOR]` Macros consume and produce canonical AST. A macro written in one flavor is usable from every flavor, and its expansion is shown in the reader's flavor ([LSP-FLAVOR-RENDER](../tooling/0020-LanguageServerAndEditors.md)).
- Pattern macros, written only as patterns and templates, are the default. Procedural macros are level-1 functions from syntax objects to syntax objects and are used when patterns cannot express the transformation.

### Declaration macros `[META-DECL]`

A declaration macro is attached to a type, function, effect, handler or module and adds declarations.

- `[META-DECL-ROLES]` A macro declares its roles: **member** (adds members to the declaration), **peer** (adds declarations beside it), **handler** (adds a handler for a named effect), **accessor** (wraps a field's reads and writes). It may do only what its roles say.
- `[META-DECL-NAMES]` A macro declares the names it introduces into scopes the user can see: a fixed list, a prefix or suffix of the attached name, or names taken from its arguments. A name outside the declaration is an error. Every other name it introduces is hygienic and invisible. This lets a module's outline be computed without running the macro ([META-ORDER-OUTLINE]).
- `[META-DECL-GENERIC]` Deriving prefers a typed route: reflect the declaration, then build code with quotes [Magalhães et al. 2010]. Templates over text are admitted through [META-PROVIDER].
- Several macros on one declaration expand in written order. Each sees the declaration as written, not the others' output, unless it declares that it runs after a named macro.

### Providers `[META-PROVIDER]`

A provider turns an external description into declarations: a [typeDiagram](https://typediagram.dev/docs/language-reference.html) model, a database schema, an API description, a template directory.

- Every input is declared in the invocation or the package manifest and is recorded by content hash. A provider reads nothing else.
- A provider may be an Osprey expander, or an external engine such as [dmx](https://dmx.dev/docs/) behind the [worker protocol](../compiler/ExpansionEngine.md#protocol-expand-protocol). Both obey the kernel.
- The declarations exist in the compilation, not in the repository. Generated source is never committed ([REPO-NO-ARTIFACTS]); `osprey expand` prints it for reading.

### More mechanisms `[META-OPEN]`

A new mechanism is a package that defines expanders, plus at most a new compile-time effect. It must not require a kernel rule to be relaxed. A proposal that needs one, such as rewriting code outside an invocation, is a change to this specification and states which principle it gives up and why.

## Order of expansion `[META-ORDER]`

Some macros need only syntax; some need types. Both are allowed, and the cost of the second is bounded.

- `[META-ORDER-OUTLINE]` A module's outline (its imports, its declarations' names and the names its declaration macros introduce) is computed from the parse and the declared names alone. Project assembly ([MODULES-PROJECT](0025-ModulesAndNamespaces.md#project-assembly-modules-project)) and name resolution use it before any expander runs.
- `[META-ORDER-SYNTAX]` An expander that asks no `Reflect` question about types runs before type checking, from the outline alone.
- `[META-ORDER-TYPED]` An expander that asks for types is run by the checker when it reaches the invocation. It may ask for the types of its arguments and for the signatures of declarations, in its own module or in imports.
- `[META-ORDER-CYCLE]` If answering a question requires the expansion that asked it, the program is rejected with the cycle printed. Expansion is never iterated to a fixed point.
- Expansion order within a module does not change the result. A test expands every corpus module in reverse and randomised order and compares bytes.

## Relation to existing contracts `[META-RELATION]`

These contracts already move work to compile time or generate code. This specification does not replace them and does not copy their rules.

| Contract | What it does | Relation |
| --- | --- | --- |
| [Static handlers](0017-AlgebraicEffects.md#stage--stage-axis) (`[STAGE]`) | Resolves a handler at compile time | The same level 1. [META-STAGE-LEVEL] and `[STAGE]` must agree on what "known at compile time" means; one definition, cited by both. |
| [Language flavors](0023-LanguageFlavors.md) | Lower each syntax to one canonical AST | Macros operate on that AST; flavors stay fixed grammars ([META-KERNEL-PARSE]). |
| [Type disciplines](0050-TypeDisciplines.md) | Check a module with another type system and erase it | A discipline transforms a whole module by its checker; a macro answers one invocation. They share the worker process, limits and cache of the [expansion engine](../compiler/ExpansionEngine.md#shared-infrastructure-expand-shared). |
| [Math to code](../tooling/MathToCode.md) | Generates Osprey from Lean formulas | A provider once [META-PROVIDER] exists; its proof contract is unchanged. |
| [Documentation examples](0026-DocumentationComments.md) | Compile and run examples in comments | Examples may use macros; documentation lists generated members ([LSP-MACRO-DOCS](../tooling/MacroTooling.md#documentation-lsp-macro-docs)). |

## Packages and security `[META-SECURITY]`

- A package that exports expanders says so in its manifest. A project that depends on it sees this in `osprey.toml` resolution and in the lock file, and may refuse expanders from a package.
- Package invariant 4 ([PACKAGE-THREATS](../ecosystem/0029-PackageManagement.md#threat-model-and-invariants-package-threats)) holds: nothing in a package runs during fetch, install, lock, build-plan creation or activation. An expander runs only while compiling a module that invokes it.
- An expander runs with no ambient authority. Its whole capability is its effect row ([META-EFFECTS]) and its declared inputs, inside the sandbox of the [expansion engine](../compiler/ExpansionEngine.md#sandbox-expand-sandbox).
- The identity of an expander (package, version, content hash) is part of every cache key and of the build's evidence, so a changed macro is a changed build.

## The existing language is untouched `[META-NO-HARM]`

A program with no invocation and no level-1 import produces the same output and the same diagnostics, byte for byte, with and without the kernel, across the whole differential corpus. The kernel adds no pass to such a program's compilation beyond finding that it has no invocations.

## Verification `[META-VERIFICATION]`

- Every mechanism has programs in `src/osprey/tests/` that pass under GC, ARC, ownership and wasm32, and rejected twins in `src/osprey/examples/failscompilation/` checked against the compiler's own diagnostics.
- A hygiene suite uses names chosen to collide (`other`, `result`, `self`, `it`, the macro's own temporaries) at every invocation shape [Adams 2015].
- Determinism: every expansion in CI runs twice and on two hosts; a difference fails the build ([EXPAND-VERIFY](../compiler/ExpansionEngine.md#verification-expand-verify)).
- [META-EVAL-AGREE] is a differential test: every pure function in the corpus that can be called with literal arguments is evaluated at both levels.
- What can be proved is proved with the proof kernel: the obligations are listed in [plan 0062](../../plans/0062-metaprogramming.md#proof-obligations).

## References

Every address below was tested on 2026-11-27: a PDF address returns the same bytes as the downloaded copy, and a DOI resolves to the work's record, whose title Crossref confirms. The [research basis](../../research/metaprogramming/README.md) says what each work establishes and what Osprey takes from it.

- [Adams 2015, *Towards the essence of hygiene*, POPL](https://michaeldadams.org/papers/hygiene/hygiene-2015-popl-authors-copy.pdf)
- [Christiansen and Brady 2016, *Elaborator reflection: extending Idris in Idris*, ICFP](https://doi.org/10.1145/2951913.2951932)
- [Culpepper and Felleisen 2010, *Fortifying macros*, ICFP](https://www2.ccs.neu.edu/racket/pubs/icfp10-cf.pdf)
- [Davies and Pfenning 2001, *A modal analysis of staged computation*, JACM](https://www.cs.cmu.edu/~fp/papers/jacm00.pdf)
- [Erdweg, Rendel, Kästner and Ostermann 2011, *SugarJ: library-based syntactic language extensibility*, OOPSLA](https://www.pl.informatik.uni-mainz.de/files/2019/04/sugarj.pdf)
- [Flatt 2002, *Composable and compilable macros: you want it when?*, ICFP](https://www.cs.utah.edu/plt/publications/macromod.pdf)
- [Flatt 2016, *Binding as sets of scopes*, POPL](https://www.cs.utah.edu/plt/publications/popl16-f.pdf)
- [Flatt et al. 2023, *Rhombus: a new spin on macros without all the parentheses*, OOPSLA](https://kirancodes.me/pdfs/rhombus-oopsla23.pdf)
- [Ganz, Sabry and Taha 2001, *Macros as multi-stage computations*, ICFP](https://doi.org/10.1145/507635.507646)
- [Jang, Gélineau, Monnier and Pientka 2022, *Mœbius: metaprogramming using contextual types*, POPL](https://arxiv.org/pdf/2111.08099)
- [Kameyama, Kiselyov and Shan 2011, *Shifting the stage*, JFP](https://doi.org/10.1017/S0956796811000256)
- [Kiselyov 2014, *The design and implementation of BER MetaOCaml*, FLOPS](https://okmij.org/ftp/meta-programming/ber-design.pdf)
- [Kovács 2022, *Staged compilation with two-level type theory*, ICFP](https://arxiv.org/pdf/2209.09729)
- [Magalhães, Dijkstra, Jeuring and Löh 2010, *A generic deriving mechanism for Haskell*, Haskell Symposium](https://dreixel.net/research/pdf/gdmh.pdf)
- [Mokhov, Mitchell and Peyton Jones 2018, *Build systems à la carte*, ICFP](https://www.microsoft.com/en-us/research/uploads/prod/2018/03/build-systems.pdf)
- [Omar and Aldrich 2018, *Reasonably programmable literal notation*, ICFP](https://doi.org/10.1145/3236801)
- [Pombrio and Krishnamurthi 2014, *Resugaring: lifting evaluation sequences through syntactic sugar*, PLDI](https://cs.brown.edu/~sk/Publications/Papers/Published/pk-resuarging/paper.pdf)
- [Stucki, Biboudis and Odersky 2018, *A practical unification of multi-stage programming and macros*, GPCE](https://biboudis.github.io/papers/pcp-gpce18.pdf)
- [Taha and Sheard 2000, *MetaML and multi-stage programming with explicit annotations*, TCS](https://doi.org/10.1016/S0304-3975(00)00053-0)
- [Xie, Pickering, Löh, Wu, Yallop and Wang 2022, *Staging with class*, POPL](https://www.cl.cam.ac.uk/~jdy22/papers/staging-with-class.pdf)
- [Xie, White, Nicole and Yallop 2023, *MacoCaml: staging composable and compilable macros*, ICFP](https://www.cl.cam.ac.uk/~jdy22/papers/macocaml-staging-composable-and-compilable-macros.pdf)
