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

## Regression coverage

The original module and effect-installer defects found on 2026-08-22 are fixed and remain mandatory regression tests. Their historical repros live in [`module_defects.rs`](../../crates/osprey-project/tests/module_defects.rs) and [`effect_installer_defects.rs`](../../crates/osprey-cli/tests/effect_installer_defects.rs).

- Repeated file-scoped namespaces retain their declarations and executable statements in both flavors.
- Project assembly rejects conflicting entrypoints and exposes ordinary exported union constructors while retaining opaque boundaries.
- Generic handler installers preserve string and integer results in either statement order, and curried ML installers emit callable resumable bodies.
- Single-line ML handler arms may assign to their cells, and a file-scope handler may coexist with a generic binding.
- Handler-owned mutable cells remain values when passed to inferred helpers returning `Result`.

The `file_scope_generic_binding` twins additionally pin declaration scope through direct calls, aliases, higher-order calls, iterator callbacks and typed record fields. They distinguish global storage from shadowing caller values and genuine local closure captures. Curried calls retain their caller arguments, nested calls use independent type instantiations, and an effect transcript proves first argument → body → second argument ordering with no duplicated work. Both flavors share one exact golden under every supported target and memory backend.
