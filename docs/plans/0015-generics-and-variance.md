# Plan 0015 — Generics with Variance and Generic Effects

**Subsystem:** tree-sitter-osprey, crates/osprey-syntax (both flavors), osprey-ast,
osprey-types, osprey-codegen, osprey-lsp
**Status:** The core is implemented and passing tests. Declared type parameters
(fn/type/effect),
declaration-site `in`/`out` variance with position checking and
variance-directed assignability, generic effects with per-site instantiation,
explicit construction-site type arguments, and static proof of the
handler/operation-instantiation seam all work in BOTH flavors. Call-site type
application and the already-documented returned-generic-lambda limitation
remain — see
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
- The assignability relation `unify_assignable` models the safe one-way
  promotion `T -> Result<T, E>` plus function param-contra/ret-co. The inverse
  `Result<T, E> -> T` is forbidden; declared variance uses this relation.
- Codegen specializes generic fns by inlining (genfn.rs), erases `Type::Var`
  to `i64` (types.rs:19), and effects run on a name-keyed handler stack
  (effects_runtime.c) — fully type-erased.

## Previous gaps

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
     the leaves. A `Result<T, E>` never coerces to `T`, under a container or at
     a direct value site, because that would erase failure and change the
     representation. Builtins: `Result<out, out>`, `List<out>`, `Fiber<out>`,
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
4. **Codegen**: generic effect ops keep one erased ABI program-wide (every
   type-var-mentioning slot is a boxed `i64`), so the C runtime is untouched.
   Perform sites box erased arguments (bitcast for floats — never `fptosi`)
   and unbox erased results to the site-resolved type; handler arms unbox
   erased params at entry and box erased returns/resumes. Static operation
   summaries retain each resolved instantiation across calls and handlers, so
   a `Stash<string>` handler does not discharge a `Stash<int>` operation and
   compilation fails while the requirement remains at entry. Handlers still
   register and performs still look up under instantiation-mangled keys
   (`Stash$int`); the null-guard is defense in depth, not normal rejection.
   Monomorphic effects keep bare names and identical behavior.
5. **Runtime**: zero C changes (keys are opaque strings).

## Testing

- Expand `tests/regressions/basics/types/pure_hindley_milner_test.{osp,ospml}`
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

### 2. Generic functions as first-class values — ✅ landed (plan 0002)

**State (2026-07): works.** `let g = identity` binds as a call alias; a
generic function flowing into a concrete function-typed slot specialises to
the slot's ABI (emitted like a capture-free lambda); lambda arguments to
generic HOFs dispatch indirectly (`fn also(x, f) = f(x)` applied at two
instantiations works). The enabling checker fix: builtin scheme binder ids
(`Var(0)`/`Var(1)`) no longer collide with live inference variables
(`RESERVED_SCHEME_VARS`), which had been silently blocking let-generalization
of `-> T`-annotated functions. Plan 0002 still rejects a generic lambda returned
from a generic function.

### 3. Static proof of the handler/operation instantiation seam — ✅ landed

**State:** statically enforced. `crates/osprey-types/src/effect_rows.rs`
computes a closed-program summary of the operations required by each function
and callback. Every requirement includes its resolved generic arguments. Calls
propagate those requirements, and a handler removes only an operation arm with
the same effect name and instantiation. A `Stash<string>` handler therefore
cannot discharge `Stash<int>.take`; the remaining requirement is a compile
error at program entry. The runtime's instantiation-mangled null guard remains
a defensive backstop.

Explicit rows such as `!Stash<int>` are checked contracts and instantiation
hints, not handlers. They constrain operations in a function body but do not
grant authority when that function is called. Unannotated functions infer the
same requirements without needless return or effect annotations.

**Representation boundary:** this safety pass does not add a general open-row
variable to `Type::Fun`; it uses fixed-point operation summaries for the
current closed-program language surface. If future syntax exposes independently
quantified row variables in public higher-order types, that representation and
its HM generalization rules remain separate work in
[plan 0016](0016-algebraic-effects-and-handlers.md).

### 4. Unspecified extensions (no work planned)

- **Bounded polymorphism** (`fn f<T: Ord>`): the spec mentions an implicit
  `Iterable` constraint once ([0004] §Collection Types) but defines no
  constraint syntax; `TypeParam` has no `bounds` field. Out of scope until a
  concrete use case is specified.
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
      instantiation-mangled runtime keys + null-guard diagnostic;
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
- [x] **Generic functions as values** — landed via plan 0002, now retired; the
      shipped contract is [spec 0004](../specs/0004-TypeSystem.md) `[TYPE-GENERICS-FN]`:
      slot-driven specialization + let-alias + inline fn-typed arg
      registration; `let g = identity` passed to a HOF compiles and runs
      (§What-is-left 2). Only the returned still-generic lambda remains, in
      plan 0002.
- [x] **Static handler/operation seam** — resolved operation summaries make an
      instantiation mismatch a compile error; explicit rows are contracts, and
      the runtime null guard is only a backstop (§What-is-left 3 and
      [plan 0016](0016-algebraic-effects-and-handlers.md)).
- [ ] failscompilation case for turbofish once it lands (arity/instantiation
      mismatch at the call site).
