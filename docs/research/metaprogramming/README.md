# Compile-time metaprogramming: research basis

This is the reading behind [spec 0056](../../specs/language/0056-Metaprogramming.md), the [expansion engine](../../specs/compiler/ExpansionEngine.md), [macro tooling](../../specs/tooling/MacroTooling.md) and [plan 0062](../../plans/0062-metaprogramming.md). Each entry says what the work establishes and what Osprey takes from it. The specs cite these as `[Author Year]`.

The papers are downloaded to `docs/research/metaprogramming/papers/`, which is ignored by git: the files are other people's copyright and are fetched, never committed. Every entry below gives the file name and the address it came from, so the folder can be rebuilt. Entries marked **not downloaded** had no open copy that could be fetched on 2026-11-27; they are cited from their published versions.

## What the literature agrees on

Six results recur across forty years and every language family. The specs treat them as rules.

1. **Hygiene is a property of binding, not of names.** Renaming schemes work, but the durable model attaches a set of scopes to every identifier and resolves a reference to the binding whose scopes are the largest subset [Flatt 2016]. It also explains definition contexts and macros that define macros.
2. **Compile time and run time are separate phases with separate state.** A module's compile-time code is instantiated afresh for each compilation, so compilation is repeatable and separate compilation is sound [Flatt 2002; Xie et al. 2023].
3. **Typed quotation makes generators safe by construction.** If code is a typed value and levels are checked, a well-typed generator only produces well-typed, well-scoped code [Taha and Sheard 2000; Davies and Pfenning 2001; Stucki et al. 2018].
4. **Macros and staging are one mechanism seen from two sides.** A macro is a compile-time function over code; `inline` plus a top-level splice gives macros from staging [Ganz et al. 2001; Stucki et al. 2018; Xie et al. 2023]. Two-level type theory states the same thing with a guarantee that nothing of the compile-time level survives [Kovács 2022].
5. **A macro must check its own input and report in its own terms.** Errors that surface from inside an expansion are the main usability failure of macro systems; declared syntax classes fix it [Culpepper and Felleisen 2010].
6. **Tools need the path from generated code back to source.** Steppers, resugaring and origin tracking are what make expanded programs debuggable [Culpepper and Felleisen 2007; Pombrio and Krishnamurthi 2014].

No single system delivers all six with editor-grade latency. That is the reason the specs define one expansion kernel and several mechanisms above it instead of choosing a mechanism.

## Hygiene and binding

| Work | File | Establishes | Osprey takes |
| --- | --- | --- | --- |
| Kohlbecker, Friedman, Felleisen and Duba 1986, *Hygienic macro expansion*, LFP | not downloaded ([doi](https://doi.org/10.1145/319838.319859)) | Names introduced by a macro must not capture or be captured by user names. | The requirement itself ([META-KERNEL-HYGIENE]). |
| Dybvig, Hieb and Bruggeman 1992, *Syntactic abstraction in Scheme*, Lisp and Symbolic Computation | not downloaded ([doi](https://doi.org/10.1007/BF01806308)) | Syntax objects carry lexical context; `syntax-case` gives procedural macros with hygiene. | Macros compute on syntax objects, never on text. |
| Flatt 2016, *Binding as sets of scopes*, POPL | `flatt-2016-binding-as-sets-of-scopes.pdf` ([source](https://www.cs.utah.edu/plt/publications/popl16-f.pdf)) | Scope sets replace renaming; resolution is by largest subset, with ambiguity an error. | The binding model of the kernel. |
| Flatt, Culpepper, Darais and Findler 2012, *Macros that work together*, JFP | `flatt-2012-macros-that-work-together.pdf` ([source](https://www.cs.utah.edu/plt/publications/jfp12-draft-fcdf.pdf)) | Compile-time bindings, partial expansion and definition contexts let macros cooperate. | Compile-time bindings as the way macros share static information. |
| Herman and Wand 2008, *A theory of hygienic macros*, ESOP | not downloaded ([doi](https://doi.org/10.1007/978-3-540-78739-6_4)) | Hygiene stated as preservation of alpha-equivalence, with macro types that declare binding structure. | The theorem to prove for pattern macros ([plan 0062](../../plans/0062-metaprogramming.md#proof-obligations)). |
| Adams 2015, *Towards the essence of hygiene*, POPL | `adams-2015-towards-the-essence-of-hygiene.pdf` ([source](https://michaeldadams.org/papers/hygiene/hygiene-2015-popl-authors-copy.pdf)) | A definition of hygiene independent of any expansion algorithm. | The specification the kernel is tested against. |
| Pombrio, Krishnamurthi and Wand 2017, *Inferring scope through syntactic sugar*, ICFP | `pombrio-2017-inferring-scope-through-sugar.pdf` ([source](https://cs.brown.edu/~sk/Publications/Papers/Published/pkw-inf-scope-syn-sugar/paper.pdf)) | Scope rules for surface syntax can be inferred from the desugaring. | Editors resolve names in macro arguments without expanding ([LSP-MACRO-NAVIGATE]). |
| Ullrich and de Moura 2020, *Beyond notations: hygienic macro expansion for theorem proving languages*, IJCAR; LMCS 2022 | `ullrich-2020-beyond-notations-lean4-macros.pdf` ([source](https://arxiv.org/pdf/2001.10490)) | Hygiene for a language with rich concrete syntax, by macro scopes on names, cheap enough for a proof assistant. | Hygiene that works through every flavor's concrete syntax. |

## Phases and modules

| Work | File | Establishes | Osprey takes |
| --- | --- | --- | --- |
| Flatt 2002, *Composable and compilable macros: you want it when?*, ICFP | `flatt-2002-composable-and-compilable-macros.pdf` ([source](https://www.cs.utah.edu/plt/publications/macromod.pdf)) | Phase separation: imports name the phase they are for, and compile-time state is never shared with run time. | [META-PHASE]. |
| Xie, White, Nicole and Yallop 2023, *MacoCaml: staging composable and compilable macros*, ICFP | `xie-2023-macocaml.pdf` ([source](https://www.cl.cam.ac.uk/~jdy22/papers/macocaml-staging-composable-and-compilable-macros.pdf)) | A typed calculus joining phase-separated macros and quotation-based staging, with soundness. | The formal model for typed staging across modules. |
| Chiang, Yallop, White and Xie 2024, *Staged compilation with module functors*, ICFP | `chiang-2024-staged-compilation-with-module-functors.pdf` ([source](https://xnning.github.io/papers/icfp24functor.pdf)) | Compile-time computation that depends on macros supplied by a functor argument. | How staging meets [signatures](../../specs/language/0025-ModulesAndNamespaces.md). |
| Tobin-Hochstadt, St-Amour, Culpepper, Flatt and Felleisen 2011, *Languages as libraries*, PLDI | `tobin-hochstadt-2011-languages-as-libraries.pdf` ([source](https://www.cs.utah.edu/plt/publications/pldi11-tscff.pdf)) | A whole language, including its type checker, delivered as a library through the macro system. | Evidence that the seam must be open; compare [type disciplines](../../specs/language/0050-TypeDisciplines.md). |
| Erdweg, Rendel, Kästner and Ostermann 2011, *SugarJ: library-based syntactic language extensibility*, OOPSLA | `erdweg-2011-sugarj.pdf` ([source](https://www.pl.informatik.uni-mainz.de/files/2019/04/sugarj.pdf)) | Grammar extensions imported like libraries; the cost is that parsing depends on imports. | The counter-example behind [META-KERNEL-PARSE]: Osprey sources parse without running anything. |

## Typed staging

| Work | File | Establishes | Osprey takes |
| --- | --- | --- | --- |
| Taha and Sheard 2000, *MetaML and multi-stage programming with explicit annotations*, TCS | not downloaded ([doi](https://doi.org/10.1016/S0304-3975(00)00053-0)) | Quote, splice and run with a type system that guarantees generated code is well typed. | `Code<T>` and level checking ([META-STAGE]). |
| Davies and Pfenning 2001, *A modal analysis of staged computation*, JACM | `davies-2001-modal-analysis-of-staged-computation.pdf` ([source](https://www.cs.cmu.edu/~fp/papers/jacm00.pdf)) | Staging is modal logic S4: `Code<T>` is the box modality over closed code. | The logical reading of levels. |
| Nanevski, Pfenning and Pientka 2008, *Contextual modal type theory*, TOCL | `nanevski-2008-contextual-modal-type-theory.pdf` ([source](https://www.cs.cmu.edu/~fp/papers/tocl07.pdf)) | Code with free variables is typed by the context it needs. | Open code fragments typed by their context. |
| Jang, Gélineau, Monnier and Pientka 2022, *Mœbius: metaprogramming using contextual types*, POPL | `jang-2022-moebius-contextual-types.pdf` ([source](https://arxiv.org/pdf/2111.08099)) | Pattern matching on typed code, with polymorphism, keeping type safety. | A sound basis for macros that inspect code, not only build it ([META-STAGE-ANALYSE]). |
| Hu and Pientka 2024, *Layered modal type theories* | `hu-2024-layered-modal-type-theory.pdf` ([source](https://arxiv.org/pdf/2305.06548)) | A layered system where code can be inspected while normalisation holds. | Same, for the proof disciplines. |
| Kiselyov 2014, *The design and implementation of BER MetaOCaml*, FLOPS | `kiselyov-2014-ber-metaocaml.pdf` ([source](https://okmij.org/ftp/meta-programming/ber-design.pdf)) | Effects during generation can move code out of its binder; the escape is detected when it happens. | [META-STAGE-EXTRUSION]: escaping code is always an error, never emitted. |
| Stucki, Biboudis and Odersky 2018, *A practical unification of multi-stage programming and macros*, GPCE | `stucki-2018-unification-of-multi-stage-programming-and-macros.pdf` ([source](https://biboudis.github.io/papers/pcp-gpce18.pdf)) | The phase consistency principle; macros are `inline` functions with a top-level splice. | One level rule for macros and staging ([META-STAGE-LEVEL]). |
| Berger, Tratt and Urban 2017, *Modelling homogeneous generative meta-programming*, ECOOP | `berger-2017-modelling-homogeneous-generative-meta-programming.pdf` ([source](https://arxiv.org/pdf/1602.06568)) | A calculus covering compile-time and run-time generation in one language. | Vocabulary for comparing the mechanisms. |
| Xie, Pickering, Löh, Wu, Yallop and Wang 2022, *Staging with class*, POPL | `xie-2022-staging-with-class.pdf` ([source](https://www.cl.cam.ac.uk/~jdy22/papers/staging-with-class.pdf)) | Constraints (type classes) need their own level discipline under staging. | Effect rows and constraints carried by `Code<T>` are level-checked too. |
| Sheard and Peyton Jones 2002, *Template meta-programming for Haskell*, Haskell Workshop | `sheard-2002-template-metaprogramming-for-haskell.pdf` ([source](https://www.microsoft.com/en-us/research/wp-content/uploads/2016/02/meta-haskell.pdf)) | Untyped AST construction with reification of declarations, checked after splicing. | The untyped escape hatch and declaration reflection ([META-REFLECT]); the kernel re-checks everything. |
| Rompf and Odersky 2010, *Lightweight modular staging*, GPCE | `rompf-2010-lightweight-modular-staging.pdf` ([source](https://infoscience.epfl.ch/record/150347/files/gpce63-rompf.pdf)) | Staging by types alone, with a graph IR that enables domain optimisation. | Staging as a library technique for DSLs such as GPU kernels. |
| Kovács 2022, *Staged compilation with two-level type theory*, ICFP | `kovacs-2022-staged-compilation-two-level-type-theory.pdf` ([source](https://arxiv.org/pdf/2209.09729)) | Two levels with a proof that staging removes the compile-time level entirely. | [META-TEMPLATE-GONE]: templates leave no run-time trace. |
| Kovács 2024, *Closure-free functional programming in a two-level type theory*, ICFP | `kovacs-2024-closure-free-functional-programming-2ltt.pdf` ([source](https://andraskovacs.github.io/pdfs/2ltt_icfp24.pdf)) | Fusion and monadic code compiled to closure-free first-order code by staging. | Zero-cost effects and iterators through templates. |
| Ganz, Sabry and Taha 2001, *Macros as multi-stage computations*, ICFP | not downloaded ([doi](https://doi.org/10.1145/507635.507646)) | Typed, generative, binding macros explained by translation to a staged language. | Macros defined by their staging meaning. |
| Parreaux, Voizard, Shaikhha and Koch 2018, *Unifying analytic and statically-typed quasiquotes*, POPL | not downloaded ([doi](https://doi.org/10.1145/3158101)) | Typed quasiquotes that also pattern-match code, tracking free variables in types. | Typed analysis of code ([META-STAGE-ANALYSE]). |
| Kameyama, Kiselyov and Shan 2011, *Shifting the stage*, JFP | not downloaded ([doi](https://doi.org/10.1017/S0956796811000256)) | Staging combined safely with delimited control, the basis of let-insertion. | Generation-time effects are Osprey effects with compiler handlers ([META-EFFECTS]). |

## Compile-time evaluation, templates and partial evaluation

| Work | File | Establishes | Osprey takes |
| --- | --- | --- | --- |
| Jones, Gomard and Sestoft 1993, *Partial evaluation and automatic program generation* | not downloaded ([book site](https://www.itu.dk/people/sestoft/pebook/)) | Binding-time analysis and the Futamura projections. | The theory behind compile-time evaluation and specialisation. |
| Würthinger et al. 2017, *Practical partial evaluation for high-performance dynamic language runtimes*, PLDI | `wurthinger-2017-practical-partial-evaluation.pdf` ([source](https://chrisseaton.com/truffleruby/pldi17-truffle/pldi17-truffle.pdf)) | Partial evaluation works at industrial scale when the boundaries are explicit. | Explicit `static` boundaries, never guessed. |
| DeVito et al. 2013, *Terra: a multi-stage language for high-performance computing*, PLDI | `devito-2013-terra.pdf` ([source](https://cs.stanford.edu/~zdevito/pldi071-devito.pdf)) | A low-level language staged from a separate high-level one, with types as compile-time values. | Types as compile-time values ([META-REFLECT]); Osprey uses one language for both levels. |
| Chlipala 2010, *Ur: statically-typed metaprogramming with type-level record computation*, PLDI | `chlipala-2010-ur-statically-typed-metaprogramming.pdf` ([source](http://adam.chlipala.net/papers/UrPLDI10/UrPLDI10.pdf)) | Generic code over record shapes, type-checked once, with no code generation. | Row-typed generic programming as an alternative to deriving by macro. |
| Magalhães, Dijkstra, Jeuring and Löh 2010, *A generic deriving mechanism for Haskell*, Haskell Symposium | `magalhaes-2010-generic-deriving-mechanism.pdf` ([source](https://dreixel.net/research/pdf/gdmh.pdf)) | Deriving as ordinary functions over a generic representation of a type. | [META-DECL-GENERIC]: prefer a typed representation over text when deriving. |

## Syntax extension and macro usability

| Work | File | Establishes | Osprey takes |
| --- | --- | --- | --- |
| Culpepper and Felleisen 2010, *Fortifying macros*, ICFP | `culpepper-2010-fortifying-macros.pdf` ([source](https://www2.ccs.neu.edu/racket/pubs/icfp10-cf.pdf)) | Syntax classes validate macro input and produce errors at the right term. | [META-SYNTAX-CLASS], [LSP-MACRO-DIAG-INPUT]. |
| Culpepper and Felleisen 2007, *Debugging macros*, GPCE | `culpepper-2007-debugging-hygienic-macros.pdf` ([source](https://www2.ccs.neu.edu/racket/pubs/gpce07-cf.pdf)) | A stepper over expansion with hiding policies. | [LSP-MACRO-EXPAND]. |
| Rafkind and Flatt 2012, *Honu: syntactic extension for algebraic notation through enforestation*, GPCE | `rafkind-2012-honu-enforestation.pdf` ([source](https://www.cs.utah.edu/plt/publications/gpce12-rf.pdf)) | Macros for infix languages by a precedence-aware step between reading and expansion. | Macros in brace and infix flavors. |
| Flatt et al. 2023, *Rhombus: a new spin on macros without all the parentheses*, OOPSLA | `flatt-2023-rhombus.pdf` ([source](https://kirancodes.me/pdfs/rhombus-oopsla23.pdf)) | Full Racket-class extensibility with conventional syntax, using shrubbery notation and binding spaces. | Token trees with delimiters and layout as the unparsed macro argument ([META-SYNTAX-TREE]). |
| Omar and Aldrich 2018, *Reasonably programmable literal notation*, ICFP | not downloaded ([doi](https://doi.org/10.1145/3236801)) | Typed literal macros: a delimited literal is parsed by a macro, with hygiene and a type known before expansion. | [META-SYNTAX-LITERAL]. |
| Pombrio and Krishnamurthi 2014, *Resugaring: lifting evaluation sequences through syntactic sugar*, PLDI | `pombrio-2014-resugaring.pdf` ([source](https://cs.brown.edu/~sk/Publications/Papers/Published/pk-resuarging/paper.pdf)) | Core-language steps can be shown as surface-language steps. | Diagnostics and debugging in the terms the user wrote ([LSP-MACRO-DIAG-BLAME]). |
| Pombrio and Krishnamurthi 2018, *Inferring type rules for syntactic sugar*, PLDI | not downloaded ([doi](https://doi.org/10.1145/3192366.3192398)) | Type rules for surface forms derived from the desugaring. | Type errors stated about the macro's surface form. |
| Chang, Knauth and Greenman 2017, *Type systems as macros*, POPL | `chang-2017-type-systems-as-macros.pdf` ([source](https://www.ccs.neu.edu/home/stchang/pubs/ckg-popl2017.pdf)) | A type checker written as macros that expand into the host language. | The link between macros and [type disciplines](../../specs/language/0050-TypeDisciplines.md). |
| Chang, Ballantyne, Turner and Bowman 2020, *Dependent type systems as macros*, POPL | `chang-2020-dependent-type-systems-as-macros.pdf` ([source](https://arxiv.org/pdf/2107.01295)) | The same for dependent types. | Same. |
| Ballantyne, *Building domain-specific languages with multi-language macros*, dissertation, Northeastern University (extends Ballantyne, King and Felleisen 2020, *Macros for domain-specific languages*, OOPSLA) | `ballantyne-dissertation-multi-language-macros.pdf` ([source](https://www2.ccs.neu.edu/racket/pubs/dissertation-ballantyne.pdf)) | A DSL declares its own grammar and binding rules and inherits hygienic expansion and editor support. | Embedded languages get binding-aware editor support ([LSP-MACRO-DSL]). |
| Christiansen and Brady 2016, *Elaborator reflection: extending Idris in Idris*, ICFP | not downloaded ([doi](https://doi.org/10.1145/2951913.2951932)) | The compiler's elaborator exposed as a typed effect to user code. | Compiler queries are an effect ([META-EFFECTS]). |

## External schemas and incremental builds

| Work | File | Establishes | Osprey takes |
| --- | --- | --- | --- |
| Syme et al. 2012, *Strongly-typed language support for internet-scale information sources*, Microsoft Research | `syme-2012-type-providers.pdf` ([source](https://www.microsoft.com/en-us/research/wp-content/uploads/2016/02/information-rich-themes-v4.pdf)) | Types computed at compile time from an external schema, live in the editor. | [META-PROVIDER]. |
| Christiansen 2013, *Dependent type providers*, WGP | not downloaded ([doi](https://doi.org/10.1145/2502488.2502495)) | Providers as ordinary compile-time programs. | A provider is a macro with declared inputs, not a second mechanism. |
| Mokhov, Mitchell and Peyton Jones 2018, *Build systems à la carte*, ICFP | `mokhov-2018-build-systems-a-la-carte.pdf` ([source](https://www.microsoft.com/en-us/research/uploads/prod/2018/03/build-systems.pdf)) | Build systems as a scheduler plus a rebuilder; early cutoff and verifying traces. | Expansion is a memoised query with recorded dependencies ([EXPAND-CACHE]). |

## Systems in production

These are documented by their own references, not by papers. Each taught the field something the specs depend on.

| System | Mechanism | Lesson |
| --- | --- | --- |
| Racket | Hygienic procedural macros, phases, `syntax-parse`, macro stepper | The complete design; its cost is that a file cannot be parsed or analysed without expanding it. |
| Lean 4 | Hygienic macros, elaborators, user syntax categories, info trees | Macros and the editor can share one record of what was elaborated. Parsing depends on imports. |
| Rust | `macro_rules!` (mixed-site hygiene), procedural macros over token streams | Token trees keep files parseable. Unsandboxed procedural macros are a supply-chain risk, a build-time cost and the hardest part of rust-analyzer. |
| Swift ([SE-0382](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0382-expression-macros.md), [SE-0389](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0389-attached-macros.md), [SE-0397](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0397-freestanding-declaration-macros.md)) | Macros with declared roles, run as sandboxed plug-ins | A macro declares what it may add and under which names; it only adds code; its expansion is viewable in the editor. |
| Scala 3 | `inline`, quotes and splices, TASTy reflection | Typed staging and macros as one feature, in production. |
| Zig | `comptime` | One language for both levels, types as values, no I/O at compile time; no hygiene problem because there is no syntax manipulation. |
| D | Compile-time function evaluation, templates, string mixins | Powerful and simple; text mixins lose locations and hygiene. |
| C++ ([P2996](https://wg21.link/p2996)) | Templates, `constexpr`, value-based reflection | Reflection as ordinary compile-time values is replacing template tricks. |
| C# | Incremental source generators | Generators only add code, and run as a pipeline of cached steps so the editor stays responsive. |
| Dart | Macros, cancelled in January 2025 | Deep semantic introspection by macros could not meet editor and hot-reload latency. A macro may ask the compiler only for what can be cached per declaration ([META-ORDER]). |
| F# | Type providers | External schemas become types at design time. |
| Template Haskell | Untyped and typed quotes with a stage restriction | Declaration order and recompilation suffer when splices are opaque to the build. |
| [dmx](https://dmx.dev/docs/) | Annotation macros for Dart: tree-sitter parse, context build, template render, hygiene, re-parse, emit; persistent workers in any language | A language-neutral pipeline whose last stages are unconditional: whatever produced the text, it is renamed, re-parsed and rejected if malformed. The [expansion engine](../../specs/compiler/ExpansionEngine.md) generalises this. |

## Open questions the research does not settle

- Typed code analysis with full polymorphism and effect rows is recent work [Jang et al. 2022; Hu and Pientka 2024]. The specs keep it behind an untyped, re-checked fallback.
- No published system gives hygienic macros, typed staging and declaration macros behind one incremental query engine with measured editor latency. The plan measures it before promising it.
- Hygiene has been proved for pattern macros [Herman and Wand 2008; Adams 2015], not for procedural macros in general. The proof obligations in the plan are scoped to what is known to be provable.
