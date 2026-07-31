# Plan 0004 — Collection / Map Standard-Library Surface

**Subsystem:** `crates/osprey-types` (builtin registry, `TypeEnv`, inference
order), `crates/osprey-codegen` (dispatch), `crates/osprey-types/src/builtin_docs*.rs`
(doc parity), `compiler/runtime` (a few new C ops), `tests/regressions`
**Status:** **The plan's premise was resolved from the other side.** Everything it
set out to *fix* is fixed and tested: the receiver-directed `length`/`isEmpty`
miscompile, across all **three** list-shaped layouts, plus the `+` operator over
the literal layout. Everything it set out to *add* — the bare-name surface
(`get`, `contains`, `reverse`, `indexOf`, `keys`, `values`, `head`, `tail`,
`entries`, `zipToMap`, `groupBy`) — was **deleted from the spec** instead of
added to the compiler. Spec 0012 §Collection Functions now reads "Except for
`length` and `isEmpty`, public names are prefixed with `list` or `map`", every
bare-name anchor this plan chased is gone from it, and `mapKeys`/`mapValues` are
the normative spellings rather than names awaiting a rename. Those items are
therefore **unspecified surface, not outstanding work** — see
[§Unspecified: no work planned](#unspecified-no-work-planned). One live defect
remains and is recorded in the TODO: `listGet` over a `List<string>`.
**Scope:** Low (remainder is one defect; the type-system work below is not
scheduled because nothing normative asks for it)
**Spec:** [0012-Built-InFunctions.md](../specs/0012-Built-InFunctions.md)

## Summary

The list/map runtime is implemented and correct, and it is exposed under
`listXxx`/`mapXxx` names. When this plan was written [the
spec](../specs/0012-Built-InFunctions.md) specified **bare** names (`append`,
`prepend`, `concat`, `get`, `reverse`, `contains`, `keys`, `values`, …), so the
documented API did not resolve. The spec has since been rewritten to the
prefixed surface, closing that gap by correcting the document rather than the
compiler. What was left is what was genuinely broken: the two bare names that
*were* exposed (`length`, `isEmpty`) were exposed **unsoundly**, and the `+`
operator shared the same layout hole. Both are fixed below.

## What works today

- Registered (prefixed) builtins: `listAppend`, `listPrepend`, `listConcat`,
  `listReverse`, `listGet`, `listContains`, `listLength`, `forEachList`, and the
  `mapSet`/`mapGet`/`mapRemove`/`mapMerge`/`mapContains`/`mapLength`/`mapKeys`/
  `mapValues` family —
  [crates/osprey-types/src/builtins.rs](../../crates/osprey-types/src/builtins.rs)
  (`lists`/`maps`).
- Codegen dispatch for these —
  [crates/osprey-codegen/src/collections.rs](../../crates/osprey-codegen/src/collections.rs) `gen`.
- Bare `length` / `isEmpty` across `string`, `List<T>` and `Map<K, V>` —
  receiver-directed, see below.
- A string `indexOf`, `contains`, `reverse` already exist (the *list* versions are
  separate and missing).

## DONE — the `length`/`isEmpty` miscompile (fixed)

A **live miscompile**, not a missing feature. `length` and `isEmpty` were
registered as `(any) -> int` / `(any) -> bool`
([builtins.rs](../../crates/osprey-types/src/builtins.rs)
`mono(e, "length", vec![any()], i())`), so they type-checked on *any* receiver —
including a `List` or a `Map`. Codegen, however, dispatched **by name**, and the
name-keyed chain in [expr.rs](../../crates/osprey-codegen/src/expr.rs) ran
`strings::gen` **ahead of** `collections::gen`. Result: `length(someList)` lowered
to the C string routine `osp_strlen` applied to the raw collection handle — an
`i8*` heap pointer read as a NUL-terminated string. Wrong answer *and* an
out-of-bounds read. Nothing in the type checker could catch it, because the
declared parameter type was `any`.

**Fix (shipped):** a receiver-directed **pre-dispatch**,
[`collections::gen_receiver_directed`](../../crates/osprey-codegen/src/collections.rs),
inserted *before* the name-keyed string/collection dispatchers. It lowers the
receiver **exactly once** (re-lowering per candidate would duplicate side
effects), unwraps it, and branches on the runtime owner tag `Value.osp_ty`
(`LIST_OWNER` / `MAP_OWNER`) versus `LType::Str`, falling through to
`strings::gen_size` for the string case. `isEmpty` is then `count == 0` on
whichever counter was chosen. This mirrors the shape `gen_arith`
([expr.rs](../../crates/osprey-codegen/src/expr.rs)) already uses to give `+` one
spelling over `int`, `float` and `string`.

Implements **[BUILTIN-COLLECTION-LENGTH]** and **[BUILTIN-COLLECTION-ISEMPTY]**
(cited in `collections.rs` and in the codegen regression test
`bare_length_and_is_empty_dispatch_on_the_receiver_type`, which asserts
`osprey_list_length`, `osprey_map_length` **and** exactly one `osp_strlen` call
site in one module).

Both anchors are now defined in
[0012 §Common](../specs/0012-Built-InFunctions.md) alongside the rest of the
collection surface, so the citation resolves.

**Second half of the same miscompile (fixed 2026-07-30).** The owner-tag branch
covered two of the *three* list-shaped layouts. A flat **list literal** —
`[1, 2, 3]`, and the `{ i64 length, char **items }` block minted by
`osp_string_lines` / `split` / `words` — is neither an `OspreyList` handle nor a
string: its `osp_ty` is a `[]<elem>` literal tag
([listlit.rs](../../crates/osprey-codegen/src/listlit.rs)). It matched neither
collection arm and fell through to `strings::gen_size`, so `osp_strlen` counted
the bytes of the struct's own leading length word:

```osprey
let xs = [1, 2, 3]
print("${length(xs)} ${listLength(xs)}")   // printed "1 3"
print("${length(lines("a\nb\nc"))}")       // printed "1"
```

`length` answered **1** for every literal of 1–255 elements (2 above 255) and
**0** for the empty literal, which is why the empty-list assertion in the corpus
passed and hid it. `gen_receiver_directed` now consults
`listlit::lit_length`, which reads the literal's leading `i64` directly — the two
layouts share that field, which is why `listLength` always read both correctly.
No runtime list is materialized just to count one. Pinned by the literal receiver
added to `bare_length_and_is_empty_dispatch_on_the_receiver_type` (the
"exactly one `osp_strlen` call site" assertion now also fences the literal) and
by `length(commands)` / `length(lines(…))` / `!isEmpty(commands)` in
`tests/core/collections/list_basics.test.{osp,ospml}`.

## Gaps (spec → impl)

Spec uses bare names ([0012 §Lists/§Maps](../specs/0012-Built-InFunctions.md)):

| Spec name | Status |
|-----------|--------|
| `length`, `isEmpty` (string/list/map) | **done** — receiver-directed |
| `append`, `prepend`, `concat` (list) | implemented under `listXxx` — name not exposed; **no string collision**, so these are the cheap ones |
| `get`, `reverse`, `contains` (list) | implemented under `listXxx` — name **collides with a registered string builtin** (see §Blocker) |
| `head(list) -> Result<T, IndexError>` | **missing** |
| `tail(list) -> List<T>` (total) | **missing** |
| `indexOf(list, value) -> Result<int, IndexError>` | **missing**; the bare name is taken by the string version |
| `set`, `remove`, `merge` (map) | implemented under `mapXxx` — name not exposed, no collision |
| `get`, `contains` (map) | implemented under `mapXxx` — three-way collision with string *and* list |
| `keys`, `values` (map) | **name collision with a different meaning** — see §Semantic collision |
| `entries` (map) | **missing** |
| `filterEntries`, `foldEntries`, `zipToMap`, `groupBy` | **missing** |

The table above describes the spec **as it was when this plan was written**. Every
row marked "name not exposed", "collides", or "missing" now describes a name spec
0012 no longer specifies; see
[§Unspecified: no work planned](#unspecified-no-work-planned).

## Unspecified: no work planned

Spec 0012 §Collection Functions is now: `length` and `isEmpty` bare and
receiver-directed, everything else prefixed `list`/`map`. Concretely, the spec no
longer defines `[BUILTIN-LIST-HEAD]`, `[BUILTIN-LIST-TAIL]`,
`[BUILTIN-LIST-INDEXOF]`, `[BUILTIN-MAP-ENTRIES]`, `[BUILTIN-MAP-ZIPTOMAP]` or
`[BUILTIN-MAP-GROUPBY]`, and it names `mapKeys` / `mapValues` normatively rather
than as spellings awaiting a rename to `keys` / `values`.

So the following are **unspecified extensions, not outstanding work** — the same
treatment [plan 0015](0015-generics-and-variance.md) §4 gives bounded
polymorphism. Reopening any of them means re-specifying it in 0012 first, and
paying §Principality:

- bare `get` / `contains` / `reverse` / `indexOf` over List and Map, and the
  overload candidate registry + inference reordering + deferred resolution they
  require;
- renaming `mapKeys` / `mapValues` to `keys` / `values`;
- `head`, `tail`, `entries`, `filterEntries`, `foldEntries`, `zipToMap`,
  `groupBy`;
- the collision-free bare aliases (`append`, `prepend`, `concat`, `set`,
  `remove`, `merge`) — cheap to register, but registering a name the spec does not
  define creates surface no document backs.

## Blocker — why bare names are not a dispatch tweak

The previous revision of this plan said “Osprey has no ad-hoc overloading today,
so dispatch must be receiver-type-directed at codegen time”, and left the choice
as an under-specified “decision needed”. Codegen dispatch alone is insufficient;
the type checker requires three changes.

1. **`TypeEnv` holds exactly one scheme per name and `insert` silently
   overwrites.** [env.rs](../../crates/osprey-types/src/env.rs):
   `vars: HashMap<String, Scheme>`, and `insert` ends in
   `let _ = self.vars.insert(name, scheme);` — the discarded return value *is*
   the previous scheme. Registering a `List` `contains` therefore **destroys** the
   string `contains`, turning every existing string call site into a type error
   while codegen keeps emitting the string runtime call.
2. **The callee scheme is instantiated before any argument is inferred.**
   [expr.rs](../../crates/osprey-types/src/expr.rs) `lookup_ident` does
   `env.get(name)` → `instantiate(...)`, and `infer_call` unifies that
   already-chosen type against the arguments afterwards. There is no point in the
   current order at which a candidate could be *selected* by receiver type.
3. **These names are not typed `any`, so the `length` trick does not transfer.**
   `contains` is `(string, string) -> bool`, `indexOf` is
   `(string, string) -> Result<int, …>`, `reverse` is `(string) -> string`,
   `listGet` is `(List<t>, int) -> Result<t, …>`, `mapGet` is
   `(Map<k,v>, k) -> Result<v, …>`. Widening them to `any` would remove type
   checking on the string surface — the same hole that produced the `length`
   miscompile above.

Approach **B** (keep prefixed names and change the spec) was previously rejected
here "because the spec defines the contract". **Approach B is what shipped.**
Spec 0012 §Collection Functions was rewritten to the prefixed surface, so the
mechanism below is no longer required by anything normative. It is retained as
the design of record should bare-name overloading ever be re-specified, and
because §Principality documents a real cost that any future proposal must pay.

## Implementation — the minimal sound mechanism

Four pieces are required in this order; omitting step 3 reintroduces silent
wrong-runtime dispatch.

1. **Separate overload candidate registry.** Keep `TypeEnv` one-scheme-per-name
   (assignment, shadowing, `mut`, and `bound_names()` redefinition detection all
   depend on it). Add a sibling map — e.g. `overloads: HashMap<String, Vec<(HeadCon,
   Scheme)>>` — keyed by the **head constructor of parameter 0** (`Str`, `List`,
   `Map`). The `TypeEnv` entry for an overloaded name stays as the *default*
   (string) scheme so unrelated code paths keep working.
2. **Reorder inference: infer argument 0 first.** In `infer_call` /
   `infer_method_call`, infer and **prune** (apply the current substitution to)
   argument 0 *before* the callee scheme is selected and instantiated. Look the
   pruned head constructor up in the candidate registry; instantiate only the
   winner; then unify the remaining arguments as today.
3. **Deferred-resolution store for still-unresolved receivers.** When the pruned
   receiver is still a type variable (`fn f(xs) = contains(xs, 1)`), no candidate
   is knowable yet. Record `(var, name, call-site node)` in a deferred store and
   resolve it in the **existing post-inference substitution phase**. If the
   variable is resolved by then, pick that candidate; if it is still free, emit an
   **explicit ambiguity error** naming the call site and the candidates. Never
   silently default to one collection kind — a silent default is how the receiver
   ends up in the wrong C routine.
4. **Codegen dispatches on the runtime owner tag.** Extend the shipped
   `gen_receiver_directed` pattern: lower the receiver once, branch on
   `Value.osp_ty` (`LIST_OWNER` / `MAP_OWNER`) vs `LType::Str`. **Do not** build a
   position-keyed “resolved overload” table handed down from the type checker:
   `Expr::Call` (and `Expr::MethodCall`) carry **no `position` field**
   ([crates/osprey-ast/src/lib.rs](../../crates/osprey-ast/src/lib.rs) — many
   *other* `Expr` variants do, `Call` does not), and positions that *are*
   available collide across string-interpolation fragments, which desugar several
   sub-expressions onto one source span.

### Principality consequence

**HM principal types do not survive this as specified.** `fn f(xs) = contains(xs, 1)`
has no principal type under the scheme above: `Scheme`
([ty.rs](../../crates/osprey-types/src/ty.rs)) is `{ vars: Vec<VarId>, ty: Type }`
with **no predicate field**, so “`xs` is some type that has a `contains`” is
inexpressible and step (3) must report an ambiguity **error**. Callers get a clear
message instead of a wrong program, but a previously-inferable generic function
now fails to compile.

Recovering principality means **qualified types**: add a `preds` field to `Scheme`
and propagate it through `instantiate` / `generalize` / unification. Because
generic user functions in Osprey are already specialised by call-site inlining
(plan 0002), this could be discharged by **monomorphization with no dictionary
passing** — every call site knows its concrete receiver. That is a strictly larger
piece of work than this plan and should be its own plan; until it exists, the
ambiguity error is the correct behaviour and must be documented in spec 0012.

### Semantic collision — `keys`/`values` vs `mapKeys`/`mapValues`

The spec defines **both**, with different meanings:

- [0012 §Maps](../specs/0012-Built-InFunctions.md): `keys(map) -> List<K>`,
  `values(map) -> List<V>` — **arity-1 accessors**.
- Same file: `mapValues(map, fn(V) -> W) -> Map<K, W>`,
  `mapKeys(map, fn(K) -> K2) -> Map<K2, V>` — **arity-2 transformers**.

The implementation registers `mapKeys` / `mapValues` with the **accessor**
semantics: `poly(e, "mapKeys", vec![0, 1], vec![m()], Type::list(k()))`
([builtins.rs](../../crates/osprey-types/src/builtins.rs) `maps`), lowered by
`map_to_list` in [collections.rs](../../crates/osprey-codegen/src/collections.rs).
`keys` and `values` are **not registered at all**. So the spec's transformer
surface is unimplemented *and* its two names are occupied by something else.

Exposing the spec surface requires a rename + re-registration:
`mapKeys` → `keys`, `mapValues` → `values`, then registering the real arity-2
transformers under `mapKeys`/`mapValues`. Call sites to migrate (grep
`mapKeys\|mapValues` under `examples/`):

| File | Lines |
|------|-------|
| `tests/core/collections/map_basics.test.osp` | map view assertion batches |
| `tests/core/collections/map_basics.test.ospml` | map view assertion batches |
| `tests/regressions/basics/types/recursive_unions.test.osp` | 75, 79, 83 |
| `tests/regressions/basics/types/recursive_unions.test.ospml` | 80, 84, 89 |
| `tests/regressions/basics/json/json_document_query.test.osp` | 26 |
| `tests/regressions/basics/json/json_document_query.test.ospml` | 34 |
| `tests/regressions/effects/fiber_effects.test.osp` | 6 (comment), 89 |
| `tests/regressions/effects/fiber_effects.test.ospml` | 70 |

Plus the non-owned prose/generated surfaces that list the name:
`website/src/status.md` and the generated `website/src/docs/**` function index.
The `.osp`/`.ospml` twins must stay byte-equivalent per [FLAVOR-IR-EQUIV], so each
pair migrates together against one shared `.expectedoutput`.

### Compatibility constraints on new bare names

- **Builtin names are non-redefinable and therefore source-breaking.**
  [check.rs](../../crates/osprey-types/src/check.rs) rejects a user `fn` whose
  name is a builtin with ``cannot redefine built-in function `{name}` ``; the only
  exceptions are `SHADOWABLE_BUILTINS = ["test", "expect", "check"]`
  ([builtins.rs](../../crates/osprey-types/src/builtins.rs)). Every bare name this
  plan registers (`append`, `get`, `keys`, `head`, `tail`, …) is a **permanent
  reservation** that breaks any program already defining a function by that name.
  Land the reservations in one batch, and note them in the spec's compatibility
  section.
- **Every new builtin needs a doc entry or the build fails.**
  [builtin_docs.rs](../../crates/osprey-types/src/builtin_docs.rs) carries the test
  `every_builtin_is_documented_with_matching_arity`, which asserts the documented
  name set equals the registered scheme set **and** that arities match. Adding a
  scheme without an entry in `builtin_docs_lang.rs` / `builtin_docs_sys.rs` is a
  failing `make test`, not a doc-debt TODO.

## Remaining implementation work

1. **Collision-free bare names.** `append`, `prepend`, `concat`
   (list) and `set`, `remove`, `merge` (map) collide with nothing. Register them
   as additional schemes pointing at the existing lowering and add doc entries;
   no overload machinery is required.
2. **Overload machinery** (steps 1–4 above) for `get`, `contains`, `reverse`,
   `indexOf`.
3. **`head` / `tail`.** `head` returns `Result<T, IndexError>`; `tail` is total
   (`tail([]) == []`). Derive from the existing `listGet`/slice ops where possible
   rather than adding C.
4. **List `indexOf`** — C equality scan + signature + dispatch, gated on (2).
5. **`entries`, `filterEntries`, `foldEntries`, `zipToMap`, `groupBy`.** These take
   callbacks; computed callbacks already work (plan 0001, retired).
6. **`find-similar` before adding** each C helper, to avoid duplicating an existing
   scan/fold primitive.

## Testing

- Extend [tests/regressions/basics/lists/](../../tests/regressions/basics/lists/)
  (`map_basics`, and the list examples) plus their `.ospml` twins to use the bare
  names and the new ops; refresh the shared `.expectedoutput`.
- Cover `head([])`/`tail([])` edge cases and `zipToMap` length-mismatch error.
- Add a **must-reject** case under `examples/failscompilation/` for the
  ambiguity error from step (3). An unresolved receiver must return the explicit
  error; a future qualified-types plan would change that behavior, so pin it in a
  test.
- Add a codegen regression per newly-overloaded name in the shape of
  `bare_length_and_is_empty_dispatch_on_the_receiver_type`: one module exercising
  string + List + Map receivers, asserting each hits its own runtime and counting
  the string call sites.

## Risks / considerations

- Receiver-directed dispatch must produce a clear diagnostic when the receiver
  type is unknown rather than defaulting to one collection kind; the `length`
  miscompile demonstrates the failure mode.
- `head`/`tail` are *functions*, distinct from the `[head, ...tail]` list
  *pattern* already shipped — do not conflate.
- Every bare name is a source-breaking reservation (see above).
- The `.osp`/`.ospml` twin equivalence bar means every example migration is two
  files against one golden output.

## TODO

- [x] **Fix the `length`/`isEmpty` miscompile** — `collections::gen_receiver_directed`
      pre-dispatch, lowering the receiver once and branching on `Value.osp_ty`
      vs `LType::Str`; ordered ahead of the name-keyed `strings::gen`.
      [BUILTIN-COLLECTION-LENGTH], [BUILTIN-COLLECTION-ISEMPTY]. Regression test
      `bare_length_and_is_empty_dispatch_on_the_receiver_type`.
- [x] **Third layout: the flat list literal** — `[1, 2, 3]` and the string
      runtime's `lines`/`split`/`words` block carry a `[]<elem>` literal tag, not
      `LIST_OWNER`, so they still reached `osp_strlen` and `length` answered `1`
      for any 1–255-element literal. `gen_receiver_directed` now reads the
      literal's leading `i64` via `listlit::lit_length`. Pinned by the literal
      receiver in the codegen test above plus `length(commands)`,
      `length(lines(…))` and `!isEmpty(commands)` in
      `tests/core/collections/list_basics.test.{osp,ospml}`.
- [x] **The `+` operator over the literal layout** — the same three-layout hole,
      one operator over. `gen_arith` (`osprey-codegen/src/expr.rs`) selected
      `listConcat` off the `LIST_OWNER` tag alone, so `xs + [1]` handed
      `osprey_list_concat` the literal's foreign header and **segfaulted**
      (exit 139), while `[1] + [2]` matched neither operand and fell into integer
      arithmetic with `expected an integer, found a string/handle`. Both operands
      are now normalised through `listlit::to_runtime_list` (a no-op for a real
      handle). Pinned by
      `list_literal_operands_of_plus_are_rebuilt_before_concatenation` (codegen
      `lib.rs`) and the four literal/handle position combinations in
      `tests/core/collections/list_basics.test.{osp,ospml}`, green under
      default / `--memory=gc` / `--memory=arc` (`ARC_LEAKY=0`) and wasm32.
- [ ] **`toFloat(n: int) -> float`** — the missing scalar conversion builtin.
      Round-to-nearest-even, exact for `|n| <= 2^53`; total (no `Result`).
      Required by [GPU-CONVERT] (docs/specs/0034-GPUComputation.md) so the
      canonical float-pipeline seed `gpuIota(n) |> gpuMap(toFloat)` is
      expressible — today the GPU float stress tests
      (`tests/core/gpu/stress.test.osp` `floatChurnCase`) iterate literal
      buffers because no int→float conversion exists anywhere in the surface.
      Register the scheme in `builtins.rs`, lower via `sitofp` (a `conv.rs`
      one-liner), add the docs entry per the checklist below, and extend the
      GPU stress corpus to seed from `gpuIota` once available.
- [ ] **Defect found, not fixed: `listGet` over a `List<string>`.** Independent
      of the layout work above — a plain runtime handle fails too. Re-measured
      2026-07-30; it has **two faces**, and the quiet one is the reason this
      matters more than a rejected program:

      ```osprey
      let ints = listAppend(List(), 7)
      let ss = listAppend(List(), "x")
      print("${listGet(ints, 0) ?: -1}")        // 7 — correct
      print("${listGet(ss, 0) ?: "missing"}")   // 0 — WRONG, and silent
      let v = listGet(ss, 0) ?: "missing"       // codegen: invalid program:
                                                // match arms disagree on type: `i64` and `i8*`
      ```

      Inside an interpolation the program compiles and prints `0` for a value
      that is `"x"`; bound to a `let` the same expression is rejected, because
      the desugared `?:` match is where the `i64` payload meets the `i8*`
      fallback. The rejection message recorded here previously
      (`expected an integer, found a string/handle`) no longer reproduces — the
      arm-type mismatch is the current one.

      `list_get` (`collections.rs`) wraps `osprey_list_get`'s raw `i64` element
      word with `result_from_flag` and never `inttoptr`s it back, so the `Result`
      payload is typed `i64` while the `?:` default is `i8*`. Only the index
      spelling `ss[0]` works, because `gen_index` reads the element `LType` off
      the literal tag. The runtime handle carries no element type, so the fix
      needs the checker's inferred `List<T>` argument type at the call site —
      `listContains` sidesteps this by reading its *needle*'s type
      (`is_str = needle.ty == LType::Str`). Same gap applies to any `listGet`
      whose element is a managed pointer (nested list, record).
- [x] Define the collection spec anchors in 0012 — `[BUILTIN-LIST]`,
      `[BUILTIN-MAP]`, `[BUILTIN-COLLECTION-LENGTH]`,
      `[BUILTIN-COLLECTION-ISEMPTY]` and the
      per-function ids (`[BUILTIN-LIST-APPEND]`, `[BUILTIN-MAP-SET]`, …) — so
      the code and example citations resolve, and the wildcard references in
      the list/map examples are expanded to concrete ids.
- [x] `make ci` green.

### Withdrawn — the bare-name surface

The items below were this plan's bulk. They are **withdrawn, not deferred**: spec
0012 §Collection Functions no longer specifies any of the names they deliver, so
there is nothing to conform to. They are listed rather than deleted because the
mechanism they describe is the design of record if the surface is ever
re-specified, and because each carries a real constraint a future proposal must
re-satisfy. See [§Unspecified: no work planned](#unspecified-no-work-planned).

- ~~Register the collision-free bare names (`append`, `prepend`, `concat`, `set`,
  `remove`, `merge`).~~ Cheap, but creates surface no document backs.
- ~~Overload candidate registry keyed by the head constructor of parameter 0,
  beside `TypeEnv` (which stays one-scheme-per-name).~~
- ~~Reorder `infer_call` / `infer_method_call` to infer + prune argument 0 before
  selecting and instantiating the callee scheme.~~
- ~~Deferred-resolution store for still-variable receivers, drained in the
  post-inference substitution phase; explicit ambiguity error, no silent
  default.~~
- ~~Codegen dispatch via `Value.osp_ty` for `get`, `contains`, `reverse`,
  `indexOf`.~~ (Not a position-keyed table — `Expr::Call` has no `Position`.)
- ~~Rename `mapKeys`→`keys`, `mapValues`→`values` and add the arity-2
  transformers.~~ Spec 0012 names `mapKeys` / `mapValues` normatively.
- ~~Implement `head`, `tail`, list `indexOf`, `entries`, `filterEntries`,
  `foldEntries`, `zipToMap`, `groupBy`.~~ None are specified.
- ~~Must-reject example pinning the overload-ambiguity error.~~ No overloading, no
  ambiguity error.
- ~~Document the principality loss in spec 0012; open a qualified-types plan.~~
  Nothing loses principality, because no overloading landed. §Principality stays
  here as the standing cost estimate.

Two constraints from that block **still bind** any future builtin, so they are
kept live rather than struck:

- [ ] Any new builtin needs a `builtin_docs_lang.rs` / `builtin_docs_sys.rs`
      entry — `every_builtin_is_documented_with_matching_arity` fails the build
      otherwise.
- [ ] `find-similar` before adding any C helper; no duplicate primitives.
