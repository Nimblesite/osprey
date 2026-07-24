# Plan 0004 — Collection / Map Standard-Library Surface

**Subsystem:** `crates/osprey-types` (builtin registry, `TypeEnv`, inference
order), `crates/osprey-codegen` (dispatch), `crates/osprey-types/src/builtin_docs*.rs`
(doc parity), `compiler/runtime` (a few new C ops), `examples/tested`
**Status:** Partially implemented. The **receiver-directed miscompile on
`length`/`isEmpty` is FIXED and shipped**. The rest of the bare-name surface
(`contains`, `get`, `reverse`, `indexOf` on List/Map) is **blocked on a type-system
change**, not on a dispatch tweak: `TypeEnv` is one-scheme-per-name and the callee
scheme is instantiated *before* any argument is inferred, so simply registering a
List `contains` **destroys** the string `contains`. Delivering it needs an overload
candidate registry, an inference reordering, a deferred-resolution store, and an
honest loss of HM principality (see §Principality)
**Scope:** **High** (was recorded as “Low–Medium”; that estimate was wrong — it
assumed dispatch was the only problem)
**Spec:** [0012-Built-InFunctions.md](../specs/0012-Built-InFunctions.md)

## Summary

The list/map runtime is implemented and correct, but most of it is exposed under
`listXxx`/`mapXxx` names while [the spec](../specs/0012-Built-InFunctions.md)
specifies **bare** names (`append`, `prepend`, `concat`, `get`, `reverse`,
`contains`, `keys`, `values`, …). A handful of spec'd operations have no
implementation at all. So the operations work, but the documented API does not
resolve — and the two bare names that *were* already exposed (`length`,
`isEmpty`) were exposed **unsoundly** until the fix below.

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

## Blocker — why bare names are not a dispatch tweak

The previous revision of this plan said “Osprey has no ad-hoc overloading today,
so dispatch must be receiver-type-directed at codegen time”, and left the choice
as an under-specified “decision needed”. That framing is wrong: **codegen is the
easy half.** The type checker is the blocker, for three verified reasons.

1. **`TypeEnv` holds exactly one scheme per name and `insert` silently
   overwrites.** [env.rs](../../crates/osprey-types/src/env.rs):
   `vars: HashMap<String, Scheme>`, and `insert` ends in
   `let _ = self.vars.insert(name, scheme);` — the discarded return value *is*
   the previous scheme. Registering a `List` `contains` therefore **destroys** the
   string `contains`, turning every existing string call site into a type error
   while codegen happily keeps emitting the string runtime call.
2. **The callee scheme is instantiated before any argument is inferred.**
   [expr.rs](../../crates/osprey-types/src/expr.rs) `lookup_ident` does
   `env.get(name)` → `instantiate(...)`, and `infer_call` unifies that
   already-chosen type against the arguments afterwards. There is no point in the
   current order at which a candidate could be *selected* by receiver type.
3. **These names are not typed `any`, so the `length` trick does not transfer.**
   `contains` is `(string, string) -> bool`, `indexOf` is
   `(string, string) -> Result<int, …>`, `reverse` is `(string) -> string`,
   `listGet` is `(List<t>, int) -> Result<t, …>`, `mapGet` is
   `(Map<k,v>, k) -> Result<v, …>`. Widening them to `any` to paper over the
   collision would delete real type checking on the string surface — the exact
   hole that produced the `length` miscompile above. Not an option.

Approach **B** (keep prefixed names, change the spec) stays rejected — the spec is
the contract. But it should be recorded that B is *cheap* and A is *expensive*,
which the old plan did not.

## Implementation — the minimal sound mechanism

Four pieces, in this order. Nothing here is optional; skipping (3) reintroduces
silent wrong-runtime dispatch.

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

### Principality — the honest consequence

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
| `examples/tested/basics/lists/map_basics.osp` | 122, 123, 129, 145 |
| `examples/tested/basics/lists/map_basics.ospml` | 122, 123, 129, 145 |
| `examples/tested/basics/types/recursive_unions.osp` | 75, 79, 83 |
| `examples/tested/basics/types/recursive_unions.ospml` | 80, 84, 89 |
| `examples/tested/basics/json/json_document_query.osp` | 26 |
| `examples/tested/basics/json/json_document_query.ospml` | 34 |
| `examples/tested/effects/fiber_effects.osp` | 6 (comment), 89 |
| `examples/tested/effects/fiber_effects.ospml` | 70 |

Plus the non-owned prose/generated surfaces that list the name:
`website/src/status.md` and the generated `website/src/docs/**` function index.
The `.osp`/`.ospml` twins must stay byte-equivalent per [FLAVOR-IR-EQUIV], so each
pair migrates together against one shared `.expectedoutput`.

### Two hard constraints on any new bare name

- **Builtin names are non-redefinable and therefore source-breaking.**
  [check.rs](../../crates/osprey-types/src/check.rs) rejects a user `fn` whose
  name is a builtin with ``cannot redefine built-in function `{name}` ``; the only
  exceptions are `SHADOWABLE_BUILTINS = ["test", "expect", "check"]`
  ([builtins.rs](../../crates/osprey-types/src/builtins.rs)). Every bare name this
  plan registers (`append`, `get`, `keys`, `head`, `tail`, …) is a **permanent
  reservation** that breaks any program already defining a function by that name.
  `head`/`tail` in particular are extremely likely user identifiers. Land the
  reservations in one batch, and note them in the spec's compatibility section.
- **Every new builtin needs a doc entry or the build fails.**
  [builtin_docs.rs](../../crates/osprey-types/src/builtin_docs.rs) carries the test
  `every_builtin_is_documented_with_matching_arity`, which asserts the documented
  name set equals the registered scheme set **and** that arities match. Adding a
  scheme without an entry in `builtin_docs_lang.rs` / `builtin_docs_sys.rs` is a
  failing `make test`, not a doc-debt TODO.

## Remaining implementation work

1. **Cheap first — collision-free bare names.** `append`, `prepend`, `concat`
   (list) and `set`, `remove`, `merge` (map) collide with nothing. Register them
   as additional schemes pointing at the existing lowering, add doc entries, and
   ship — no overload machinery needed. This retires a third of the gap table for
   a fraction of the cost.
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

- Extend [examples/tested/basics/lists/](../../examples/tested/basics/lists/)
  (`map_basics`, and the list examples) plus their `.ospml` twins to use the bare
  names and the new ops; refresh the shared `.expectedoutput`.
- Cover `head([])`/`tail([])` edge cases and `zipToMap` length-mismatch error.
- Add a **must-reject** case under `examples/failscompilation/` for the
  ambiguity error from step (3) — an unresolved receiver must fail loudly, and
  that is exactly the behaviour a future qualified-types plan would change, so it
  needs a pinned test.
- Add a codegen regression per newly-overloaded name in the shape of
  `bare_length_and_is_empty_dispatch_on_the_receiver_type`: one module exercising
  string + List + Map receivers, asserting each hits its own runtime and counting
  the string call sites.

## Risks / considerations

- Receiver-directed dispatch must produce a clear diagnostic when the receiver
  type is unknown, rather than defaulting to one collection kind. The `length`
  miscompile is the standing proof of what the silent default costs.
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
- [x] Define the collection spec anchors in 0012 — `[BUILTIN-COLLECTIONS]`,
      `[BUILTIN-LIST]`, `[BUILTIN-MAP]`, `[BUILTIN-COLLECTION-COMMON]`,
      `[BUILTIN-COLLECTION-LENGTH]`, `[BUILTIN-COLLECTION-ISEMPTY]` and the
      per-function ids (`[BUILTIN-LIST-APPEND]`, `[BUILTIN-MAP-SET]`, …) — so
      the code and example citations resolve, and the wildcard references in
      the list/map examples are expanded to concrete ids.
- [ ] Register the **collision-free** bare names (`append`, `prepend`, `concat`,
      `set`, `remove`, `merge`) against the existing lowering + doc entries.
- [ ] Overload candidate registry keyed by the head constructor of parameter 0,
      kept **beside** `TypeEnv` (which stays one-scheme-per-name).
- [ ] Reorder `infer_call` / `infer_method_call` to infer + prune argument 0
      **before** selecting and instantiating the callee scheme.
- [ ] Deferred-resolution store for still-variable receivers, drained in the
      existing post-inference substitution phase; **explicit ambiguity error**, no
      silent default.
- [ ] Codegen dispatch via `Value.osp_ty` for `get`, `contains`, `reverse`,
      `indexOf` (**not** a position-keyed table — `Expr::Call` has no `Position`).
- [ ] Rename impl `mapKeys`→`keys`, `mapValues`→`values`; register the spec's
      arity-2 `mapKeys`/`mapValues` transformers; migrate the 8 example files
      listed above (twins together, one shared `.expectedoutput`).
- [ ] Implement `head` (Result) and `tail` (total) — signatures, dispatch, C if
      needed. Note the source-breaking name reservation.
- [ ] Implement list `indexOf` (gated on the overload machinery).
- [ ] Implement `entries`, `filterEntries`, `foldEntries`, `zipToMap`, `groupBy`
      (computed callbacks already work — plan 0001 done).
- [ ] Doc entry in `builtin_docs_lang.rs`/`builtin_docs_sys.rs` for **every** new
      builtin — `every_builtin_is_documented_with_matching_arity` fails the build
      otherwise.
- [ ] `find-similar` before adding any C helper; no duplicate primitives.
- [ ] Must-reject example pinning the overload-ambiguity error.
- [ ] Extend `tested/basics/lists` + a map example (+ `.ospml` twins); refresh
      `.expectedoutput`.
- [ ] Document the principality loss in spec 0012; open a separate plan for
      qualified types (`preds` on `Scheme`, discharged by monomorphization, no
      dictionary passing) if principality is to be recovered.
- [ ] `make ci` green.
