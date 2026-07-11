# Plan 0015 — Generics with Variance and Generic Effects

**Subsystem:** tree-sitter-osprey, crates/osprey-syntax (both flavors), osprey-ast,
osprey-types, osprey-codegen, osprey-lsp
**Status:** Core landed and green. Declared type parameters (fn/type/effect),
declaration-site `in`/`out` variance with position checking and
variance-directed assignability, generic effects with per-site instantiation,
and explicit construction-site type arguments all work in BOTH flavors. Three
follow-ups remain (call-site type application, generic functions as values,
static proof of the handler/row instantiation seam) — see
[§What is left](#what-is-left-detailed).
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

## What is left (detailed)

The declared-generics + variance + generic-effects core is done. Three
follow-ups are known-incomplete, each with a concrete failing repro today:

### 1. Call-site type application (turbofish) — `identity<int>(5)`

**State:** unsupported. `identity<int>(5)` parses `int` as a *value*
identifier and errors `unknown identifier int`; the spec only ever shows
`identity<int>` in comments, never as callable syntax. Declaration-site
binders (`fn map<T,U>`) and construction-site args (`Box<int> { … }`) work;
the call-site form does not.

**Why it matters:** the only way to pin an otherwise-unconstrained
polymorphic return today is an annotated `let` (`let x: int = identity(5)`);
turbofish is the direct spelling the docs imply.

**Scope:** grammar (`call_expression` needs a `< typeList >` postfix that
does not collide with `<` comparison — the same GLR/lookahead hazard the
construction-site form already solved), a `type_args` field on `Expr::Call`,
lowering in both flavors, and a checker step that unifies the call's
instantiation variables against the written arguments (reuse
`current_fn_typarams` threading from construction sites). ML spelling TBD
(angle-bracket `f<int>(x)` vs a signature-only story).

### 2. Generic functions as first-class values

**State:** bails. `let g = identity; apply(g, 5)` errors
`codegen: unsupported construct: a generic function as a function value`.
This is [plan 0002](0002-codegen-generic-function-values.md)'s open bail —
generics did not create it, but declared type parameters enlarge its surface
(a user can now *name* a polymorphic function intending to pass it).

**Scope:** belongs to plan 0002 (per-`(name, concrete-type-args)`
monomorphic specialization cache + forwarder emission). Tracked there;
listed here because it is the most common thing users will hit once they
write explicit generic signatures. No new work in *this* plan — close 0002.

### 3. Static proof of the handler/row instantiation seam

**State:** checked-then-guarded, not statically proven. Function effect rows
(`!Stash<int>`) and `handle` sites each instantiate independently; because
`Type::Fun` carries no effect row, a call across a function boundary does not
unify the handler's instantiation with the callee's row. Today the runtime
closes the hole: handlers register and performs look up under
instantiation-mangled keys (`Stash$int`), so a mismatch misses and aborts
loudly (`unhandled effect: Stash$int.take`) — sound, but a **runtime**
failure where the effect system's promise is **compile-time** safety.

**Scope:** effect rows must flow through function types (an effect-row
component on `Type::Fun`, or a row-polymorphism variable `!E`) so a `handle`
site unifies its instantiation with the rows of functions invoked in the
handled body. This is effect-row polymorphism — larger than generics, and
the natural home is the effects roadmap ([plan 0016](0016-algebraic-effects-and-handlers.md)),
not this plan. Documented in `[EFFECTS-GENERIC-ROWS]` as a known limitation.

### 4. Nice-to-haves (unspecified, no work planned)

- **Bounded polymorphism** (`fn f<T: Ord>`): the spec name-drops an implicit
  `Iterable` constraint once ([0004] §Collection Types) but defines no
  constraint syntax; `TypeParam` has no `bounds` field. Out of scope until a
  concrete need lands.
- **Higher-kinded type parameters** (`F<_>`): not represented, not planned.

## TODO

Core (done):

- [x] AST: `Variance`/`TypeParam`/`EffectRef` + new fields
- [x] Default grammar + parser regen + lowering
- [x] ML lexer/parser/CST/lowering (incl. `Box<int>(item = …)` ctor args)
- [x] Checker: variance table, position checks (incl. effect ops), generic
      effects, per-site publishing, ctor type-arg resolution against the
      enclosing fn binder, arity errors
- [x] Codegen: erased-slot box/unbox at perform/handler boundaries;
      instantiation-mangled runtime keys + null-guard loud-abort;
      `has_type_var`-based erasure (nested `Result<T,…>` slots); Result-slot
      resume boxing
- [x] LSP symbol/hover rendering of type params
- [x] Examples expanded in both flavors + 7 failscompilation cases
- [x] Specs 0002/0003/0004/0017/0023/0024 updated
- [x] make ci green

Remaining:

- [ ] **Call-site type application** `identity<int>(5)` — grammar +
      `Expr::Call.type_args` + both-flavor lowering + checker unification
      (§What-is-left 1).
- [ ] **Generic functions as values** — land [plan 0002](0002-codegen-generic-function-values.md)'s
      specialization cache; then a generic `let g = identity` passed to a HOF
      compiles (§What-is-left 2).
- [ ] **Static handler/row seam** — effect-row polymorphism on `Type::Fun`
      so the instantiation mismatch becomes a compile error, not a runtime
      abort; owned by [plan 0016](0016-algebraic-effects-and-handlers.md)
      (§What-is-left 3).
- [ ] failscompilation case for turbofish once it lands (arity/instantiation
      mismatch at the call site).
