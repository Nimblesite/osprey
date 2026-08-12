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
| The falsification gate ([STAGE-FALSIFY]) | **Passed for the higher-order case only.** One unannotated `fn twice(f, x) = f(f(x))` serves a static callback and a dynamic one in the same program. The spec names three falsifiers; the generic-signal identity/reactive-rebuild and nested-parallel-matmul programs do not exist yet, so the gate as specified is incomplete — the prototype status holds until all three pass. |

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
7. **Monotonicity is not transitive, and unused arms erase unvalidated.**
   `validate_static_arms` checks missing arms, `resume`, and *direct*
   `perform` syntax only (`crates/osprey-ast/src/stage.rs`); a static arm that
   calls a helper with a dynamic effect passes, and an outer dynamic handler
   can then make the program survive erasure — violating
   [STAGE-STATIC-MONOTONE]. Because discharge runs before type/effect
   checking and drops the region's arms, an unknown, duplicate, wrong-arity,
   or ill-typed **unused** arm can vanish before ordinary validation sees it.
   Fix: validate every static arm against declared operation signatures and
   resolved transitive effect rows *before* erasure, with must-reject
   fixtures for the direct, transitive, unused-arm, duplicate-arm, arity, and
   ill-typed cases.
8. **Tail `resume` is specified but rejected.** Spec 0035 permits a tail-call
   `resume`; the implementation rejects every `resume` in a static arm.
   Either implement tail substitution or narrow the spec until it exists.
9. **Discharge runs per source file, before project assembly.** Declarations,
   handlers, and helpers split across files cannot resolve
   (`crates/osprey-project` assembles after `osprey-syntax` discharges).
   Discharge must move after assembly or work against a project-wide symbol
   index.
10. **`--deps` is approximate and error-tolerant.** Dependencies come from
    raw identifier names, so shadowing or passing an effectful function value
    can fabricate or hide one, and parse errors are dropped while the CLI
    still exits `0`. [STAGE-SIGNALS-EXACT] requires resolved effect-row
    provenance and a nonzero exit on any diagnostic.
11. **The wasm rejection contract is not implemented.** Spec 0035 requires a
    diagnostic naming the effect and operation for a residual dynamic effect
    on wasm32; today the build dies on an undefined coroutine symbol, which
    the corpus reads as a generic capability skip. Reject before linking,
    with the required message.

## Decision — why Osprey does not use MLIR today

### What MLIR is

[MLIR](https://mlir.llvm.org/) (Multi-Level Intermediate Representation) is a
subproject of LLVM. LLVM IR is one fixed, low-level language — registers,
loads, branches — so by the time a program reaches it, the structure an
optimiser would want (this is a matrix multiply; this loop is parallel) is
already gone. MLIR's answer is two ideas:

- **Dialects.** You define your own operations at whatever abstraction level
  fits the problem, and dialects coexist in one IR. `linalg` has `matmul`,
  `scf` has `parallel`, `gpu` has `launch`, `nvvm` is close to PTX.
- **Progressive lowering.** Compilation is a pipeline of conversion passes,
  each rewriting one dialect into a more concrete one, until only target-level
  operations remain. Each pass understands its own operations and nothing else.

Mojo, Triton, IREE and TensorFlow are built on it. The upstream GPU stack
(`gpu`, `nvgpu`, `nvvm`, `spirv`) is a working, maintained path from parallel
loops to PTX and SPIR-V.

### The decision

**Not now, deliberately, and the door is held open.** Osprey emits target-
agnostic textual LLVM IR and hands it to clang; `wasm32` is a sibling link
driver (`crates/osprey-cli/src/wasm.rs`). Static discharge is an Osprey-language
rewrite over the canonical AST. Five reasons:

1. **Correspondence is not dependency.** The architectural benefit — abstract
   operations plus rewriting layers that give them meaning — is a *design*, and
   staging already has it. Adopting the framework would buy the design a second
   time at the price of a large dependency.
2. **The user-facing win points the other way.** The reason to declare
   `Parallel` as an effect rather than a compiler dialect is that a user can
   read its handler, replace it in a test, and write their own. A conversion
   pattern written in C++ and TableGen inside the compiler is exactly the thing
   a user cannot do. Moving lowering into MLIR would move it *out* of the
   language, discarding the feature this plan exists to deliver.
3. **Nothing today needs it.** There is no device backend
   ([plan 0023](0023-gpu-computation.md) stages 3–4 are unstarted); the host
   backend is fused native loops. A dialect framework acquired before there is
   a target to lower to is infrastructure for a program that does not exist.
4. **It changes the shipping story on every platform.** MLIR means building
   against LLVM's C++ libraries at a pinned version, with TableGen in the build
   graph — against a Rust workspace whose current output is text a stock clang
   consumes. That reaches the release pipeline, the Homebrew tap, the Scoop
   bucket and the per-platform VSIX bundles ([docs/RELEASING.md](../RELEASING.md)),
   and it is a cost that must be paid by a benefit that does not yet exist.
5. **Deferring is cheap; adopting early is not.** The seam is an AST-to-AST
   pass with a checked contract. Adding an MLIR-backed discharge later is
   additive — a second implementation behind the same surface — whereas
   unwinding a premature dependency is not.

### What keeps the option open

These are invariants, not aspirations. Breaking one forfeits the option, so a
change that breaks one needs this section updated in the same edit:

- **The rewrite stays a separate pass over the canonical AST**
  (`crates/osprey-ast/src/lower_static.rs`), never entangled with LLVM text
  emission. A second discharge backend must be substitutable at that seam.
- **The four obligations stay as they are.** Total coverage is MLIR's full
  conversion, [STAGE-RESIDUE] is target legality, tail-resumptiveness
  ([STAGE-STATIC-TAIL]) is what makes an arm expressible as a rewrite pattern,
  and stage monotonicity keeps a pass from needing a runtime.
- **Effect signatures stay declarative** — a name, typed operations, no
  implementation — so an effect declaration can be mapped onto a dialect
  definition mechanically.
- **The host backend stays the reference semantics** ([GPU-BACKEND-HOST]), so
  any lowering path is verified against a fixed oracle rather than against
  itself.
- **Dialect-shaped effects are named after their MLIR counterparts** where one
  exists — `Parallel` ↔ `scf.parallel`, `Tensor` ↔ `linalg`, `Alloc` ↔
  `memref` — so the mapping stays obvious to whoever picks this up.

### When to pick it up

Revisit at [plan 0023](0023-gpu-computation.md) stage 4's decision checkpoint,
and adopt when **three or more** of these hold:

- A device target is being written and hand-rolled lowering to PTX, Metal or
  WGSL would duplicate what `gpu`/`nvgpu`/`nvvm`/`spirv` already provide.
- Tiling, fusion or vectorization is needed and would otherwise mean
  reimplementing `linalg`/`affine` passes.
- Two or more device backends exist and share more than half their lowering.
- The measured gap to a published baseline (the Mojo numbers in
  [plan 0023](0023-gpu-computation.md)) is attributable to lowering quality
  rather than to kernel extraction or checked arithmetic.
- The LLVM C++ build dependency has been shown acceptable on every release
  platform, with the pinned-version policy written down.

Adoption, if it happens, is **backend-only**: `static effect` and
`handle static` stay the surface, the obligations stay the contract, and a
program's meaning must not change ([STAGE-COMPAT]). The acceptance gate is that
the differential corpus stays byte-exact against the host backend with the MLIR
path enabled.

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

Close limitations 2–4 and 7–9: resolve names before rewriting, keep the
original definition alive for diagnostics, bind operation arguments
hygienically, validate arms against operation signatures and transitive
effect rows before erasure, implement (or de-specify) tail `resume`, and
move discharge after project assembly. Limitation 7 is the branch review's
P0.3 — it is soundness, not polish, and leads this stage. Gate: a fixture
per limitation, each currently mis-reported, reporting correctly.

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
      [STAGE-STATIC-FINITE] checks with distinct messages — **direct syntax
      only**: the monotone check is not transitive and unused arms erase
      unvalidated (limitation 7); tail resume is rejected outright
      (limitation 8)
- [x] Region-stack rewrite with per-region helper specialization
- [x] `tests/effects/staged/staged_effects.test.osp` + golden
- [x] Five must-reject fixtures with `.expectedoutput`
- [x] `osprey --deps` + `osprey_syntax::dependency_sets`
- [x] Falsification gate run and recorded
- [x] `crates/osprey-cli/tests/staged_effects.rs`: residue asserted against the
      emitted IR, with the dynamic control case that proves it can fail
- [ ] A fixture for [STAGE-STATIC-FINITE]'s rewrite bound
- [ ] The two missing [STAGE-FALSIFY] falsifiers: generic-signal
      identity/reactive rebuild, nested-parallel matmul
- [ ] [STAGE-SIGNALS-EXACT]: `--deps` from resolved effect rows, diagnostics
      propagated, nonzero exit on error (limitation 10)
- [ ] Wasm residual-effect rejection naming effect and operation before
      linking (limitation 11)
- [ ] Static handler-state coverage either way (limitation 6)
- [ ] Stage 2: ML surface + twin + `[FLAVOR-IR-EQUIV]` coverage
- [ ] Stage 3: name resolution before rewriting; keep originals for
      diagnostics; hygienic argument binding; arm validation against
      signatures and transitive rows before erasure (limitation 7 / review
      P0.3); tail `resume` or spec narrowing (limitation 8); discharge after
      project assembly (limitation 9)
- [ ] Stage 4: instantiation-keyed rewrite rules
- [ ] Stage 5: `signal` form, LSP dependency view, reactive example
- [ ] Stage 6: device dialects and the `kernel` region form
- [ ] Re-evaluate the MLIR decision at [plan 0023](0023-gpu-computation.md)
      stage 4's checkpoint against the adoption criteria above; record the
      outcome here either way
- [ ] Static handler state: decide and test, or reject it explicitly
