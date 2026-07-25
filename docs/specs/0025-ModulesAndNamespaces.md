# Modules and Namespaces

Osprey projects use logical namespaces and closed modules. File paths select
project sources but do not determine exported names.

Both source flavors lower module syntax to the same AST before project
assembly. The project layer then resolves imports, validates boundaries, and
flattens the graph into one canonical program.

## Canonical Project Model `[MODULES-MODEL]`

`osprey_project::SourceFile` contains a physical path, selected flavor, source
text, and canonical `Program`. Project assembly performs these steps in sorted
source-path order:

```text
discover .osp/.ospml sources
  -> parse each source with its selected flavor
  -> extract logical namespace contributions
  -> collect modules, declarations, signatures, and constants
  -> validate imports, exports, signatures, and state ownership
  -> resolve names and flatten to one Program
```

`AssembledProject` retains the flat program, entry source, per-source position
ranges, and a map from internal linkage names back to source-level names.

## Surface Projection `[MODULES-FLAVOR-PROJECTION]`

| Concept | Default | ML | Canonical AST |
| --- | --- | --- | --- |
| namespace | `namespace app { ... }` or `namespace app;` | `namespace app` with an optional indented body | `Stmt::Namespace` |
| module | `module M { ... }` | `module M` plus layout body | `Stmt::Module { kind: Plain }` |
| state module | `state module M { ... }` | `state M` plus layout body | `Stmt::Module { kind: State }` |
| import | braced member selection | indented member selection | `Stmt::Import` |
| signature | braced items | indented items | `Stmt::Signature` |
| symbol path | `app::M::name` | same | `Expr::Path` |

Project assembly does not retain which surface produced a declaration.

## Namespaces `[MODULES-NAMESPACE]`

A namespace is an open logical group. Multiple files and both flavors may
contribute to the same label. Duplicate declarations in the merged namespace
are errors.

Default:

```osprey
namespace billing;

fn zero() = 0
```

ML:

```osprey-ml
namespace billing

zero () = 0
```

Identifier labels and quoted labels are distinct. A quoted label such as
`"billing/api"` is opaque: `/` does not create a parent namespace.

### File-scoped Namespaces `[MODULES-FILE-SCOPED-NAMESPACE]`

Default `namespace name;` and an ML namespace header without an indented body
apply to the declarations that follow. A namespace with a brace/layout body is
one block-scoped contribution.

### Path Independence `[MODULES-PATH-INDEPENDENCE]`

The source path is used for discovery and diagnostics only. Moving
`src/a.ospml` to `src/deep/b.ospml` does not change the namespace or symbol
identity written in that source.

## Modules `[MODULES-MODULE]`

A plain module is a closed, stateless declaration boundary. It may contain
immutable values, functions, types, effects, external declarations, and nested
plain modules. Direct module-level `mut` is rejected; local mutation inside a
function remains legal.

```osprey
namespace billing;

module Tax {
    let rate = 10
    export fn add(cents: int) -> int = cents + cents * rate / 100
}
```

```osprey-ml
namespace billing

module Tax
    rate = 10
    export add cents = cents + cents * rate / 100
```

Unascribed module items are private unless marked `export`. An ascribed module
exports the items named by its signature.

## Imports `[MODULES-IMPORT]`

Imports target logical namespaces or modules, never files.

Default:

```osprey
import billing::Tax
import billing::Tax::{add, zero as noTax}
import billing::Tax as T
import billing::Tax::*
```

ML:

```osprey-ml
import billing::Tax
import billing::Tax as T
import billing::Tax
    add
    zero as noTax
import billing::Tax
    *
```

A whole module import binds its final path segment, or the explicit alias. A
member import binds only the selected exported members. Quoted namespace labels
require `as Alias` for whole imports.

Wildcard imports require `[modules].allow_wildcard_imports = true` and are
always forbidden for a state module or a namespace containing one. Duplicate
import bindings are ambiguous errors; imports never replace a nearer local or
module declaration.

### Name Resolution `[MODULES-RESOLUTION]`

Bare names resolve from the innermost current module outward, then through
imported members. Qualified paths try the current module and parents, imported
aliases/members, and explicit namespace labels. Crossing a private intermediate
module is an error.

`::` qualifies logical declarations. `.` remains record/member access.

## Exports and Visibility `[MODULES-EXPORTS]`

Namespace declarations are visible across their namespace graph. Module items
are private by default. An unascribed module uses `export`; an ascribed module's
signature is its public surface.

An import of a private member or traversal through a private nested module is an
error. A signature cannot export a state cell. In Default, `: Signature + extra`
permits additional explicitly exported items; otherwise extra exports are an
error. ML ascription is exact and rejects redundant `export` markers.

### Opaque Types `[MODULES-OPAQUE-TYPES]`

The syntax and graph retain opaque type metadata, and opaque union constructors
are private. A manifest opaque alias such as
`export opaque type UserId = int`, including an implementation of an abstract
signature type by such an alias, is rejected during flattening with an
`opaque alias ... unsupported` diagnostic. The compiler must reject this case
rather than expose `int` to clients.

## Signatures `[MODULES-SIGNATURE]`

A signature lists exported values, functions, types, effects, and nested
modules. A module ascription resolves a signature relative to the module's
namespace and parent module.

```osprey
signature StoreApi {
    opaque type Store
    effect StoreFx {
        load : fn() -> Store
    }
    fn empty() -> Store
}
```

```osprey-ml
signature StoreApi
    type Store
    effect StoreFx
        load : Unit => Store
    empty : Unit -> Store
```

Conformance checks declaration kind, generic arity, parameter count and types,
return type, declared effect row, manifest type representation, effect
operations, and nested-module ascription. Every signature item needs an
implementation. Non-exported implementation details remain private.

In an ML signature, bare `type T` is abstract and `type T = R` is manifest;
`opaque type T` is redundant and rejected.

## State Ownership `[MODULES-STATE]`

Local `mut` remains lexical. Durable module-owned cells may occur only in a
state module and may be accessed only inside that module's own lexical effect
handler arms.

### Forbidden Top-level State `[MODULES-STATE-TOPLEVEL]`

A direct `mut` in a namespace or plain module is an error. `export mut` is
always an error. State-module cell initializers must be pure and may not depend
on project declarations.

### State Modules `[MODULES-STATE-MODULE]`

A state module with cells must expose at least one owned effect and at least one
exported function whose body contains a lexical handler for that effect. The
assembler removes cell declarations from module scope and injects fresh cells
into each qualifying installer function.

Only the handler arms may read or write those cells. Ordinary functions,
nested modules, lambdas, and spawned bodies do not acquire ownership by
containing a handler. Qualified aliases cannot bypass this check.

Each namespace may contain at most one state module. Importing a state module
allocates no cells; calling an installer creates a fresh instance.

### Cross-Module State Access `[MODULES-STATE-SOURCE-OF-TRUTH]`

All cross-module state access is mediated by the exported effect operations.
Direct reads and writes, including alias-qualified paths, are rejected outside
the owning handler arms.

## Effects and Capabilities `[MODULES-EFFECTS]`

Module boundaries do not erase effect rows. Signature functions and effects
retain their generic binders, operation payloads/results, and declared effect
rows. Importing a module does not install or handle an effect.

A state module exposes state access through an owned algebraic effect and a
lexical handler installer. Callers must handle that effect or propagate it in
the ordinary language rules.

## Initialisation `[MODULES-INIT]`

Imports have no runtime effect. Pure immutable constants may be inlined during
assembly. Effectful setup belongs in an explicit function. State cells are
initialized inside a qualifying installer, so each call gets fresh cells.

Constant initializer cycles and type-alias cycles are rejected before code
generation.

## Project Assembly `[MODULES-PROJECT]`

A project input is a directory or `osprey.toml`. Source roots are scanned
recursively for exact `.osp` and `.ospml` extensions; hidden directories and
`target` are skipped.

```toml
[project]
name = "billing"
source_roots = ["src"]
default_namespace = "billing"
entry = "src/main.ospml"
# flavor = "ml"            # optional project-wide override

[modules]
allow_wildcard_imports = false
```

Files without a namespace contribute to `default_namespace`, or to the project
name when it is absent. A configured `flavor` is passed as an explicit flavor
override for every project source; otherwise markers and extensions select each
file independently.

A manifest-free directory uses its directory name as project/default namespace
and scans `src` when present, otherwise the directory itself.

### Entry Point `[MODULES-ENTRYPOINT]`

Entry selection uses this order:

1. configured `[project].entry`;
2. a unique source containing a namespace-level `main`;
3. a unique source containing top-level executable statements;
4. the only source in a one-file project.

Zero or multiple candidates are errors. Namespace-level `main` and executable
top-level statements are rejected outside the selected entry source. Project
`main` cannot take parameters.

## Cycles `[MODULES-CYCLES]`

The cycle checks reject immutable constant-initializer cycles and type
alias cycles. No parameterised or recursive module semantics are implied.

## Name Mangling and ABI `[MODULES-ABI]`

Every project declaration has a source identity such as
`billing::Tax::add`. Internal non-extern linkage names encode every namespace
and module path segment deterministically and collision-free. Extern declarations
retain their external symbol name. The assembled project keeps a reverse map so
symbol output and project diagnostics can restore source-level names.

The selected entry function links as `main`.

## Diagnostics `[MODULES-DIAG]`

Project diagnostics include the source path and local position when available.
The implemented checks report unknown/private imports, ambiguous bindings,
duplicate declarations, private path traversal, signature mismatches, opaque
alias rejection, state ownership violations, entry conflicts, and initializer
cycles.

## Tested Example

[`examples/projects/modules/`](../../examples/projects/modules/) is the
end-to-end project fixture. `crates/osprey-cli/tests/project_e2e.rs` checks
directory/manifest inputs, AST flattening, LLVM output, source-name restoration,
and byte-exact execution. `crates/osprey-project/tests/` covers graph,
visibility, signature, state, entry, cycle, and opaque-boundary behavior.
