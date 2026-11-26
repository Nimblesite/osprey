# Macro tooling: diagnostics, editor, debugger [LSP-MACRO]

**Status:** **specified, not implemented.** Delivery is [plan 0062](../../plans/0062-metaprogramming.md). This is the tooling contract for [compile-time metaprogramming](../language/0056-Metaprogramming.md), run by the [expansion engine](../compiler/ExpansionEngine.md). It extends the [language server](0020-LanguageServerAndEditors.md) and changes none of its existing rules.

## Aim `[LSP-MACRO-AIM]`

Code that uses macros is as easy to write, read, navigate and debug as code that does not. A person never has to read expanded code to understand an error, never loses an editor feature inside a macro's arguments, and can always see exactly what a macro produced.

The compiler and the language server share one expansion: the same engine, the same cache, the same origin records and the same diagnostics. There is no editor-only approximation of what a macro does.

## Origin `[LSP-MACRO-ORIGIN]`

Every tool below reads the origin record of [META-KERNEL-ORIGIN](../language/0056-Metaprogramming.md#kernel-meta-kernel). A node of expanded code is one of:

- **written:** it came from the invocation's arguments, and has the span the user typed;
- **introduced:** it came from the expander, and has a span in the expander's definition (or in a template) plus the invocation that produced it.

Both carry the chain of invocations when macros expand to macros. A tool that has a position in expanded code can always answer two questions: where did the user write this, and which macro added it.

## Diagnostics `[LSP-MACRO-DIAG]`

One renderer produces every diagnostic for the CLI and the editor, so `osprey check` and the Problems panel never disagree.

- `[LSP-MACRO-DIAG-INPUT]` **Bad input is reported by the macro, at the term, before expansion.** A mismatch against a declared syntax class ([META-SYNTAX-CLASS](../language/0056-Metaprogramming.md#syntax-macros-meta-syntax)) names the term, the class expected and the macro. A macro's own `Diagnose` reports appear at the span it chose inside its arguments, with its notes and fixes.
- `[LSP-MACRO-DIAG-BLAME]` **An error in expanded code is placed where a person can act on it.**
  - If the failing node is *written*, the diagnostic is at that span in the user's file, worded as for hand-written code, with one note naming the macro it passed through.
  - If the failing node is *introduced*, the diagnostic is at the invocation. Its message says the macro produced code that does not check, quotes the smallest failing fragment, and links the expander's definition and the expansion view. This is the macro author's defect and is labelled so.
  - A type error between a written node and an introduced one is reported at the written node, stated in terms of the macro's declared input where it has one [Pombrio and Krishnamurthi 2014].
- `[LSP-MACRO-DIAG-VISIBLE]` A diagnostic never has a primary position in text the user cannot see. Positions inside an expansion appear only as related information that opens the expansion view.
- `[LSP-MACRO-DIAG-CHAIN]` Nested expansion is shown as related information, innermost first, capped at a fixed depth with the rest counted.
- `[LSP-MACRO-DIAG-ENGINE]` Engine failures have their own codes and are reported at the invocation: step or memory limit, deadline, crash, malformed answer, non-determinism, undeclared input, expansion cycle, depth limit, staging escape ([META-STAGE-EXTRUSION](../language/0056-Metaprogramming.md#typed-staging-meta-stage)). Each says what happened and what to change.
- `[LSP-MACRO-DIAG-DEDUPE]` A template instantiated many times reports a defect in the template once, with the instantiations listed, not once per use.
- `[LSP-MACRO-DIAG-FIX]` Quick fixes offered by a macro edit only the user's arguments. A fix that would edit expanded code is not offered.
- Warnings for unused bindings and unreachable code are not reported on introduced nodes; they are reported to the macro's author when the macro's own package is checked.

## Latency `[LSP-MACRO-LATENCY]`

The existing rule holds: diagnostics never delay a request ([LSP-DIAGNOSTICS](0020-LanguageServerAndEditors.md)).

- Expansion for the editor runs on the analysis thread and is cancelled when its file changes again.
- A request that needs an expansion still being computed answers from the last complete one and refreshes when the new one arrives. Hover and completion are never blocked on a worker.
- The cache of [EXPAND-CACHE](../compiler/ExpansionEngine.md#cache-expand-cache) is shared with builds. Editing inside a function re-expands only invocations whose key changed.
- An expander that exceeds its editor deadline is reported once at the invocation and retried on save, not on every keystroke.
- The module outline never needs an expander ([META-ORDER-OUTLINE](../language/0056-Metaprogramming.md#order-of-expansion-meta-order)), so workspace symbols, imports and cross-file navigation work before any macro has run.

## Navigation and editing `[LSP-MACRO-NAVIGATE]`

- **Inside arguments,** hover, go to definition, find references, rename, completion, signature help and semantic highlighting behave as if the argument were ordinary code. The server maps the position to the written nodes in the expansion and answers from them; when one argument is used several times, answers that agree are merged and answers that differ are all shown.
- **Completion on incomplete code** expands speculatively with a marker at the cursor. An expander receives syntax with error nodes and is expected to return what it can; one that cannot simply yields no completions there, and the rest of the file is unaffected.
- **On the invocation's name,** hover shows the macro's documentation, mechanism, roles and declared names, and a summary of what this invocation added. Go to definition opens the expander.
- **On a generated name** used in ordinary code (a member a declaration macro added), go to definition goes to the invocation that declared it, with the expansion view one step away. Hover shows the generated signature and the macro responsible.
- **Rename** of a user's name updates every written occurrence, including inside macro arguments. A name that exists only because a macro declared it is renamed by changing the macro's arguments or not at all; the server explains which.
- **Find references** includes uses in introduced code, marked as generated and grouped by macro.
- **Code lens and inlay hints** show inferred types for bindings a macro introduces into the user's scope.
- Answers are rendered in the flavor of the file being edited, whatever flavor the macro was written in ([LSP-FLAVOR-RENDER](0020-LanguageServerAndEditors.md)).

## Seeing expansions `[LSP-MACRO-EXPAND]`

- **Expand macro** is a code action and a command on every invocation. It opens a read-only virtual document holding the formatted expansion in the reader's flavor, with introduced code visually distinct from written code.
- The view steps: expand once, expand fully, and step through nested expansions one at a time [Culpepper and Felleisen 2007]. Macros from the standard distribution can be hidden so a user sees their own macros' steps.
- Positions in the view link back: selecting generated code reveals the template or definition that produced it; selecting written code reveals the argument.
- `osprey expand <file>` prints the same text, and `osprey expand --json` the origins, so the CLI, CI and agents see what the editor sees.
- The view is never a file in the repository and is never edited.

## Embedded languages `[LSP-MACRO-DSL]`

A delimited literal ([META-SYNTAX-LITERAL](../language/0056-Metaprogramming.md#syntax-macros-meta-syntax)) is a small language inside a file. Its macro may offer editor services for the body through the engine's protocol: semantic tokens, diagnostics with spans in the body, completion, hover and formatting. The server forwards them and merges the results with its own.

A literal whose macro offers nothing is treated as a string with balanced brackets: it is highlighted plainly, never reformatted, and its diagnostics are the macro's.

## Formatting `[LSP-MACRO-FORMAT]`

The formatter formats arguments that are ordinary syntax. It does not change a token tree unless that literal's macro supplies a formatter. Formatting never depends on an expansion.

## Debugging, coverage and profiling `[LSP-MACRO-DEBUG]`

- Debug information maps introduced code to the expander's definition and written code to the user's file ([debug artifacts](../compiler/DebugArtifacts.md)). Stepping over an invocation is one step; stepping in enters the expansion view at the right line.
- A stack trace shows the invocation's frame with the macro named, not a line in text that exists nowhere.
- Coverage attributes introduced code to the expander's definition, measured in the macro's own package and tests. A user's coverage counts their written code only, so adding a derive never lowers a project's coverage.
- The profiler attributes time in introduced code to the invocation, grouped by macro.

## Documentation `[LSP-MACRO-DOCS]`

[Documentation tools](DocumentationTools.md) list members a declaration macro adds, each marked with the macro that produced it and carrying documentation the macro supplied. A macro's own documentation states its mechanism, roles, declared names and inputs, taken from its declaration, not from prose.

## Verification `[LSP-MACRO-VERIFICATION]`

Every rule here has a language-server test in `src/toolchain/crates/osprey-lsp` and, where it reaches the editor, a VS Code test.

- A corpus of faulty programs pairs each defect (bad macro input, a type error in an argument, a defect in a template, each engine failure) with its exact diagnostic: position, message, related information.
- The same corpus is checked through the CLI; the two outputs must match.
- Each navigation feature is tested at positions inside arguments for every invocation shape and every flavor.
- A test asserts that no published diagnostic has a primary range outside the user's files.
- Latency tests hold the editor budget of [EXPAND-BUDGETS](../compiler/ExpansionEngine.md#budgets-expand-budgets) with a slow worker attached: requests still answer within their existing limits.

## References

Every address below was tested on 2026-11-27: a PDF address returns the same bytes as the downloaded copy, and a DOI resolves to the work's record, whose title Crossref confirms. The [research basis](../../research/metaprogramming/README.md) says what each work establishes and what Osprey takes from it.

- [Ballantyne, *Building domain-specific languages with multi-language macros*, dissertation, Northeastern University (extends Ballantyne, King and Felleisen 2020, *Macros for domain-specific languages*, OOPSLA)](https://www2.ccs.neu.edu/racket/pubs/dissertation-ballantyne.pdf)
- [Culpepper and Felleisen 2007, *Debugging macros*, GPCE](https://www2.ccs.neu.edu/racket/pubs/gpce07-cf.pdf)
- [Culpepper and Felleisen 2010, *Fortifying macros*, ICFP](https://www2.ccs.neu.edu/racket/pubs/icfp10-cf.pdf)
- [Pombrio and Krishnamurthi 2014, *Resugaring: lifting evaluation sequences through syntactic sugar*, PLDI](https://cs.brown.edu/~sk/Publications/Papers/Published/pk-resuarging/paper.pdf)
- [Pombrio, Krishnamurthi and Wand 2017, *Inferring scope through syntactic sugar*, ICFP](https://cs.brown.edu/~sk/Publications/Papers/Published/pkw-inf-scope-syn-sugar/paper.pdf)
