# Plan 0004 — Collection / Map Standard-Library Surface

**Subsystem:** `crates/osprey-types` (builtin registry), `crates/osprey-codegen`
(dispatch), `compiler/runtime` (a few new C ops)
**Status:** Partially implemented
**Spec:** [0012-Built-InFunctions.md](../specs/0012-Built-InFunctions.md)

## Summary

The list/map runtime is implemented, but it is exposed under `listXxx`/`mapXxx`
names while [the spec](../specs/0012-Built-InFunctions.md) specifies **bare**
names (`append`, `prepend`, `concat`, `get`, `reverse`, `contains`, …). A handful
of spec'd operations have no implementation at all. So the operations work, but
the documented API does not resolve.

## What works today

- Registered (prefixed) builtins: `listAppend`, `listPrepend`, `listConcat`,
  `listReverse`, `listGet`, and the `mapGet`/`mapSet`/`mapRemove`/`mapMerge`/
  `mapContains`/`mapKeys`/`mapValues` family — [crates/osprey-types/src/builtins.rs](../../crates/osprey-types/src/builtins.rs).
- Codegen dispatch for these — [crates/osprey-codegen/src/collections.rs](../../crates/osprey-codegen/src/collections.rs).
- A string `indexOf` already exists (note: list `indexOf` is separate and missing).

## Gaps (spec → impl)

Spec uses bare names ([0012 §Lists/§Maps](../specs/0012-Built-InFunctions.md)):

| Spec name | Status |
|-----------|--------|
| `append`, `prepend`, `concat`, `get`, `reverse`, `contains` (list) | implemented under `listXxx` — **name not exposed** |
| `head(list) -> Result<T, IndexError>` | **missing** |
| `tail(list) -> List<T>` (total) | **missing** |
| `indexOf(list, value) -> Result<int, IndexError>` | **missing** (string version exists) |
| `get`, `set`, `remove`, `merge`, `contains`, `keys`, `values` (map) | implemented under `mapXxx` — **name not exposed** |
| `filterEntries`, `foldEntries`, `zipToMap`, `groupBy` | **missing** |

## Decision needed: aliasing vs. overloading

`contains` and `get` mean different things on lists vs. maps. Osprey has no ad-hoc
overloading today, so dispatch must be **receiver-type-directed** at codegen time.
Two viable approaches:

- **A (recommended):** Expose the bare spec names and resolve `listXxx`/`mapXxx`
  by the inferred receiver type during codegen dispatch. Keep one implementation
  per operation (no duplicate code).
- **B:** Keep prefixed names as the only surface and change the spec. (Rejected —
  the spec is the contract.)

## Implementation plan

1. **Expose bare names in the type registry.** Register `append`, `prepend`,
   `concat`, `get`, `reverse`, `contains`, `set`, `remove`, `merge`, `keys`,
   `values` as the public spellings; have the checker pick the list- or map-typed
   signature from the receiver type.
2. **Receiver-directed codegen dispatch.** In
   [collections.rs](../../crates/osprey-codegen/src/collections.rs), map a bare
   name + receiver type to the existing `listXxx`/`mapXxx` lowering. No new
   runtime needed for these.
3. **Implement `head`/`tail`.** `head` returns `Result<T, IndexError>`; `tail` is
   total (`tail([]) == []`). Add type signatures, codegen dispatch, and (if not
   derivable from existing `listGet`/slice ops) thin C runtime helpers.
4. **Implement list `indexOf`.** Add C runtime equality scan + signature + dispatch.
5. **Implement `filterEntries`, `foldEntries`, `zipToMap`, `groupBy`.** These take
   callbacks; computed callbacks already work (plan 0001, higher-order calls,
   is done and retired). Add C runtime + signatures + dispatch.
6. **Run `find-similar` before adding** each C helper to avoid duplicating an
   existing scan/fold primitive.

## Testing

- Extend [examples/tested/basics/lists/](../../examples/tested/basics/lists/)
  and a map example to use the bare names and the new ops; refresh
  `.expectedoutput`.
- Cover `head([])`/`tail([])` edge cases and `zipToMap` length-mismatch error.

## Risks / considerations

- Receiver-directed dispatch must produce a clear diagnostic when the receiver
  type is unknown, rather than defaulting to one collection kind.
- `head`/`tail` are *functions*, distinct from the `[head, ...tail]` list
  *pattern* already shipped — do not conflate.

## TODO

- [ ] Register bare list/map names; resolve signature by receiver type.
- [ ] Receiver-directed codegen dispatch to existing `listXxx`/`mapXxx` lowering.
- [ ] Implement `head` (Result) and `tail` (total) — signatures, dispatch, C if needed.
- [ ] Implement list `indexOf`.
- [ ] Implement `filterEntries`, `foldEntries`, `zipToMap`, `groupBy`
      (computed callbacks already work — plan 0001 done).
- [ ] `find-similar` before adding any C helper; no duplicate primitives.
- [ ] Extend `tested/basics/lists` + a map example; refresh `.expectedoutput`.
- [ ] `make ci` green.
