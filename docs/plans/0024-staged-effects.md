# Plan 0024 — Staged Effects: static handlers as lowering passes

**Subsystem:** `crates/osprey-ast` (the rewrite) + `tree-sitter-osprey` +
`crates/osprey-syntax` (surface and pipeline order) + `crates/osprey-cli`
(`--deps`)
**Status:** **Prototype landed and road-tested in the Default flavor.** Stage 1
(surface, obligations, rewrite, zero residue) and the falsification gate are
done and covered by tests. Stages 2–6 — the ML surface, generic instantiation
identity, the reactive layer, and everything device-side — are unstarted.
**Spec:** [0035-StagedEffects.md](../specs/0035-StagedEffects.md)

## Summary

An effect declared `static` is answered by the compiler and leaves nothing
behind. The implementation is one AST-to-AST rewrite
(`crates/osprey-ast/src/lower_static.rs`) applied **at the flavor boundary** —
inside `parse_program_with_flavor`, where many CSTs already converge on one
canonical AST ([FLAVOR-BOUNDARY]). Every consumer of a parsed program (the
checker, codegen, the language server, the test harnesses, project assembly)
therefore receives an ordinary program with no compile-time effect in it, and
no consumer can forget to run the pass. That ordering is the whole design: four
features that would each need their own machinery instead fall out of one pass.

The one thing that must *not* run after it is the dependency query, since
discharge is what erases the dependencies. `osprey_syntax::dependency_sets`
parses without discharging and is the only supported way to read a row's
dependency set.

## Road test results

Run against the prototype, Default flavor, release build. The programs are
`tests/effects/staged/staged_effects.test.osp` and the five must-reject
fixtures named below.

| Claim | Result |
| --- | --- |
| Zero residue ([STAGE-RESIDUE]) | A static handler emits **no** `__osprey_handler_push`, `__osprey_handler_lookup` or arm thunk: 114 IR lines versus 148 for the dynamic twin, with identical output. The operation's arm body appears inline as straight-line code. |
| GPU legality ([STAGE-GPU-LEGAL]) | A kernel performing a **static** effect compiles and runs. The same kernel performing a **dynamic** effect is still rejected — `GPU kernel must be pure; it performs: Log.write`. **The GPU checker was not modified.** Erasure before checking turned the existing purity gate into the stage-legality gate. |
| WebAssembly ([STAGE-WASM]) | The static program links to a 27 KB `wasm32` module. Its resuming dynamic twin fails to link: `undefined symbol: __osprey_coro_free`. The continuation runtime is exactly what static handlers do not need. |
| Dependency sets ([STAGE-SIGNALS-DIRTY]) | `osprey --deps` derives them exactly: a helper reading one signal reports one, its caller inherits it transitively, a function reading two reports two, and a function reading none **appears nowhere**. No dependency arrays, no runtime tracking. |
| The falsification gate ([STAGE-FALSIFY]) | **Passed.** One unannotated `fn twice(f, x) = f(f(x))` serves a static callback and a dynamic one in the same program. |

### What the gate actually settled

The original design memo called stage polymorphism "the actual research risk"
and budgeted a year for it. **It does not arise.** Because the rewrite runs
before inference, the checker never sees one function at two stages — it sees
an empty row at the static call site and an ordinary row at the dynamic one.
Open effect rows in `Type::Fun` ([plan 0016](0016-algebraic-effects-and-handlers.md))
remain worth having for published higher-order signatures, but staging does not
depend on them.

The cost of that ordering, stated plainly: a type error inside a static handler
arm is reported against the substituted code, not the arm as written.

## What landed

- `static effect E { … }` and `handle static E … in …` in the grammar
  (`tree-sitter-osprey/grammar.js`, `static_stage` rule) and the Default
  lowerer; `Stage` on `Stmt::Effect` and `Expr::Handler`
  (`crates/osprey-ast/src/stage.rs`).
- The four obligations, each with its own message and must-reject fixture in
  `examples/failscompilation/`: `staged_partial_static_handler.ospo`
  ([STAGE-STATIC-TOTAL]), `staged_static_handler_resumes.ospo`
  ([STAGE-STATIC-TAIL]), `staged_static_arm_requires_dynamic.ospo`
  ([STAGE-STATIC-MONOTONE]), plus both stage-mismatch directions
  (`staged_dynamic_effect_static_handler.ospo`,
  `staged_static_effect_dynamic_handler.ospo`). [STAGE-STATIC-FINITE] is
  enforced by a rewrite bound but has no fixture yet.
- The rewrite itself: a top-down traversal over a stack of enclosing regions.
  Nested regions compose, a `perform` resolves to the innermost answering arm,
  and each region specializes the helpers it reaches — so two regions may
  answer the same effect differently in one program, which
  `staged_effects.test.osp` pins.
- `osprey --deps` and `osprey_syntax::dependency_sets`, reporting each
  function's dependency set from the pre-discharge AST.
- `crates/osprey-ast/src/mutate.rs`, the by-unique-reference twin of `visit`,
  which any future rewriting pass reuses.

## Known limitations of the prototype

These are real and none of them is hidden by a passing test:

1. **Default flavor only.** ML parses `effect`/`handle` as dynamic
   unconditionally (`crates/osprey-syntax/src/ml/lower.rs`). An ML twin of the
   staged suite cannot be written yet, which also means staging is outside
   `[FLAVOR-IR-EQUIV]` coverage.
2. **Specialization consumes the original.** A function a region specializes is
   removed from the program. Calling it from outside any region reports
   `unknown identifier` instead of naming the undischarged operation.
3. **Identifier-level renaming.** Specialization rewrites `Expr::Identifier`
   occurrences, so a local binding shadowing a specialized function name would
   be renamed with it. No corpus program does this; the fix is to resolve names
   before rewriting.
4. **No hygiene on argument binding.** An operation's arguments are bound with
   `let` using the arm's parameter names. A capture is possible if an argument
   references a name the arm also binds.
5. **Generic static effects are keyed by effect name only,** so
   `Signal<Count>` and `Signal<Cursor>` are not yet distinct rewrite targets.
   [STAGE-SIGNALS-EXACT] requires per-instantiation identity, which is why
   road test 4 uses one effect per signal.
6. **Static handler state is untested.** A `mut` cell captured by a static arm
   has no coverage either way.

## Stages

Each stage keeps `make ci` green and the differential harness byte-exact, and
must not change the meaning of any program an earlier stage accepted.

### Stage 1 — surface, obligations, rewrite ✅

Landed. Acceptance: `tests/effects/staged/staged_effects.test.osp` passes under
every memory backend, the five fixtures reject with their exact messages, and
the emitted IR for a static effect contains no handler-runtime symbol.

### Stage 2 — ML flavor parity

`static effect` and `handle static` in the ML frontend, an ML twin of the
staged suite sharing its Default golden, and staging inside `[FLAVOR-IR-EQUIV]`.
Gate: the twins emit byte-identical IR.

### Stage 3 — soundness hardening

Close limitations 2–4: resolve names before rewriting, keep the original
definition alive for diagnostics, and bind operation arguments hygienically.
Gate: a fixture per limitation, each currently mis-reported, reporting
correctly.

### Stage 4 — generic instantiation identity

Key rewrite rules by resolved instantiation, not effect name, reusing the seam
that already distinguishes `Stash<string>` from `Stash<int>`
(`crates/osprey-types/src/effect_rows.rs`, [plan 0015](0015-generics-and-variance.md)).
Gate: `Signal<Count>` and `Signal<Cursor>` are distinct dependencies in
`--deps` and distinct rewrite targets.

### Stage 5 — the reactive layer

A `signal` declaration form over stage 4, the dirty set exposed through the
language server so a developer can see a widget's dependencies, and the
reactive counter as a runnable example. Gate: changing one signal recomputes
exactly the functions whose dependency set names it — asserted, not narrated.

### Stage 6 — device dialects

`Parallel`, `Tensor` and `Alloc` as static effects whose handlers are the
lowering passes a device backend needs, and the `kernel` region form of
[STAGE-GPU-KERNEL]. This stage is **downstream of
[plan 0023](0023-gpu-computation.md) stage 3–4**: it supplies the surface, not
the code generator, and the MLIR-versus-direct decision stays with that plan.

## TODO

- [x] `static_stage` grammar rule; regenerate `tree-sitter-osprey/src/parser.c`
- [x] `Stage` on `Stmt::Effect` and `Expr::Handler`; Default lowerer reads it
- [x] `stage::discharge` at the flavor boundary, so every consumer gets a
      discharged program and staging violations arrive as parse diagnostics
- [x] [STAGE-STATIC-TOTAL] / [STAGE-STATIC-TAIL] / [STAGE-STATIC-MONOTONE] /
      [STAGE-STATIC-FINITE] checks with distinct messages
- [x] Region-stack rewrite with per-region helper specialization
- [x] `tests/effects/staged/staged_effects.test.osp` + golden
- [x] Five must-reject fixtures with `.expectedoutput`
- [x] `osprey --deps` + `osprey_syntax::dependency_sets`
- [x] Falsification gate run and recorded
- [x] `crates/osprey-cli/tests/staged_effects.rs`: residue asserted against the
      emitted IR, with the dynamic control case that proves it can fail
- [ ] A fixture for [STAGE-STATIC-FINITE]'s rewrite bound
- [ ] Stage 2: ML surface + twin + `[FLAVOR-IR-EQUIV]` coverage
- [ ] Stage 3: name resolution before rewriting; keep originals for
      diagnostics; hygienic argument binding
- [ ] Stage 4: instantiation-keyed rewrite rules
- [ ] Stage 5: `signal` form, LSP dependency view, reactive example
- [ ] Stage 6: device dialects and the `kernel` region form
- [ ] Static handler state: decide and test, or reject it explicitly
