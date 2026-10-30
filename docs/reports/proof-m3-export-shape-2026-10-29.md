# Proof kernel M3 export shape — recorded 2026-10-29

Verification record for the inputs and revision named below. Current development follows the [Lean oracle and discipline-correction contract](../messaging.md#proof-development-and-the-lean-oracle); this record does not define a competitive objective or establish consistency.

Task: record a real Lean + `lean4export` run that contains genuine `inductive`
blocks (types, constructors, recursors) so the proof kernel's M3 inductive work
has a measured target instead of a guess ([PROOF-EXPORT]).

**Result: recorded and checker-verified. `lean4checker` exits 0. The one line
kind in the file that the kernel reader cannot read is `inductive`
(`examples/proof/kernel/src/export.ospml:327` refuses it as
`unsupported: inductive`); everything else in the file — including the three
`def` and three `thm` lines with their extra members — the reader already
accepts.**

## Files

- `benchmarks/proof/tools/AndSwap.lean` — new module.
- `benchmarks/proof/tools/lakefile.toml` — added one `[[lean_lib]]` (`AndSwap`)
  beside the existing `IdImp`; requires and pins unchanged.
- `examples/proof/kernel/exports/and_swap.ndjson` — the recorded export,
  17652 bytes, 312 lines, sha256
  `c32affd58cdd28574c2c31d14fe290ed237665952e19b00c5fdf39f839797a2d`
  (`shasum -a 256 and_swap.ndjson`).

## Environment

- `lean --version`:
  `Lean (version 4.25.0, arm64-apple-darwin24.6.0, commit cdd38ac5115bdeec5f609e9126cce00f51ae88b3, Release)`
  (toolchain `~/.elan/toolchains/leanprover--lean4---v4.25.0`, selected by
  `benchmarks/proof/tools/lean-toolchain` = `leanprover/lean4:v4.25.0`).
- lean4export: rev `cfefafacc917b2d14c5f9bb092ed337f71317a4a` (`git describe`
  → `v4.25.0`); it writes format version `3.1.0` and names itself
  `{"name":"lean4export","version":"3.1.0"}` (`Export.lean:414-434`); its own
  `lean-toolchain` is `leanprover/lean4:v4.25.0`.
- lean4checker: rev `aa03950243dc18750c1546602ab054609ea8d57d` (`git describe`
  → `v4.25.0`); the binary has no version flag (it treats `--help` as a module
  name), so its version is its pinned rev plus its `lean-toolchain`
  `leanprover/lean4:v4.25.0`.

Both are the `[[require]]` pins already in `benchmarks/proof/tools/lakefile.toml`.

## The module

`AndSwap.lean` opens with `prelude` (no Init import, exactly like `IdImp.lean`),
so nothing in the export is a copy of Lean's own core declaration — every name
below is declared by this file:

```lean
prelude

inductive And (a b : Prop) : Prop where
  | intro (left : a) (right : b) : And a b

inductive Iff (a b : Prop) : Prop where
  | intro (mp : a → b) (mpr : b → a) : Iff a b

universe u

inductive Eq {α : Sort u} : α → α → Prop where
  | refl (a : α) : Eq a a

theorem and_swap (p q : Prop) (h : And p q) : And q p :=
  And.intro (@And.rec p q (fun _ => q) (fun _hp hq => hq) h)
    (@And.rec p q (fun _ => p) (fun hp _hq => hp) h)

theorem eq_symm {α : Sort u} {a b : α} (h : Eq a b) : Eq b a :=
  @Eq.rec α a (fun b _ => Eq b a) (Eq.refl a) b h

theorem iff_refl (p : Prop) : Iff p p :=
  Iff.intro (fun h => h) (fun h => h)
```

Argument names, universe structure and the `Eq` index family match core:
`And`/`Iff` are `structure`s there (`Init/Prelude.lean:546`,
`Init/Core.lean:181`), which is the same kernel declaration as the `inductive`
written here (one constructor, the fields as constructor arguments) — so the
exported blocks have core's shape with core's names. `Eq` is
`inductive Eq : α → α → Prop` with `refl (a : α) : Eq a a`
(`Init/Prelude.lean:274`) over `{α : Sort u}`; here it is written with `{α :
Sort u}` explicit and exports as `numParams 2` (`α` and the fixed `a`),
`numIndices 1`, i.e. exactly core's parameter/index split.

The explicit `@And.rec` / `@Eq.rec` motives were required: `prelude` supplies
no `And.left`/`Eq.symm` helpers, and `And.rec`'s motive is a family motive over
`And p q` while `Eq.rec`'s is over the index and the proof
(`@Eq.rec α a (fun b _ => Eq b a) (Eq.refl a) b h`).

## Commands run

```
cd benchmarks/proof/tools
lake build AndSwap
  → Built AndSwap; Build completed successfully (3 jobs).

lake exe lean4export AndSwap > ../../../examples/proof/kernel/exports/and_swap.ndjson
  → exit 0; 312 lines

LEAN_PATH="$(lake env printenv LEAN_PATH)" \
  .lake/packages/lean4checker/.lake/build/bin/lean4checker AndSwap
  → exit 0
```

Re-running the exporter produced a byte-identical file (`cmp` clean), so the
sha256 above is stable for this toolchain and this source.

## Line kinds present (312 lines)

| kind | count | read by `export.ospml` today |
| --- | --- | --- |
| `meta` | 1 | yes |
| `in` name (`str` 45 / `num` 6) | 51 | yes |
| `il` level (`param` 2) | 2 | yes |
| `ie` expr (`app` 107, `forallE` 73, `lam` 49, `const` 11, `bvar` 6, `sort` 3) | 249 | yes |
| `def` | 3 | yes (extra members dropped, see below) |
| `thm` | 3 | yes (extra member dropped, see below) |
| `inductive` | 3 | **no — `unsupported: inductive`** |

No `letE`, `proj`, `natVal`, `strVal`, `mdata`, `fvar`, `mvar`, `quot`,
`opaque` or `axiom` line appears.

Key order inside every object is byte order, not declaration order, because
`Json` objects are ordered maps (`Lean/Data/Json/Basic.lean:185-192`,
`mkObj` at `:223`). Line 1 is the meta header:

```json
{"meta":{"exporter":{"name":"lean4export","version":"3.1.0"},"format":{"version":"3.1.0"},"lean":{"githash":"cdd38ac5115bdeec5f609e9126cce00f51ae88b3","version":"4.25.0"}}}
```

## The new kind: `inductive`

One line carries a whole mutual group. Verbatim, line 60 (the `Eq` group):

```json
{"inductive":{"ctors":[{"cidx":0,"induct":1,"isUnsafe":false,"levelParams":[2],"name":9,"numFields":0,"numParams":2,"type":12}],"recs":[{"all":[1],"isUnsafe":false,"k":true,"levelParams":[11,2],"name":10,"numIndices":1,"numMinors":1,"numMotives":1,"numParams":2,"rules":[{"ctor":9,"nfields":0,"rhs":41}],"type":37}],"types":[{"all":[1],"ctors":[9],"isRec":false,"isReflexive":false,"isUnsafe":false,"levelParams":[2],"name":1,"numIndices":1,"numNested":0,"numParams":2,"type":6}]}}
```

Shape: `{"inductive": {"types": [InductiveVal…], "ctors": [ConstructorVal…],
"recs": [RecursorVal…]}}` (`Export.lean:388-392`). Related groups are emitted
as one unit so a type never appears without its constructors and recursors: the
exporter walks `ConstantInfo.inductInfo` and pulls its siblings out of the
environment (`Export.lean:304-330`), it does **not** re-emit Lean's original
`Declaration.inductDecl`. A consumer therefore re-derives nothing for free: it
gets the types, the constructor types and the recursor types *and* rules as
data to check.

### `types[]` — one per inductive type former (`Export.lean:348-361`)

`name`, `levelParams`, `type` are the common `ConstantVal` fields
(`Lean/Declaration.lean:87-92`): the declaration's name-table index, its
universe parameters as **name** indices (`dumpUparams`, `Export.lean:119-123`,
which also emits the `param` level lines), and its type as an expr index.
The rest are `InductiveVal` (`Lean/Declaration.lean:253-292`):

- `numParams` (`:261`) — arguments to the type that are fixed across
  constructors (parametric polymorphism). `Eq` has 2 (`α`, `a`), `And`/`Iff`
  have 2 (`a`, `b`).
- `numIndices` (`:266`) — arguments that vary across constructors. `Eq` = 1
  (the second `α` argument), `And`/`Iff` = 0.
- `all` (`:268`) — every type in the mutual group, including this one; here
  always a singleton.
- `ctors` (`:270`) — constructor names, in `cidx` order.
- `numNested` (`:274`) — auxiliary types produced from nested occurrences
  (`Array T`-style); 0 for all three.
- `isRec` (`:276`) — the type appears as a constructor argument. `false` for
  all three: `Eq` is indexed, not recursive, and `And`/`Iff` do not mention
  themselves.
- `isUnsafe` (`:278`) — flagged unsafe; `false` everywhere here.
- `isReflexive` (`:292`) — some constructor takes a function returning the type
  being defined (`WideTree.branch`-style); `false` for all three.

### `ctors[]` — one per constructor (`Export.lean:362-372`)

`name`, `levelParams`, `type` as above, plus `ConstructorVal`
(`Lean/Declaration.lean:320-329`):

- `induct` (`:322`) — **name** index of the inductive type this constructor
  belongs to (`1` = `Eq`), not an expr index.
- `cidx` (`:324`) — position of the constructor in that inductive's
  declaration.
- `numParams` (`:326`) — the inductive's parameter count, repeated here.
- `numFields` (`:328`) — arity minus parameters, i.e. how many arguments the
  constructor's minor premise receives. `Eq.refl` = 0 (its `a` is the
  parameter), `Iff.intro`/`And.intro` = 2.
- `isUnsafe` (`:329`).

### `recs[]` — one per recursor (`Export.lean:373-386`)

`name`, `levelParams`, `type`, plus `RecursorVal`
(`Lean/Declaration.lean:349-372`):

- `all` (`:351`) — inductive types in the group that generated this recursor;
  the member to dispatch a recursor back to its block.
- `numParams` (`:353`), `numIndices` (`:355`) — parameter and index counts of
  the major premise's family.
- `numMotives` (`:357`) — one per type in `all`; 1 here.
- `numMinors` (`:359`) — one per constructor of every type in `all`; 1 here,
  because each group has exactly one constructor.
- `rules` (`:361`) — one `RecursorRule` per constructor: `ctor` is the
  constructor's name index (`Lean/Declaration.lean:342`), `nfields` its field
  count without parameters (`:344`), `rhs` the reduction right-hand side as an
  expr index (`:346`); exported by `dumpRecRule` (`Export.lean:403-408`).
  Line 60's single rule is `{"ctor":9,"nfields":0,"rhs":41}`,
  i.e. `Eq.refl` with zero fields reducing to expr 41.
- `k` (`:371`) — supports K-like reduction (single constructor, no fields, lives
  in `Prop`). `true` for `Eq.rec` only; `And.rec` and `Iff.rec` are `false`
  because their constructor has two fields.
- `isUnsafe` (`:372`).

The other two blocks, for completeness: line 173 is
`Iff`/`Iff.intro`/`Iff.rec` (`numParams 2`, `numIndices 0`, ctor `numFields 2`,
rule `{"ctor":27,"nfields":2,"rhs":134}`, `k false`) and line 235 is
`And`/`And.intro`/`And.rec` (same counts, rule
`{"ctor":34,"nfields":2,"rhs":188}`, `k false`). `And`'s and `Iff`'s `type`
indices are both `96`: `(a b : Prop) → Prop` is structurally one expression and
the exporter deduplicates by `Expr` equality (`getIdx`, `Export.lean:77-93`).

## Members the reader drops on kinds it already reads

`def` is the only declaration kind here to carry the full
`DefinitionVal` payload (`Export.lean:250-263`,
`Lean/Declaration.lean:127-131`). Verbatim, line 91:

```json
{"def":{"all":[15],"hints":"abbrev","levelParams":[11,2],"name":15,"safety":"safe","type":56,"value":70}}
```

`export.ospml:331` (`defined`) reads `name`, `levelParams`, `type`, `value` and
ignores:

- `hints` — reducibility hint, `"abbrev"` here; encoded as `"opaque"`,
  `"abbrev"` or `{"regular": n}` (`Export.lean:13-17`).
- `safety` — `"safe"`/`"unsafe"`/`"partial"` (`Export.lean:24-28`).
- `all` — names in the mutual definition group (`Lean/Declaration.lean:127-131`,
  exported by `dumpNames`, `Export.lean:124-126,261`).

`thm` likewise gains `all` (`Export.lean:265-276`,
`Lean/Declaration.lean:137-141`). Verbatim, line 126:

```json
{"thm":{"all":[16],"levelParams":[2],"name":16,"type":76,"value":94}}
```

`export.ospml:329` (`theoremed`) ignores `all`. `axiom` would gain
`isUnsafe` (`Export.lean:245-248`, `Lean/Declaration.lean:93-95`); no axiom line
appears in this file.

The three `def` lines are Lean's compiler-generated auxiliary recursors
`Eq.recOn`, `Iff.recOn`, `And.recOn` — emitted by the inductive command
(`Lean/Elab/MutualInductive.lean:1124`) as `.abbrev` definitions that reorder
the recursor's arguments (`Lean/Meta/Constructions/RecOn.lean:21-37`). They are
ordinary `def`s in the export and need no new reader kind; the kernel already
accepts them.

## Expressions and levels in this file

Levels: `param` only, twice — `{"il":1,"param":2}` (`u`) and
`{"il":2,"param":11}` (`u_1`). No `succ`, `max` or `imax` line is needed: the
three types live in `Prop` and the theorems are universe-polymorphic only
through `u`/`u_1`.

Expressions: `sort` 3, `bvar` 6, `const` 11, `app` 107, `lam` 49, `forallE`
73 — all already read (`export.ospml:262-292`). `const` names its head and
carries level arguments, e.g. line 19 `{"const":{"name":1,"us":[1]},"ie":7}`
(`Eq` at level `u`); line 77's `{"const":{"name":10,"us":[2,1]},"ie":57}` is
`Eq.rec` at levels `[u_1, u]`, matching its `levelParams`.

Not present, with their encoding for when M3 needs them (all in
`Export.lean:155-213`):

- `letE` — `{"letE":{"name","type","value","body","nondep"}}`; `nondep` is a
  hint and is normalised to `false` by `removeMData` (`Export.lean:128-153`) so
  the same term never gets two expr indices.
- `proj` — `{"proj":{"typeName","idx","struct"}}`, `typeName` a **name** index,
  `idx` a field number, `struct` an expr index.
- literals — `{"natVal":"<decimal string>"}` and `{"strVal":"<text>"}`; a
  natural literal also drags in `Nat` and `Char.ofNat`, a string literal
  `String.ofByteArray` (`Export.lean:215-225`).
- `mdata` — `{"mdata":{"data":<object>,"expr":<index>}}`; absent even when
  present in the term because metadata is stripped unless the exporter is run
  with `--export-mdata` (`Export.lean:27,227-232`).
- `fvar`/`mvar` — never exported: `dumpExprAux` panics on them
  (`Export.lean:158`).

## Declarations in file order

Order is `Environment.constants` iteration order (`Main.lean:11-14`), not source
order and not alphabetical.

| line | name | kind | key fields |
| --- | --- | --- | --- |
| 60 | `Eq` | inductive type | params 2, indices 1, `all [Eq]`, ctors `[Eq.refl]`, numNested 0, isRec false, isReflexive false, levelParams `[u]` |
| 60 | `Eq.refl` | constructor | induct `Eq`, cidx 0, numParams 2, numFields 0 |
| 60 | `Eq.rec` | recursor | params 2, indices 1, motives 1, minors 1, rules `[{Eq.refl, 0, rhs 41}]`, k **true**, levelParams `[u_1, u]` |
| 91 | `Eq.recOn` | def (`abbrev`, safe) | all `[Eq.recOn]`, levelParams `[u_1, u]` |
| 126 | `eq_symm` | thm | levelParams `[u]`, all `[eq_symm]` |
| 173 | `Iff` | inductive type | params 2, indices 0, ctors `[Iff.intro]`, isRec false, isReflexive false, levelParams `[]` |
| 173 | `Iff.intro` | constructor | induct `Iff`, cidx 0, numParams 2, numFields 2 |
| 173 | `Iff.rec` | recursor | params 2, indices 0, motives 1, minors 1, rules `[{Iff.intro, 2, rhs 134}]`, k false, levelParams `[u]` |
| 200 | `Iff.recOn` | def (`abbrev`, safe) | all `[Iff.recOn]`, levelParams `[u_1, u]` |
| 235 | `And` | inductive type | params 2, indices 0, ctors `[And.intro]`, isRec false, isReflexive false, levelParams `[]` |
| 235 | `And.intro` | constructor | induct `And`, cidx 0, numParams 2, numFields 2 |
| 235 | `And.rec` | recursor | params 2, indices 0, motives 1, minors 1, rules `[{And.intro, 2, rhs 188}]`, k false, levelParams `[u]` |
| 247 | `iff_refl` | thm | levelParams `[]`, all `[iff_refl]` |
| 287 | `and_swap` | thm | levelParams `[]`, all `[and_swap]` |
| 312 | `And.recOn` | def (`abbrev`, safe) | all `[And.recOn]`, levelParams `[u]` |

15 declarations in 13 lines: 3 inductive types, 3 constructors, 3 recursors,
3 generated `recOn` definitions, 3 theorems. Name table: 51 names
(`u`, `u_1`, `motive`, `t`, `refl`, `mp`, `mpr`, `left`, `right`, `intro`,
`_hp`/`hq`/`hp`/`_hq`, plus Lean's hygienic `x._@.AndSwap.<n>._hygCtx._hyg.<m>`
binder names). 2 levels, 249 expressions.

## What this means for M3

- The reader's own tables already hold everything an inductive block references:
  names, levels and expressions are numbered in the same three spaces, and a
  block's `type`, `ctor.type`, `rec.type` and `rules[].rhs` are ordinary expr
  indices defined earlier in the file (the exporter dumps dependencies before
  the block, `Export.lean:304-330`).
- The original shape capture failed with `unsupported: inductive`. The shared reader and kernel now read and validate these blocks; the execution evidence below supersedes that implementation status.
- `*On` definitions and `thm`/`def` `all` lists arrive as extra members on kinds
  the reader already parses and are already ignored, so no compatibility break
  is needed for them.
- `Eq` here is the module's own `Eq` (prelude, no Init), and `Eq.rec` carries
  `k = true`: an inductive reader cannot assume `k = false` for every `Prop`
  inductive, and cannot assume `isRec = true` for an indexed family.
- `scripts/proof-export.sh` now selects `id_imp`, `and_swap` or `Nat.add_comm`. All three real exports have been rebuilt and compared byte for byte locally. The CI proof job now verifies each recorded export and runs the sixteen recorder regressions; its original conditions and timeout remain intact.

## Common arithmetic proof execution

The pinned Lean 4.25.0 core theorem `Nat.add_comm : ∀ n m : Nat, n + m = m + n` now checks from an empty Osprey environment. `exports/nat_add_comm.ndjson` is 58,926 bytes and 1,108 lines, SHA-256 `448084c203b9857a1dceb6986042987be4185ead138d1504039dec0e6c9c3a9f`. Its complete closure contains seven inductive blocks, twenty-one definitions, five theorems and zero axioms. The theorems are `Eq.symm`, `congrArg`, `Nat.zero_add`, `Nat.succ_add` and `Nat.add_comm`. The original Lean license is retained beside the export as `LICENSE.lean4`.

The Osprey checker parses and checks the closure itself; Lean is an independent fixture oracle. `lake exe lean4checker --fresh Init.Data.Nat.Basic` passed. Reproduction from the repository root:

```sh
bash scripts/proof-export.sh --check --theorem Nat.add_comm
target/ci/osprey examples/proof/kernel --run --quiet --no-warnings --memory=gc
make BIN=target/ci/osprey _test_proof_kernel
node --test scripts/test-proof-export.mjs
```

The example's final lines are `checked: Nat.add_comm (export)` and a type-mismatch rejection for the deliberately corrupted proof. Its original thirteen output lines are preserved. The canonical tests compare the arithmetic statement structurally, assert zero axioms, corrupt the proof, and replace the statement by the false universal equality `n + m = m`. They use the same `Checked::exportedDeclaring` entry as the example.

The real acceptance regression first failed at the unsupported projection on line 179, then the natural literal on line 627, then `HAdd.rec`'s binder annotation. The kernel now checks structure projections, including dependent fields and the prohibition on extracting data from `Prop`; natural literals use exact arbitrary-size limbs and must refer to the checked canonical `Nat` family. Recursor derivation removes recognized identity type annotations as Lean does. It verifies the annotation's definition rather than trusting its name. Strict structural recursor-type and rule agreement remains in place; a beta-redex containing an unbound term cannot conceal a forged statement.

The expanded suite passes all 164 cases under GC, ARC and wasm32 on unchanged source fingerprints. The canonical `make BIN=target/ci/osprey _test_proof_kernel` gate passes, including all fifteen example output lines on each target. The existing identity/export speed gates pass; these measurements do not establish performance for `Nat.add_comm` or Mathlib. Sixteen recorder regressions pass, including selected-theorem arguments, pinned toolchain checks and exact trailing-newline preservation. All three selected real exports were byte-identical on rerun.

This establishes a common Lean core proof milestone. Mathlib imports, the full discipline frontend, the exact-export speed gate and workbench screenshot acceptance remain separate delivery requirements. Navier–Stokes is a future high-water mark, not a result established here.
