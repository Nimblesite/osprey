# Module and namespace assertion suites

Eleven Default/ML twin pairs covering [Modules and Namespaces](../../docs/specs/0025-ModulesAndNamespaces.md).
Each pair is one program written on two surfaces and held to ONE golden, so
every claim below is asserted twice and the flavors are proved byte-identical
([FLAVOR-IR-EQUIV](../../docs/specs/0023-LanguageFlavors.md)).

| Pair | Spec sections |
| --- | --- |
| `namespace_surface` | `[MODULES-NAMESPACE]` `[MODULES-FILE-SCOPED-NAMESPACE]` `[MODULES-FLAVOR-PROJECTION]` |
| `module_boundary` | `[MODULES-MODULE]` `[MODULES-EXPORTS]` |
| `import_resolution` | `[MODULES-IMPORT]` `[MODULES-RESOLUTION]` |
| `signature_ascription` | `[MODULES-SIGNATURE]` `[MODULES-EXPORTS]` |
| `state_module_cells` | `[MODULES-STATE]` `[MODULES-STATE-MODULE]` `[MODULES-STATE-SOURCE-OF-TRUTH]` |
| `module_effects_rows` | `[MODULES-EFFECTS]` |
| `module_data_types` | `[MODULES-MODULE]` `[MODULES-EXPORTS]` with records, unions and generics |
| `module_composition` | `[MODULES-ABI]` `[MODULES-PROJECT]` with fibers, iterators and `Result` |
| `file_scope_bindings`, `file_scope_generic_binding` | `[MODULES-FILE-SCOPE-BINDING]` `[MODULES-INIT]` |
| `ml_layout_depth` | `[FLAVOR-ML-LAYOUT]` `[MODULES-FLAVOR-PROJECTION]` — four nesting levels, an indented import selection, an effect declared four segments deep |

Rejection paths are not here. A must-reject module program never reaches
codegen, so it cannot carry a golden; those live as source-driven diagnostics in
[`crates/osprey-project/tests/module_diagnostics.rs`](../../crates/osprey-project/tests/module_diagnostics.rs),
which asserts each message on BOTH surfaces. Note that
`examples/failscompilation/` is the wrong home for them: that corpus is graded by
parse → type-check → codegen and never runs `osprey_project::assemble`, so a
module-assembly rejection written there would be graded on a path that cannot
produce it.

## Known defects pinned red

Writing these suites surfaced eight defects. Each has a failing test; none has
been worked around in the passing suites above, and the passing suites are
written to steer clear of the broken shapes so they measure what they claim to.

The red tests live in
[`crates/osprey-project/tests/module_defects.rs`](../../crates/osprey-project/tests/module_defects.rs)
and
[`crates/osprey-cli/tests/effect_installer_defects.rs`](../../crates/osprey-cli/tests/effect_installer_defects.rs).

### Silent — the program exits zero having done the wrong thing

1. **A generic handler installer applied at two body result types renders a raw
   pointer.** `fn feeding(reading, body) = handle Feed … in body()` used once
   with a `string`-returning body and once with an `int`-returning one, both
   evaluated inside a top-level statement's interpolation, prints a
   machine-dependent integer where the string belonged — and exits `0`.
   Reversing the two statements turns it into a SIGSEGV instead. Binding each
   call to a `let`, or making the calls from inside a function body, avoids it.
   Reproduces under `default`, `gc` and `arc`.

2. **A second file-scoped `namespace` header silently discards everything after
   it (ML).** The ML lowering nests the second header inside the first
   namespace's body instead of closing it, and `osprey_project::contribution`
   only walks top-level `Stmt::Namespace`. The second namespace's declarations
   vanish and every statement written after the header never runs. Empty output,
   exit `0`, no diagnostic.

3. **A second file-scoped `namespace` header is ignored outright (Default).**
   Declarations after it are filed under the FIRST namespace, so a source that
   wrote `two::B::v` gets a declaration answering to `one::B::v` and a call site
   blamed for an unknown path.

4. **`main` beside a top-level statement is not rejected under project
   assembly.** [MODULES-ENTRYPOINT] forbids one source declaring two entries, and
   `examples/failscompilation/main_beside_top_level_statement.ospo` pins the
   rejection for a plain program. Add a `namespace` and the check is skipped:
   both entries are kept and both run, statement first.

### Loud — rejected or crashing, but wrong

5. **A curried ML installer emits an unlinkable resume trampoline.** ML's
   `feeding reading body = handle … in body ()` fails to link with
   `undefined symbol "_body"` from `___resume_body_Feed_0`. The tupled head
   `feeding (reading, body)`, which lowers to a flat parameter list, compiles —
   so currying and resume-body outlining disagree, against [FLAVOR-ML-CURRY].

6. **The ML spec's own single-line assigning handler arm does not parse.**
   [FLAVOR-ML-BIND] prints `tick => requests := (requests + 1) ?: requests`.
   Only the indented-block form is accepted; either the grammar or the spec page
   is stale.

7. **A file-scope handler and a file-scope generic binding cannot coexist.**
   [MODULES-FILE-SCOPE-BINDING] documents both in the same section, and each
   half compiles alone, but together they are rejected with "program entry
   invokes a dynamic callable whose effect provenance cannot be proven". A
   generic binding has no runtime value to store ([TYPE-GENERICS-FN]), so it
   cannot be the dynamic callable — the check is over-approximating. This is why
   `file_scope_generic_binding` is a separate program from `file_scope_bindings`.

8. **An exported module union has no reachable constructors.**
   [MODULES-OPAQUE-TYPES] singles out OPAQUE union constructors as private,
   which only means something if a plain exported union's are public. Neither
   `Circle` nor `Geo::Circle` resolves outside the module, so an exported type
   can only be constructed if the module also exports a factory per variant.
   `module_data_types` therefore builds every value through a factory, and says
   so in its header.
