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

## Handler boundaries

The effect suites use the current callable and block-scoped forms. A handler installer can use a block-scoped `handle Feed` followed by `body()`; a reusable policy can return `handler Feed { read => reading }` and be applied as `policy(work)`. ML uses the same operations with layout syntax. File-scope installation and the former `handle ... in/do ...` syntax are rejected.

Module regressions live in [`module_defects.rs`](../../crates/osprey-project/tests/module_defects.rs) and [`effect_installer_defects.rs`](../../crates/osprey-cli/tests/effect_installer_defects.rs). Their assertions cover namespace contributions, entry points, generic handler results, curried callbacks, and state captures. Consult [plan 0016](../../docs/plans/0016-algebraic-effects-and-handlers.md) for current effects limitations.
