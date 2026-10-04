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

### Project Style Advice `[MODULES-STYLE]`

Style findings are warnings, never rejection criteria. `namespace-folder-drift` reports each contributing source when one namespace spans more than one physical parent folder; no folder-to-namespace naming convention is imposed. `module-deep-hierarchy` reports module declarations nested more than three levels. `namespace-reverse-domain` reports application namespace labels starting with `com.`, `org.`, `net.`, `io.`, `dev.` or `edu.` and containing at least three nonempty dot-separated components. Dots and slashes remain ordinary label characters, never namespace ancestry.

`[modules].published_library = true` suppresses the application-only hierarchy and reverse-domain advice. It does not suppress folder drift or state ownership warnings. Omitted, it defaults to `false`; a non-boolean value is a manifest error. CLI and LSP consume the same sorted, source-located findings from project assembly.

## Modules `[MODULES-MODULE]`

A plain module is a closed, stateless declaration boundary. It may contain
immutable values, functions, types, effects, external declarations, and nested
plain modules. Direct module-level `mut` is rejected. A `mut` cell inside a
function is intended for handler-owned effect state
([Bindings](0003-Syntax.md#bindings)); the checker restricts every local
reassignment to an effect handler arm.

```osprey
namespace billing;

module Tax {
    let rate = 10
    export fn add(cents: int) -> int = {
        let scaled = cents * rate
        let tax = intDiv(scaled, 100)
        let answer = cents + tax
        answer
    }
}
```

```osprey-ml
namespace billing

module Tax
    rate = 10
    export add cents =
        scaled = cents * rate
        tax = intDiv (scaled, 100)
        answer = cents + tax
        answer
```

A module that needs to expose a failure may export a `Result`-returning function; module boundaries do not erase its error channel. Nor do they erase an arithmetic fault: it crosses the boundary as an undischarged requirement and is rejected at the program entry if nothing handles it, so the policy is installed once per program rather than once per module ([ARITH-TOTAL](0037-ArithmeticEffects.md#the-guarantee--arith-total)).

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
error. A signature's `let` entry names an immutable value; an ascribed state
module whose cell matches the entry is rejected with
`state cell ... cannot be exported by a signature`. In Default, `: Signature + extra`
permits additional explicitly exported items; otherwise extra exports are an
error. ML ascription is exact and rejects redundant `export` markers.

### Opaque Types `[MODULES-OPAQUE-TYPES]`

`export opaque type` keeps a type's representation inside the module that
declares it (an abstract `type T` in an ascribed signature has the same effect).
Outside that module the type is a name only: constructing a value, destructuring
one with a constructor or structural pattern, reading a field and updating a
field are all rejected (``opaque type `M::T` cannot be constructed outside
module `M` ``, ``... destructured ...``, ``field `f` of opaque type `M::T` is
hidden outside module `M` ``) and an opaque union's constructors are private.
The module's exported functions are the only way through, including a generic
accessor it exports: a field obligation records the declaration it was written
in, so it keeps its rights when it travels to a client's call. A module constant
whose initializer reads an opaque field is inlined at each use and is therefore
rejected in a client; export a function instead.

A manifest opaque alias such as `export opaque type UserId = int`, including an
implementation of an abstract signature type by such an alias, is rejected
during flattening with `opaque alias ... unsupported`: the flat checker would
expose `int` to clients, and rejecting is the truthful answer.

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

Alias expansion preserves annotation provenance: a type inserted by an ascription remains a contract constraint and cannot produce a redundant-annotation warning or deletion action. A written annotation retains its own source identity through chained and generic aliases; separate uses remain separately removable under [TYPE-ANNOTATION-REDUNDANT](0004-TypeSystem.md#redundant-annotations--type-annotation-redundant). `alias_expansion_preserves_*` and the editor alias-action tests enforce both flavors.

The runnable bank’s `MoneyApi` exposes manifest `Cents = int` and abstract `Amount`, implemented by a private record. Clients convert through `fromCents`/`toCents`; they cannot construct the record, read its fields or substitute a raw integer for an amount. `bank_money_signature_*` exercises the actual source from both syntax flavors under every native allocator. The same module supplies the Wasm browser application. This example does not require or imply support for opaque manifest aliases.

## State Ownership `[MODULES-STATE]`

Local `mut` remains lexical. Durable module-owned cells may occur only in a
state module and may be accessed only inside that module's own lexical effect
handler arms.

### State Boundary Inventory `[MODULES-STATE-INVENTORY]`

Every resolved state module, including an empty or private owner, contributes one inventory entry: its qualified source name, declaration location, private cell count and sorted exported owned effect names. Cell names, initializers and private helper declarations are excluded. Every declaration receives a `state-boundary` warning describing that entry. This makes state ownership visible without changing acceptance or implying that importing a module installs a handler. Each installer still creates fresh cells.

The CLI, LSP and generated project documentation consume this same inventory. Live editor changes replace it together with the checked project. Invalid syntax, assembly or types cannot justify a stale ownership report.

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

### File-scope Bindings `[MODULES-FILE-SCOPE-BINDING]`

A `let` or `mut` written at the file scope of the entry source is a
declaration, not a statement of the entry. Its initializer runs where it is
written, in source order, before the entry runs — so a source may carry both a
`main` and the bindings `main` reads.

A function body may read a file-scope binding declared above it. The binding
then has module storage rather than a slot in the entry's frame, and every read
observes the current value at that storage. A `mut` an effect handler owns is
one shared cell ([EFFECTS-HANDLER-STATE](0017-AlgebraicEffects.md#handler-owned-state)),
so a function reads what the arms most recently wrote and not a copy taken at
declaration time. This does not relax
[MODULES-STATE-TOPLEVEL](#forbidden-top-level-state-modules-state-toplevel): a
`mut` directly inside a namespace or plain module remains an error.

Three programs are rejected, because each would otherwise read storage that
holds no bound value or the wrong one:

- A statement that reaches a binding *before* its initializer runs, through any
  function it can call. Naming a function counts as calling it, since a name
  handed to a higher-order function is invoked out of sight.
- A file-scope name that is rebound and also read from a function body. Later
  statements may see a newer binding; one module slot cannot hold both.
- Reading a binding declared below the function that names it, which is already
  an unknown identifier.

```osprey
mut hits = 0
let counter = handler Counter {
    tick amount => {
        hits = hits + amount
        hits
    }
}
let total = counter(run)

// Reads the live cell and the finished total, from outside the entry.
fn summary() = "${hits} hits, total ${total}"
```
A binding whose value is a GENERIC function is the one case with no module
storage at all. A generic definition has no single machine-level shape, so it
exists only as a body each call site specialises
([TYPE-GENERICS-FN](0004-TypeSystem.md)); binding it produces no runtime value
to store. `let alias = identity` and `let idl = |x| => x` therefore resolve by
NAME, and a function that calls one specialises the same body its own arguments
fix. Every such call site is independent: the binding is not narrowed to
whichever type the first caller used.

```osprey
fn identity(x) = x
let alias = identity

// Specialised twice from one binding, before the entry runs a statement.
fn round(n) = alias(n)
fn label(s) = alias(s)
```

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
published_library = false
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

A source declares one entry, so a `main` and a top-level executable statement
cannot share it: candidates 2 and 3 would both select that source and only one
of them could run. A file-scope `let` or `mut` is a declaration and stays where
it is ([MODULES-FILE-SCOPE-BINDING](#file-scope-bindings-modules-file-scope-binding));
a bare expression or an assignment beside `main` is an error.

## Cycles `[MODULES-CYCLES]`

The cycle checks reject immutable constant-initializer cycles and type
alias cycles. No parameterised or recursive module semantics are implied.

## Name Mangling and ABI `[MODULES-ABI]`

Every project declaration has a source identity such as
`billing::Tax::add`. Internal non-extern linkage names encode every namespace
and module path segment deterministically and collision-free. Extern declarations
retain their external symbol name. The assembled project keeps a reverse map so
symbol output and project diagnostics can restore source-level names.

The selected entry function links as `main`. Native LLVM symbols retain that ABI while debug metadata emits the qualified source identity as the subprogram `name`, so debugger stack frames show names such as `billing::Tax::add`. The optional DWARF linkage name is omitted: LLDB otherwise prefers the encoded name even though it cannot demangle it. Generated handler names also restore embedded module identities. `module_debug_frames_keep_source_names_in_both_flavors` pins the distinction for both source flavors; the editor’s `module stack frames retain their source names` tests stop in real LLDB sessions and assert the frame name, source line and parameter value.

## Diagnostics `[MODULES-DIAG]`

Project diagnostics include the source path and local position when available.
The implemented checks report unknown/private imports, ambiguous bindings,
duplicate declarations, private path traversal, signature mismatches, opaque
alias rejection, state ownership violations, entry conflicts, and initializer
cycles. Unknown import targets and members include up to three visible candidates ranked by Unicode edit distance, then qualified source name for deterministic ties. Private declarations and declarations behind private intermediate modules are excluded. `misspelled_import_targets_offer_ranked_public_candidates`, `misspelled_import_members_only_suggest_exports`, and `import_suggestions_never_reveal_private_intermediate_modules` exercise both flavors.

## Tested Example

[`examples/projects/modules/`](../../examples/projects/modules/) is the
end-to-end project fixture. `crates/osprey-cli/tests/project_e2e.rs` checks
directory/manifest inputs, AST flattening, LLVM output, source-name restoration,
and byte-exact execution. `crates/osprey-project/tests/` covers graph,
visibility, signature, state, entry, cycle, and opaque-boundary behavior.

`mixed_flavor_project_graphs_emit_identical_ir` in `crates/osprey-cli/tests/cross_flavor_ir_equiv.rs` requires byte-identical IR for all eight flavor assignments to a three-file graph. It covers split namespace contributions, imported modules, abstract and manifest signature types, and caller-supplied effect and arithmetic handlers.

The editor's incremental project support shares compiler discovery and parsing rules, overlays all open Default/ML sources, and reuses unchanged syntax and checked programs. Source creation/deletion, buffer closure and manifest changes invalidate the affected inputs. The normative editor contract and named transport tests are [LSP-WORKSPACE] and [LSP-PROJECT-BATCH] in [spec 0020](0020-LanguageServerAndEditors.md#project-wide-analysis-lsp-workspace).
