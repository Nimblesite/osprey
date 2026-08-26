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
still-generic lambda returned from a generic function. It remains a codegen
error because treating a `string`/`float` instantiation as `i64` would be
incorrect.

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
- **Generic function as a *builtin* iterator callback** — `map`/`filter`/
  `fold`/`forEach` are lowered specially in
  [iter.rs](../../crates/osprey-codegen/src/iter.rs) (fused loop), not via
  `try_inline`, so their callback resolution had the same link-error gap: a
  generic reducer like `fn add(a, b) -> int = (a + b) ?: a` passed to `fold(0, add)`
  fell through to `call @add`, a symbol that generics never emit. `callback_of`
  now resolves a name found in `fn_defs` to an inlined lambda, beta-reducing it
  per element. Implements [BUILTIN-ITER-CALLBACK]; pinned by
  `generic_function_as_an_iterator_callback_inlines_not_calls_a_missing_symbol`
  (codegen lib.rs) and
  `tests/regressions/basics/memory/struct_allocation_stress.{osp,ospml}`.
- **The `-> T` generalization poisoning is fixed**: builtin schemes
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
functions as values are not covered by tests.

- The FFI-callback case is a deliberate, permanent restriction (captures cannot
  cross the C ABI): now pinned by
  `examples/failscompilation/ffi_capturing_callback.ospo`.

## Testing

- `tests/regressions/basics/function_composition_test.test.osp` §"Generic functions as
  first-class values": `identity<T>` into a concrete slot, a let-alias call,
  and `alsoDo(x, f) = f(x)` applied at **two instantiations** (int and string).
- `generic_function_into_concrete_slot_specialises` and
  `one_generic_function_at_one_abi_is_emitted_exactly_once` unit tests
  (codegen lib.rs) — the latter pins the emit-once cache: same ABI shares a
  body, different ABI does not.
- `examples/failscompilation/ffi_capturing_callback.ospo` — capturing lambda
  across the C boundary remains rejected.

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
- [x] **A function value returning a TAGGED COLLECTION keeps its element type.**
      The lowered `FnSig` carried parameter slots, a return `LType`, a `Result`
      inner type and a `FiberSig` — but no return OWNER. A named function
      recovers one through `Codegen::fn_ret_owner`; a call through a closure
      cell has only the signature, so `|| => [0.5, 1.25]` handed back an
      untagged `i8*` and `listGet` on it met an `i64` payload against a
      `double` default. The program was REJECTED with
      `match arms disagree on type: \`i64\` and \`double\`` — truthful about
      the representation, but naming no user construct, so there was nothing in
      the message to act on. The same loss reached a captured value, a lambda
      passed as a function-value parameter, and a generic HOF
      (`fn applyTo(f, x) = f(x)`).

      `FnSig` gained a fifth slot, the return owner, filled by
      `Codegen::fn_value_sig` from `types::owner_name` and applied in
      `closure::returned`; `monofn`'s `Abi` return slot carries it too, so a
      specialisation stays element-typed at its call site. `FiberSig` gained
      `elem_owner`/`elem_payload_owner` on the same principle — the tag now
      TRAVELS with the signature instead of being rebuilt by each consumer,
      which is what left the closure routes binding `None`. That deleted the
      per-consumer reconstruction in `cast.rs`, `stmt.rs` and
      `ctor_field_handle`.

      One trap on the way: tagging the return `List#double` made a returned
      list LITERAL a **segfault** rather than a rejection, because a literal
      leaves the body as a flat `{length, data}` header. `closure::ret_as_sig`
      now converts at the escape through `listlit::escaping`, the same seam
      `fit_lambda_return` uses on the named-function path.

      Pinned by `a function value returning a collection keeps its element type`
      (`tests/regressions/basics/functional/functional_showcase.test.{osp,ospml}`)
      — verified to FAIL on the pre-fix compiler — and, for the channel routes,
      `a channel reached through a closure keeps its element descriptor`
      (`tests/core/collections/nested_generic_collections_fibers.test.{osp,ospml}`).
      Both flavors share one golden; green under default / `--memory=gc` /
      `--memory=arc`. Closes the function-value face of
      [issue #227](https://github.com/Nimblesite/osprey/issues/227) and the
      last defect of retired plan 0004.

- [ ] Materialize a still-generic *returned* lambda against its call-site
      instantiations (per-instantiation cache) — replaces the last
      `lambda_value` bail.
- [x] **Recursive generic function → the annotations it forced are gone.**
      The load-bearing annotations this item named are no longer load-bearing:
      `simulate`/`mix` (`tests/core/gpu/stress.test.osp`) and `rowDot`/`train`
      (`tests/core/gpu/mlkernels.test.osp`) now infer, and this plan's own
      acceptance criterion was to "drop them when this lands", so they are
      dropped. Both files stay **byte-exact** against their goldens with the
      annotations removed, in both flavors — which is the proof they were
      redundant, per CLAUDE.md's no-inferable-annotation rule.

      The `try_inline` re-entry guard still rejects the shape it was written
      for, and it does so TRUTHFULLY:
      ``walk` is recursive and its return type is not inferred; annotate its
      return type so it is emitted as a real function` (golden
      `examples/failscompilation/recursive_generic_needs_annotation.ospo`), not
      the LLVM `use of undefined value '@name'` clang crash it replaced. A
      recursive generic that inference cannot resolve is therefore rejected
      with an actionable message rather than miscompiled — the stabilization
      bar. Emitting a real monomorphic definition for that residue is a
      performance/ergonomics improvement, not a correctness gap, and is
      recorded in spec [GPU-KERNEL-FORM] rather than held open here.

      The ML twins keep their `simulate : (GpuBuffer<int>, int) -> …` signature
      LINES. That is a different construct from an inline parameter annotation,
      the convention is pervasive across the `.ospml` corpus, and the
      redundant-annotation gate does not sweep ML at all
      ([#215](https://github.com/Nimblesite/osprey/issues/215)) — settling ML
      signature policy belongs with that issue, not with a unilateral edit to
      two files.

- [x] **Block-bodied lambda with internal `let` as a HOF/kernel callback** —
      **stale item, now pinned.** The recorded repro
      `gpuMap(|r| => { let base = (r * 3) ?: 0 ... })` does NOT fail with
      `unknown identifier \`base\``; it lowers correctly and answers
      `[1, 4, 7, 10]` over `gpuIota(4)`. Re-measured 2026-08-26 against the
      compiler at this commit AND at its parent, so an earlier change had
      already fixed it and nothing recorded that — the item stayed open because
      no test covered the shape, not because the shape was broken.

      Now covered: `a block-bodied kernel keeps its internal let bindings`
      (`tests/core/gpu/buffers.test.{osp,ospml}`) exercises both the `fn(r) =>`
      and `|r| =>` spellings plus a block that widens `int` to `float`, and the
      GPU suite's own differential runs it under **both**
      `OSPREY_GPU_KERNELS=extract` and `=inline` — byte-identical output under
      each, so the claim holds for the extracted and the inlined lowering
      alike. Named kernels are no longer the only supported form, so
      [GPU-KERNEL-FORM] is satisfied for block bodies. Green under default /
      `--memory=gc` / `--memory=arc`, both flavors sharing one golden.
- [x] Generic function as a **builtin iterator callback** (`map`/`filter`/
      `fold`/`forEach`) — the fused-loop path in `iter.rs` had the same
      link-error gap as user HOFs did before `bind_inline_arg`. `callback_of`
      now inlines a name found in `fn_defs` instead of emitting `call @name`.
      A record-typed `fold` accumulator is carried through the uniform slot and
      recovered at its real type (`rebuild_acc`), then owned by the enclosing
      region so ARC frees it (zero leaks). Implements [BUILTIN-ITER-CALLBACK];
      pinned by two codegen unit tests and
      `tests/regressions/basics/memory/struct_allocation_stress.{osp,ospml}`
      (green under default/GC/ARC, `ARC_LEAKY=0`).
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
