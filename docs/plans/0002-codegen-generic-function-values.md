# Plan 0002 — Generic Functions & Lambdas as First-Class Values

**Subsystem:** `crates/osprey-codegen` (with `crates/osprey-types` support)
**Status:** Mostly done — slot-driven specialization + let-alias + emit-once
specialisation cache landed; one scoped remainder (a still-generic lambda
*returned* from a generic function)
**Spec:** [0004-TypeSystem.md](../specs/0004-TypeSystem.md), [0005-FunctionCalls.md](../specs/0005-FunctionCalls.md)

## Summary

Function values work when their type is fully concrete, **and a generic
function now specialises wherever a consuming slot fixes its ABI**. The only
remaining refusal is a function value whose type is *nowhere* concrete — a
still-generic lambda returned from a generic function — which keeps its loud
bail rather than risk treating a `string`/`float` instantiation as `i64`.

## What works today

- Capture-free lambda → constant cell; capturing lambda → heap closure with
  snapshotted captures — [crates/osprey-codegen/src/closure.rs](../../crates/osprey-codegen/src/closure.rs).
- Named top-level (monomorphic) function as a value via an emitted forwarder cell
  — `named_fn_cell` / `emit_forwarder` in
  [closure.rs](../../crates/osprey-codegen/src/closure.rs).
- **Generic function into a concrete function-typed slot** — `eval_arg`
  specialises it to the slot's ABI by emitting its (params, body) exactly like
  a capture-free lambda (`expr.rs` → `closure::emit_closure`); the FFI variant
  lifts through `raw_callback_lambda`. Implements [TYPE-GENERICS-FN].
- **`let g = identity` (generic target)** — bound as a call alias in `gen_bind`
  ([lower.rs](../../crates/osprey-codegen/src/lower.rs)): `g(5)` specialises at
  its call sites exactly as a direct call would; a value use resolves the alias
  where a consuming slot fixes the ABI. Annotated lets work the same way.
- **Function-valued arguments to generic HOFs** — `try_inline` registers a
  lambda/function-typed argument's signature for its parameter
  (`bind_inline_arg` in [genfn.rs](../../crates/osprey-codegen/src/genfn.rs)),
  so the inlined body's `f(x)` dispatches through the closure cell. Previously
  this emitted a call to a nonexistent symbol — a **link error** — for
  `fn also(x, f) = f(x)` applied to a lambda (the Kotlin-`let` idiom).
- **The `-> T` generalization poisoning is fixed at the root**: builtin schemes
  hand-write `Var(0)`/`Var(1)` as quantified binders, and the checker's fresh
  supply used to hand out those same ids to live inference variables; once a
  var-var unification routed through a colliding id, `TypeEnv::free_vars`
  resolved *through* the builtin's binder and silently blocked
  let-generalization (`fn identity<T>(x) -> T = x` lost its polymorphism
  depending on unification direction). The checker now reserves the builtin
  binder ids (`builtins::RESERVED_SCHEME_VARS`,
  [crates/osprey-types/src/check.rs](../../crates/osprey-types/src/check.rs)).
- Concreteness gate: `fn_value_concrete` decides whether a function type is safe
  to lower as a value — [crates/osprey-codegen/src/types.rs](../../crates/osprey-codegen/src/types.rs).

## What is left

One bail remains, by design until per-instantiation cells exist:

```rust
// closure.rs — lambda_value
"a closure value with a still-generic type (wrap it in a function with concrete parameter/return types)"
```

Repro: `fn mk<T>(x: T) = |y| => x` then `let f = mk(1)` — the lambda's `y` is
genuinely polymorphic at the point the value must be emitted (the binding
generalizes to `∀y. (y) -> int`), so no single concrete ABI exists. Pinned by
`generic_function_value_without_a_slot_is_rejected`
([crates/osprey-codegen/src/lib.rs](../../crates/osprey-codegen/src/lib.rs))
and the `cli_e2e` codegen-error fixtures. Supporting it needs specialisation
at the *call* sites of the value (`f(0)` → int), i.e. a per-instantiation
cache keyed by use-site resolution — the original "on-demand monomorphic
copies" strategy, now needed only for this last shape.

Known hazard (pre-existing, unchanged): a *recursive* generic function
specialised by inlining falls back to a direct call to a symbol that is never
emitted (the `inlining` re-entry guard's fallback). Recursive generic
functions as values are untested territory.

- The FFI-callback case is a deliberate, permanent restriction (captures cannot
  cross the C ABI): now pinned by
  `examples/failscompilation/ffi_capturing_callback.ospo`.

## Testing

- `examples/tested/basics/function_composition_test.osp` §"Generic functions as
  first-class values": `identity<T>` into a concrete slot, a let-alias call,
  and `alsoDo(x, f) = f(x)` applied at **two instantiations** (int and string).
- `generic_function_into_concrete_slot_specialises` and
  `one_generic_function_at_one_abi_is_emitted_exactly_once` unit tests
  (codegen lib.rs) — the latter pins the emit-once cache: same ABI shares a
  body, different ABI does not.
- `examples/failscompilation/ffi_capturing_callback.ospo` — capturing lambda
  across the C boundary still rejected loudly.

## TODO

- [x] Resolve concrete types at the use site — done slot-driven: the consuming
      slot's `FnSig` (call argument, FFI callback) or a call alias (`let`).
- [x] Replace the `named_fn_cell` "generic function as a function value" bail
      for slot-typed uses and let-bound names.
- [x] Keep the FFI-callback restriction; pin it with a failscompilation case.
- [x] `tested/basics` coverage for a generic function used at ≥2
      instantiations; `.expectedoutput` refreshed.
- [x] Root-cause and fix the `-> T` generalization poisoning (builtin binder
      id collision — `RESERVED_SCHEME_VARS`).
- [ ] Materialize a still-generic *returned* lambda against its call-site
      instantiations (per-instantiation cache) — replaces the last
      `lambda_value` bail.
- [x] Emit-once dedupe cache for repeated same-slot specializations — **done**.
      `emit_closure_keyed` (`closure.rs`) takes a `(function, slot ABI)` key
      built by `specialisation_key`, so N call sites at the SAME ABI share one
      emitted body and one constant cell while distinct ABIs still specialise
      apart. It reuses the existing `fnval_cells` map (whose bare-name keys
      cannot collide: a specialisation key contains `|`), so the cache adds no
      new codegen state. A capturing cell is never shared — captures are
      recomputed at each site and snapshot the values live *there*. Pinned by
      `one_generic_function_at_one_abi_is_emitted_exactly_once`
      (`codegen/src/lib.rs`): three `int` uses plus one `string` use of
      `identity` emit **2** bodies, not 4.
- [x] `make ci` green.
