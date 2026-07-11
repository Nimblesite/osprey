# Plan 0002 — Generic Functions & Lambdas as First-Class Values

**Subsystem:** `crates/osprey-codegen` (with `crates/osprey-types` support)
**Status:** Partially implemented
**Spec:** [0004-TypeSystem.md](../specs/0004-TypeSystem.md), [0005-FunctionCalls.md](../specs/0005-FunctionCalls.md)

## Summary

Function values work when their type is fully concrete. A capture-free lambda
becomes a constant cell; a capturing lambda is heap-allocated with its captures
snapshotted; a named monomorphic function can be taken as a value via a
forwarder. What does **not** work is taking a *generic* function or a
*generic-typed* lambda as a value — the backend refuses rather than risk treating
a `string`/`float` instantiation as `i64`.

## What works today

- Capture-free lambda → constant cell; capturing lambda → heap closure with
  snapshotted captures — [crates/osprey-codegen/src/closure.rs](../../crates/osprey-codegen/src/closure.rs).
- Named top-level (monomorphic) function as a value via an emitted forwarder cell
  — `named_fn_cell` / `emit_forwarder` in
  [closure.rs](../../crates/osprey-codegen/src/closure.rs).
- Concreteness gate: `fn_value_concrete` decides whether a function type is safe
  to lower as a value — [crates/osprey-codegen/src/types.rs](../../crates/osprey-codegen/src/types.rs).

## Where it bails

```rust
// closure.rs:52  — generic-typed lambda value
"a closure value with a still-generic type (wrap it in a function with concrete parameter/return types)"

// closure.rs (~205) — capturing lambda as an FFI callback
"a capturing lambda as an FFI callback (captures cannot cross the C boundary; use a named function)"

// closure.rs:288 — generic function as a function value
"a generic function as a function value"
```

The root cause is that generic functions are **monomorphized by inlining at each
call site**, so there is no single definition whose address can be taken, and a
lambda inside a generic body has one source position serving many instantiations.
The FFI-callback case is a deliberate, permanent restriction (captures cannot
cross the C ABI) and is **out of scope** — keep that error.

## Implementation plan

1. **Decide the strategy: on-demand monomorphic copies.** When a generic
   function (or a lambda in a generic body) is used as a *value*, emit a concrete
   specialization keyed by the resolved type arguments at that use site, then take
   the address of that specialization.
2. **Key specializations.** Add a `(name, [concrete type args])` → emitted-symbol
   cache alongside `fnval_cells` so each distinct instantiation is emitted once.
3. **Resolve the concrete types.** Pull the resolved type arguments for the use
   site from the type table (the inliner already computes these for call sites;
   reuse that resolution rather than re-inferring).
4. **Emit a forwarder per specialization** and a constant cell pointing at it,
   mirroring `emit_forwarder`/`named_fn_cell` but parameterized by the concrete
   `FnSig`.
5. **For generic-typed lambdas**, materialize the lambda against the resolved
   `FnSig` (reuse `emit_closure`) instead of erroring in `lambda_value`.
6. **Keep the FFI-callback bail** unchanged.

## Testing

- Extend a `tested/basics` example: define `identity<T>(x: T) -> T`, pass it as a
  value used at two instantiations (`int` and `string`), and call both. Refresh
  `.expectedoutput`.
- Add a generic-typed lambda stored in a `let` and passed to a higher-order
  function (higher-order calls themselves already work — that was plan 0001,
  since completed and retired).
- Keep/extend a `failscompilation` case proving a *capturing* lambda as an FFI
  callback is still rejected.

## Risks / considerations

- Specialization can multiply emitted code; dedupe rigorously via the cache.
- The higher-order-call path this builds on (plan 0001) is already done and
  retired; this plan is the remaining generic-value slice of that work.
- Verify ABI correctness for `float`/`string` instantiations (the exact hazard the
  current guard guards against).

## TODO

- [ ] Add a `(name, type-args)` specialization cache for function values.
- [ ] Resolve concrete type args at the use site from the type table.
- [ ] Emit one forwarder + cell per specialization; replace the `closure.rs:288`
      bail.
- [ ] Materialize generic-typed lambdas against the resolved `FnSig`; replace the
      `closure.rs:52` bail.
- [ ] Leave the FFI-callback restriction in place.
- [ ] Add `tested/basics` coverage for a generic function used at ≥2
      instantiations; refresh `.expectedoutput`.
- [ ] `make ci` green.
