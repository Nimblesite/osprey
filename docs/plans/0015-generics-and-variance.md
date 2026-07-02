# Plan 0015 — Generics with Variance and Generic Effects

**Subsystem:** tree-sitter-osprey, crates/osprey-syntax (both flavors), osprey-ast,
osprey-types, osprey-codegen, osprey-lsp
**Status:** Implemented
**Spec:** 0004 §Generics/§Variance ([TYPE-GENERICS-*], [TYPE-VARIANCE-*]),
0017 §Generic Effects ([EFFECTS-GENERIC-*]), 0003 §typeParamList/§effectSet,
0024 [FLAVOR-ML-GENERICS]

## Summary

User-declared generics land in both flavors: explicit type parameters on
functions, variance-annotated (`out`/`in`) type parameters on type and effect
declarations, generic effects (`effect State<T>`), and effect rows carrying
type arguments (`![State<int>]`). Variance is declaration-site (C#/Kotlin
style): `out T` restricts `T` to covariant positions, `in T` to contravariant
positions, and use-site subsumption is variance-directed *assignability* —
plain HM unification is untouched, so principal types survive.

## What works today (file:line evidence)

- `type Box<T>` / ML `type Box T` parse and check end-to-end
  (grammar.js:135-146, ml/parser.rs:230, check.rs `collect_type`).
- HM let-polymorphism: implicit generalization of top-level fns
  (check.rs `check_function`, env.rs `generalize`/`instantiate`).
- The assignability relation `unify_assignable` (unify.rs:102) already models
  Result auto-unwrap/wrap and function param-contra/ret-co — the natural
  attachment point for declared variance.
- Codegen specializes generic fns by inlining (genfn.rs), erases `Type::Var`
  to `i64` (types.rs:19), and effects run on a name-keyed handler stack
  (effects_runtime.c) — fully type-erased.

## Where it bailed / stopped

- No syntax: fn type params, effect type params, effect-row type args,
  variance keywords (`ERROR` nodes in both flavors).
- `Stmt::Function`/`Stmt::Effect` had no `type_params`; effect rows were bare
  `Vec<String>`; `Expr::TypeConstructor.type_args` was parsed then discarded.
- `infer_perform` (expr.rs:183) never unified arguments against operation
  parameters and returned the *shared* global op signature — two
  instantiations of one effect could not coexist.
- No variance representation or checking anywhere (grep-verified).

## Chosen design

1. **AST** (`osprey-ast/src/generics.rs`): `Variance`
   {Invariant/Covariant/Contravariant}, `TypeParam { name, variance }`,
   `EffectRef { name, type_args }`. `Stmt::Type.type_params` and new
   `Stmt::Function.type_params`/`Stmt::Effect.type_params` are
   `Vec<TypeParam>`; `Stmt::Function.effects` is `Vec<EffectRef>`;
   `Expr::Perform`/`Expr::Handler` gain `position` (per-site instantiation
   keys, mirroring `Expr::Lambda`).
2. **Surfaces** (identical canonical lowering per [FLAVOR-BOUNDARY]):
   - Default: `type Source<out T>`, `fn map<T, U>(...)`,
     `effect State<T> { ... }`, `![State<int>, Log]`.
   - ML: `type Source out T =`, `map<T, U> : ...` signature binder,
     `effect State T`, `! State<int>`. `out` is contextual; `in` (a hard
     keyword) is accepted inside type-parameter position only.
3. **Checker**:
   - `InferCtx` carries a constructor→variance table; `unify_assignable`
     matches same-name `Con` args variance-directed (co: expected←actual,
     contra: flipped, invariant: plain `unify`), with EXACT unification at
     the leaves — the coercive Result unwrap never applies under a container
     (it is representation-changing and codegen coerces only at direct value
     sites). Builtins: `Result<out, out>`, `List<out>`, `Fiber<out>`,
     `Map<inv, out>`.
   - Declaration-site position validation walks variant-field and
     effect-op types with a polarity that function parameters flip and
     nested constructors multiply; violations and variance on fn type
     params are errors.
   - Effects are stored generically (`type_params` + raw op sigs) and
     instantiated per handle site and per effect-row entry; a handler-scope
     stack resolves `perform` sites innermost-first (matching the runtime's
     innermost-wins dynamic semantics); `perform` arguments now unify
     against instantiated op parameters.
   - Inference publishes per-position resolved op signatures
     (`ProgramTypes::performs`, `ProgramTypes::handler_ops`).
4. **Codegen**: generic effect ops keep ONE erased ABI program-wide (every
   type-var-mentioning slot is a boxed `i64`), so the C runtime is untouched.
   Perform sites box erased arguments (bitcast for floats — never `fptosi`)
   and unbox erased results to the site-resolved type; handler arms unbox
   erased params at entry and box erased returns/resumes. Handlers register
   and performs look up under instantiation-mangled keys (`Stash$int`), so a
   handler/row instantiation mismatch — which the checker cannot rule out
   across the dynamic-scoping seam — misses the lookup and aborts loudly
   (`unhandled effect: …`) via a null-guard at every perform site, never
   type-confusing values. Monomorphic effects keep bare names and identical
   behavior.
5. **Runtime**: zero C changes (keys are opaque strings).

## Testing

- Expand `examples/tested/basics/types/pure_hindley_milner_test.{osp,ospml}`
  (fn type params), `type_equality_comprehensive.{osp,ospml}` (variance
  assignability), `effects/algebraic_effects_comprehensive.{osp,ospml}`
  (generic effect, two instantiations, rows with args) — shared goldens,
  byte-identical IR across flavors.
- New must-reject cases in `examples/failscompilation/`: covariant param in
  input position, contravariant param in output position, variance on a fn
  type param, effect type-argument mismatch.
- Unit tests: variance polarity walk, variance-directed assignability,
  effect instantiation, ML parse paths (`ml_coverage.rs`).

## Risks / considerations

- The tree-sitter parser (`src/parser.c`) is checked in and regenerated
  manually (`npm run generate`) — grammar edits are inert until regenerated.
- Cross-flavor Debug-string AST equality compares every new field — both
  lowerers must fill them identically.
- `perform` argument unification tightens checking; latent mismatches in
  examples surface as (genuine) type errors.
- Float payloads crossing erased effect slots must use bitcast boxing
  (`box_to_i64`), never `coerce_to`'s numeric `fptosi`.

## TODO

- [x] AST: `Variance`/`TypeParam`/`EffectRef` + new fields
- [x] Default grammar + parser regen + lowering
- [x] ML lexer/parser/CST/lowering
- [x] Checker: variance table, position checks, generic effects, publishing
- [x] Codegen: erased-slot box/unbox at perform/handler boundaries
- [x] LSP symbol/hover rendering of type params
- [x] Examples expanded in both flavors + failscompilation cases
- [x] Specs 0002/0003/0004/0017/0023/0024 updated
- [x] make ci green
