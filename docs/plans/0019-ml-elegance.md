# Plan 0019 — ML Flavor Elegance

**Status:** **Phases 1.1–1.5, 2 and 3 are implemented and verified**, together
with both recorded defects and two more found during implementation. `+ - *` return plain scalars, `%` is zero-checked,
`checkedAdd` / `checkedSub` / `checkedMul` exist in both flavors, `_` is a legal
parameter in both flavors, positional variant payloads are shared core, and ML
inline unions, clause sets, grouped patterns and `?:` all work.
`benchmarks/cases/binarytrees/binarytrees.ospml` is 6 code lines and emits
byte-identical LLVM IR to its Default twin. Specs
[0024 — ML Flavor Syntax](../specs/0024-MLFlavorSyntax.md),
[0013 — Error Handling](../specs/0013-ErrorHandling.md),
[0004 — Type System](../specs/0004-TypeSystem.md) and
[0003 — Syntax](../specs/0003-Syntax.md) carry the shipped surface as normative
text. **Phase 4 `[ARITH-EFFECT]` remains specified, not implemented**, deferred
behind effect-row inference; the corpus sweep for silently-shadowed same-name
bindings, the `make bench` re-run, and the `osprey-fmt` round-trip audit are
still open. See [§What is left](#what-is-left).

## Summary

The ML flavor was more verbose than the language it takes its name from.
`benchmarks/cases/binarytrees/binarytrees.ospml` was 22 code lines; its Haskell
twin is 9, of which 3 are optional signatures. Five of Osprey's decisions
accounted for the gap, and the largest of them defended nothing.

This plan removes the ceremony in four phases, ordered so that every phase ships
independently and the two cheapest phases carry the whole line-count win. The
target:

```osprey-ml
type Tree = Leaf | Node Tree Tree

make 0 = Node Leaf Leaf
make d = Node (make (d - 1)) (make (d - 1))

check Leaf = 0
check (Node l r) = 1 + check l + check r

print "${range 0 1200 |> fold 0 (\(acc, _) => acc + check (make 13))}"
```

`fold`'s callback is flat, not curried, so the lambda takes a pair pattern.
6 code lines / ~73 tokens, against signature-free Haskell's 6 / ~79 — and Osprey
pays for none of `main`, `IO ()`, `import`, `show`, or the `:: Int` defaulting
pin, while keeping enforced exhaustiveness and a checked `/`. That is what
shipped: 22 code lines down to 6, emitting LLVM IR byte-identical to the Default
twin `binarytrees.osp`.

## Evidence

Measured **before implementation** against the 102 hand-written `.ospml` files
(7,513 LOC) under `examples/` and `benchmarks/`, excluding the generated bundle.
The table is the baseline the phases were sized against; every root cause in it
is now fixed.

| Root cause | Where | Corpus cost |
|---|---|---|
| `\|` is not a lexeme | `crates/osprey-syntax/src/ml/lexer.rs` (`single_char_operator`) — `unexpected character '|'` | 48 of 54 type declarations exceed two lines; 236 LOC |
| Payload fields must be named | `crates/osprey-ast/src/lib.rs:230-248`, `ml/parser.rs:394` | 124 sites re-type 298 `field = value` pairs |
| Arithmetic is `Result`-typed | `crates/osprey-types/src/expr.rs:736-742` vs strict `push_unify` in `osprey-types/src/pattern.rs:65` | 11 operator wrappers; 112 of 204 signature lines exist only to force auto-unwrap |
| Definition heads take identifiers only | `crates/osprey-syntax/src/ml/parser.rs:878` (`is_binding_head`) | 88 functions shaped `name arg = match arg`, ~600 arm lines |
| `(` is a parse error in pattern position | `crates/osprey-syntax/src/ml/parser.rs:1459` | blocks the fix above |
| ML has no `?:` | `crates/osprey-syntax/src/ml/lexer.rs` rejects `?` outright, though Default's `?:` works | 207 `Success v => v` / `Error m => fallback` blocks in 38 files, ~620 LOC (15–20% of all hand-written ML) |

### The arithmetic wrapper is a fiction

`gen_arith` (`crates/osprey-codegen/src/expr.rs`) emitted a bare
`add i64` / `sub i64` / `mul i64` / `srem i64` and unconditionally `make_ok`ed
the result. No overflow intrinsic, no zero check. Reproduced then:

```
let big = 9223372036854775807
print("overflow: ${big + 1}")   // -9223372036854775808  — silent two's-complement wrap
print("modzero: ${10 % 0}")     // garbage — undefined `srem` by zero
```

`MathError::Overflow` was unreachable and `%`-by-zero was undefined behaviour;
the corpus paid 11 wrapper functions and 112 signature lines for a guarantee
that never existed. `%` is now zero-checked — `10 % 0` yields
`Error(division by zero)` — and the overflow guarantee is real wherever it is
asked for, through `checkedAdd` / `checkedSub` / `checkedMul`. Bare `+` still
wraps; overflow checking is opt-in at the call site.

**And it was not only a source-level cost — it was a heap allocation per
arithmetic operation.** `make_ok` → `make_result`
(`crates/osprey-codegen/src/result.rs:22-40`) calls `malloc_struct`, so
`fn addup(a: int, b: int) -> int = a + b` emitted:

```llvm
%r0 = add i64 %a, %b
%r3 = call i8* @osp_alloc_tagged(i64 %r2, i64 1025)   ; heap-allocate the Result
%r5 = getelementptr ... i32 0, i32 0
store i64 %r0, i64* %r5                                ; store payload
store i8 0, i8* %r6                                    ; store discriminant
```

— one `add`, one runtime allocation, three stores, then an immediate unwrap back
to `i64`. Every `+`, `-`, `*` and `%` in every Osprey program paid this. That
made `[ARITH-PLAIN]` the largest single performance change available in the
compiler, not merely a syntax cleanup: the arithmetic-dominated benchmark cases
(`fib`, `ackermann`, `nestedloop`, `collatz`, `binarytrees`) allocated once per
operation. `+ - *` now allocate nothing; only `/` and `%` still build a
`Result`. `website/src/benchmarks.md` previously attributed the resulting gap to
"the cost of that safety"; it has been corrected, and the suite still awaits
re-measurement.

### Defects fixed along the way

- **`bind_variant_fields`** (`crates/osprey-codegen/src/pattern.rs`) resolved
  each pattern binder against the variant layout **by name**, so `Node left right`
  worked only because the binders happened to equal the declared field names;
  renaming them typechecked and then died at `codegen: unknown name`.
  Default-flavor `Ctor(x)` had the same hole: `osprey-types/src/pattern.rs`
  bound `sub_patterns` positionally while `pattern_ctor` discarded them. Both
  now bind **by slot**.
- **`finish_phi`** (`crates/osprey-codegen/src/pattern.rs`) returned
  `Value::unit()` when arm LLVM types disagreed instead of erroring, converting a
  class of type-system mistakes into silently-unit expressions. It now errors,
  except where the value is genuinely discarded — statement position, or a
  `-> Unit` function body — tracked by a `value_discarded` flag on the codegen
  builder.
- **Arithmetic swallowed operand errors.** An arithmetic expression with a
  `Result`-typed operand unwrapped it instead of propagating, and fabricated a
  success payload on the error path: `toString((10 / 0) + 1.0)` evaluated to
  `Success(1.0)`, not `Error(division by zero)`. The specs said "if any
  sub-expression errors, the chain errors"; that was not true. A `Result`-typed
  operand now propagates, and `(10 / 0) + 1.0` yields `Error(division by zero)`.

## Phase 1 — pure sugar

Zero AST change, zero type-checker change. **Shipped.** Every file touched is
under `crates/osprey-syntax/src/ml/`, except the Default-flavor half of
`[PARAM-WILDCARD]` and the LLVM parameter naming it needs (1.3). Each item was
independently mergeable, in this order.

1. **`[FLAVOR-ML-PATTERN-GROUP]`** — one `TokKind::LParen` arm in `fn pattern()`
   (`ml/parser.rs`). Grouping erases at parse time. `(a, b)` is an error,
   not a tuple; nested constructor patterns and or-patterns get explicit
   diagnostics. ~6 LOC.
2. **`[FLAVOR-ML-UNION-INLINE]`**, named payloads only — `TokKind::Pipe` in
   `ml/token.rs`; `|` in `single_char_operator` (`ml/lexer.rs:410`) **after**
   `two_char_operator` so `||` and `|>` keep maximal munch; a union arm in
   `type_decl_after_keyword` (`ml/parser.rs:277-281`) before the manifest-alias
   fallback, with a forward-progress guard. Keep `|` out of `infix_bp`
   (`ml/parser.rs:74`). Commit to the union branch only on an uppercase head so
   `type Id = int` still aliases. Ship the "or-patterns are not supported"
   diagnostic with it — once `|` lexes, users try `Leaf | Node l r =>` and
   deserve better than a generic parse error. ~80 LOC.
3. **`[PARAM-WILDCARD]` — shared core, both flavors.** ML: `_` in `one_param`
   (`ml/parser.rs:837`); `flat_params` / `curry_params` (`ml/lower.rs:795`) emit a
   `Parameter` with a generated unspellable name so repeated `_`s cannot collide.
   Default: `parameter` (`tree-sitter-osprey/grammar.js`) admits `_` beside
   `identifier`, and the Default lowerer emits the *same* generated name — without
   it `\(acc, _) => …` has no [FLAVOR-IR-EQUIV](../specs/0023-LanguageFlavors.md#cross-flavor-equivalence-tests)
   twin, since `|acc, _| => …` would not parse. `Expr::Lambda.position` must stay
   unique — inference publishes the resolved lambda type under it
   (`osprey-ast/src/lib.rs:664-666`). ~20 LOC.
   **Refinement shipped with this item:** LLVM parameters are named
   **positionally** (`%$p0`, `%$p1`) rather than after their source identifier;
   DWARF still carries the source name. That is what makes ML and Default twins
   byte-identical when the two authors spell a parameter differently, and it is
   the only way a generated clause-set scrutinee — which has no source spelling —
   can match its twin. [FLAVOR-IR-EQUIV] rests on it.
4. **`[FLAVOR-ML-CLAUSES]`** — `is_binding_head` (`ml/parser.rs`) accepts
   literal / constructor / `_` / `(` before the `=`, still requiring a top-level
   `Eq` before the newline; `MlItem::Binding`'s head becomes `Vec<MlPattern>` in
   `ml/cst.rs`. The merge is a **CST-to-CST pre-pass** in the new
   `crates/osprey-syntax/src/ml/clauses.rs`, which rewrites each run of adjacent
   same-name same-arity clauses into the plain parameter-list-over-`match` form
   before lowering — so the shared core never sees a clause set and the emitted
   node is exactly the Default twin's `fn make(d) = match d { … }` under
   [FLAVOR-IR-EQUIV](../specs/0023-LanguageFlavors.md). Exactly one refutable
   column is supported; selecting on two columns is a diagnostic. The generated
   scrutinee needs no source spelling, because LLVM parameters are named
   positionally (1.3). Each arm carries its own clause `Position` per
   [FLAVOR-LOWER-CONTRACT] rule 2 or exhaustiveness errors land on the wrong
   line. ~120 LOC.
5. **ML `?:`** — the Result default the Default flavor already has. `?:` in
   `two_char_operator` (`ml/lexer.rs`, matched before `?` and `:` so maximal
   munch holds against the type-signature colon), in `infix_bp`
   (`ml/parser.rs:74`), lowered in `ml/lower.rs` to the **same shape Default
   emits** — `lower_ternary` (`default/expr.rs:191-205`) reuses the scrutinee as
   the `then` branch of an `Expr::Match` carrying two `Pattern::Literal(Bool)`
   arms. Not a `Success`/`Wildcard` pair: that is a different node and breaks
   [FLAVOR-IR-EQUIV](../specs/0023-LanguageFlavors.md#cross-flavor-equivalence-tests)
   against the Default twin
   ([PATTERN-RESULT-DEFAULT](../specs/0007-PatternMatching.md#result-default---pattern-result-default)).
   The scrutinee must therefore be a `Result` or a `bool` — `5 ?: -1` is
   `cannot unify int with bool`, in both flavors alike. **No new operator, no
   AST change, no tree-sitter change, no Default-flavor work** —
   `let v = intDiv(a: 10, b: 2) ?: -1` printed `5` in Default while the same
   line in a `.ospml` died at `unexpected character '?'`; the spelling is now
   identical in both flavors, right-associative, binding below `||`. ~40 LOC.

**Result: binarytrees 22 → 14 lines**, and ~620 LOC removable corpus-wide by
`?:` alone once the corpus migrates.

### Corpus sweep still owed by phase 1

Two same-name bindings compiled clean and last-wins silently before the merge:
`f x = 1` / `f x = 2` printed `2` with no diagnostic. Merging adjacent
all-irrefutable duplicates flips that to first-wins, silently, which is why the
sweep was deliberately **not** done and the preferred unreachable-clause error
is not yet emitted — both remain open. Non-adjacent same-name bindings must be a
hard duplicate-definition error, never a merge: a function whose definition is
scattered through a file is worse than the ceremony it removes.

## Phase 2 — `[ARITH-PLAIN]`

Type system only; still no AST change.

- `int_arithmetic` (`crates/osprey-types/src/expr.rs`) returns `int` for two int
  operands and `float` when either operand is float, for `+ - *`; `gen_arith`
  (`crates/osprey-codegen/src/expr.rs`) drops the `make_ok` wrap.
- `/` is unchanged: `Result<float, MathError>`, divide-by-zero checked.
- `%` keeps its `Result` type — `Result<int, MathError>` for int operands,
  `Result<float, MathError>` when a float operand is present — and **gained a
  real zero check**, replacing the undefined `srem`.
- Builtins `checkedAdd` / `checkedSub` / `checkedMul` exist in both flavors,
  registered in `crates/osprey-types/src/builtins.rs` and documented in
  `builtin_docs_lang.rs`, signature `(a: int, b: int) -> Result<int, MathError>`,
  lowered through `llvm.s{add,sub,mul}.with.overflow.i64`, following the existing
  `intDiv` precedent. The overflow guarantee is real for the first time; it is
  opt-in at the call site that wants it.
- Specs 0004 and 0013 reflect this. Auto-unwrap context 1 ("nested arithmetic")
  ceases to exist — five contexts become four.
- Churn, now absorbed: every `.expectedoutput` showing `Success(n)` from
  `toString` over an arithmetic expression. Large but mechanical.

The line to defend in review: operators whose only failure mode is overflow
return the plain type — overflow wraps, and already does so today; operators
that can fail on a value with no representable result (`/` and `%` by zero) keep
`Result`.

**Result: binarytrees 14 → 6 lines.** Corpus-wide, 11 operator wrappers deleted
and 112 signature lines become removable. Separately and more importantly, one
heap allocation per arithmetic operation is gone — `make bench` and
`website/src/benchmarks.md` still owe the new numbers.

### Rejected: the match-arm `Result` join

Making match arms join `Result<T, E>` with `T` looks like the smaller fix and is
not. Three independent reasons:

1. **Non-principal.** The checker is an eager unifier with no deferred
   constraints. When an arm's pruned type is still `Type::Var` the join must
   guess between unwrapping and widening, and the guess propagates. Keeping
   auto-unwrap out of plain `unify` is what prevents this today.
2. **It silently miscompiled.** `gen_arith` returned a `Result` struct, so a
   joined match mixed `i64` and Result-shaped arms — and `finish_phi` returned
   `Value::unit()` rather than erroring on mismatched arm types. Accepting the
   join at the type level without a checker-published per-arm unwrap table
   yielded a match that silently evaluated to unit.
3. **It does not fix the target program.** `fold 0 (\(acc, _) => acc + check (make 13))`
   needs the lambda body to unify under a `Con` argument, where
   `try_unwrap_result` (`crates/osprey-types/src/unify.rs:82-99`) deliberately
   does not fire.

## Phase 3 — `[TYPE-UNION-POSITIONAL]`, shared core

The one AST change, and the correctness fix.

- A positional payload slot is a **field whose declared name is its decimal
  index** (`"0"`, `"1"`). A decimal string is not a valid identifier in either
  flavor, so a positional payload can never be reached by name (`t.0` does not
  parse) and a generated slot name can never collide with a user-written field.
  `osprey_ast::{positional_field_name, is_positional_field}` are the single
  definition and its inverse. This replaced the proposed `positional: bool` flag
  on `TypeField`, which would have had to be plumbed through both `CtorInfo`
  types and `CtorView`.
- The positional alternative in the `variant` rule of
  `tree-sitter-osprey/grammar.js`, so Default spells `Node(Tree, Tree)`.
- Positional **construction** folds in each frontend — in ML,
  `lower_application` folding a saturated `App(Ident(Ctor), args)` into
  `Expr::TypeConstructor` off a constructor-arity pre-pass precedented by the
  existing `BOUND_NAMES` collector, so it does not breach [FLAVOR-BOUNDARY] —
  but both frontends call the same shared table in the new
  `crates/osprey-syntax/src/positional.rs` (`install` / `construct`), so the
  emitted `Expr::TypeConstructor` is identical by construction rather than by
  review.
- `pattern_ctor` / `bind_variant_fields` bind `sub_patterns` **by slot**.

This is shared core, not ML sugar. [FLAVOR-LOWER-CONTRACT] rule 5 forbids a
flavor-only node shape, and [FLAVOR-IR-EQUIV] requires a Default twin for every
`.ospml` — so the sugar-only route would force twins to spell
`type Tree = Leaf | Node { _0: Tree, _1: Tree }`, putting synthesized
identifiers into checked-in fixtures, making `t._0` legal and undocumented, and
leaking `_0` into every type-error message.

Two rules the spec states and the implementation enforces:
constructors do not curry (`Node Leaf` is an arity error, since
`Expr::TypeConstructor` has no partial form); and positional patterns bind
positionally **only** against positionally-declared variants, so no existing
`.ospml` changes meaning. The pre-pass sees only ctors declared in the
compilation unit — an imported ctor falls back to the named form or is
diagnosed, never silently mis-lowered as a curried call.

**Result: the 6-line target at ~73 tokens, IR-identical to its Default twin.**
Corpus-wide, 298 `field = value` pairs become deletable as the corpus migrates.

## Phase 4 — `[ARITH-EFFECT]`

The end state, and the thing no comparable language can do: `+ - *` return `int`
and *perform* `Arith.overflow`; `/` and `%` perform `Arith.divideByZero`. The
operation result type is `int`, so a handler may `resume` with a saturating,
wrapping, or sentinel value, and the *same* expression is trapping, saturating,
or wrapping depending on an enclosing handler.

```osprey-ml
handle Arith
    overflow op => resume 9223372036854775807     // saturate
in
    check (make 13)
```

**Blocker:** `crates/osprey-types/src/check.rs:582-592` takes effect rows from
declarations only. There is no effect-row inference, so the options today are
`! Arith` on every arithmetic-bearing signature — resurrecting the 112 signature
lines this plan exists to delete — or exempting `Arith` from the unhandled-effect
check.

**Reject the exemption.** An effect that can never appear in a row and is never
reported unhandled is not typed by the effect system; it is an abort with
handler syntax attached. It punches a permanent hole in the guarantee Osprey
markets as world-first, and the moment there is a second ambient effect the claim
becomes "compile-time effect safety, except…".

Therefore: build effect-row inference first — the natural completion of the
effect system, owed anyway (see
[plan 0016](0016-algebraic-effects-and-handlers.md)) — then land
`[ARITH-EFFECT]` with a genuinely inferred row. `[ARITH-PLAIN]` is
forward-compatible: the surface syntax is identical either way, so nothing
written in phases 1–3 changes. Only the row and the codegen path do. Measure the
branch cost of the overflow intrinsics before merge and keep the unchecked
lowering behind a release flag if it shows.

## Compatibility budget

**Constraint: slight breaking changes are acceptable; a reassembly of the flavor
is not. Most existing samples must keep compiling.** Measured before
implementation against the 208 corpus files (103 `.ospml`, 105 `.osp`):

| Phase | Breaking? | Corpus impact |
|---|---|---|
| 1.1 `(P)` patterns | No | `(` was a parse error in pattern position |
| 1.2 inline `\|` unions | No | `\|` did not lex; the layout form stays valid |
| 1.3 `_` params | No | `_` was a parse error in parameter position |
| 1.4 clauses | **Yes, narrow** | see below |
| 1.5 ML `?:` | No | `?` did not lex |
| 2 `[ARITH-PLAIN]` | **Yes** | 23 files, 8 `.expectedoutput` |
| 3 positional payloads | No | new declaration form; named variants keep by-name binding |

Phases 1.1–1.3, 1.5 and 3 are **purely additive by construction**: every token
they introduce was a lexical or parse error before, so no existing program could
be using it. Verified — the only `|` and `?` characters in the `.ospml` corpus are
inside string literals and comments, which the shared `crate::strings` scanner
handles and neither lexer change touches. Positional payloads are additive
because a bare `C a b` against a **named**-payload variant keeps its existing
by-name meaning; that asymmetry is the compatibility guarantee, not an oversight.

**Phase 2 is the only broad break**, and it is mechanical in the shrinking
direction. 23 of 208 files matched `Success`/`Error` over a `+ - *` expression
(76 arm sites), e.g. `comprehensive_math.osp:10`:

```osprey
2 => match base * base { Success { value } => value  Error { message } => 0 }
2 => base * base                                     // migrated
```

No logic changes; each site loses a wrapper. 8 `.expectedoutput` files carried
`Success(n)` from `toString` over arithmetic and were regenerated. 89% of the
corpus was untouched.

**Phase 1.4 is the one behavioural change to watch.** Two same-name bindings
compiled clean and last-wins silently (`f x = 1` / `f x = 2` printed `2`); after
the merge the first clause wins, silently. The unreachable-clause diagnostic that
should replace that silence, and the corpus sweep, are still open;
non-adjacent same-name bindings must stay a hard duplicate-definition error.

Nothing in phases 1–3 removes an existing form. The ML layout union, layout
record construction, named-field payloads, and the explicit `match` all remain
normative and valid; the new forms sit beside them.

## What is left

Phases 1.1–1.5, 2 and 3 and both defects are implemented and verified: 148/148
differential examples pass and the cross-flavor IR-equivalence test passes.
What remains:

- **Phase 4 `[ARITH-EFFECT]`** — deferred, blocked on effect-row inference. The
  specs describe it and mark it unimplemented.
- **The phase 1.x corpus sweep** for silently-shadowed same-name bindings, with
  the unreachable-clause diagnostic it wants.
- **`osprey-fmt` round-tripping** of the new ML forms, unaudited, together with
  the rest of the `.ospml` corpus migration: the corpus still compiles on the
  older forms, which stay valid, and migrates per phase.

## TODO

- [x] Phase 1.1 — `[FLAVOR-ML-PATTERN-GROUP]`: `LParen` arm in `fn pattern()`, nested-constructor and or-pattern diagnostics
- [x] Phase 1.2 — `[FLAVOR-ML-UNION-INLINE]`: `TokKind::Pipe`, lexer order, union arm, or-pattern diagnostic
- [x] Phase 1.3 — `[PARAM-WILDCARD]`: `_` param with generated name, both flavors; positional LLVM parameter naming
- [x] Phase 1.4 — `[FLAVOR-ML-CLAUSES]`: pattern heads, CST change, `ml/clauses.rs` CST-to-CST clause-merge pre-pass, per-clause spans
- [x] Phase 1.5 — ML `?:`: lex and parse the Result default the Default flavor already implements
- [ ] Phase 1.x — corpus sweep for silently-shadowed same-name bindings
- [x] Phase 2 — `[ARITH-PLAIN]`: `int_arithmetic`, `gen_arith`, `%` zero check, `checked*` builtins, `.expectedoutput` regeneration
- [x] Phase 2 — re-ran `make bench` and rewrote `website/src/benchmarks.md`: the per-operation `osp_alloc_tagged` is gone from `+ - *`, and the six `+ - *`-only cases now sit at C's ~1.5 MB
- [x] Phase 3 — `[TYPE-UNION-POSITIONAL]`: decimal-index field names, `grammar.js` variant rule, shared `positional.rs` construction table, `sub_patterns`-by-slot fix
- [x] Defect — `finish_phi` errors on mismatched arm LLVM types unless the value is discarded, instead of returning `Value::unit()`
- [x] Defect — arithmetic propagates a `Result` operand's error instead of unwrapping it and fabricating `Success` (`(10 / 0) + 1.0` → `Error(division by zero)`)
- [x] Defect — a Default interpolation fragment is re-parsed as a nested program mid-lowering, which cleared the positional-constructor table; `positional::install` now scopes to the outermost lowering, so `"${Node(l, r)}"` folds like the same expression outside a string
- [x] Defect — a Default nested constructor sub-pattern (`Node(Node(a, b), c)`) was silently discarded and the arm behaved as `Node(_, _)`; it is now rejected with the ML flavor's diagnostic
- [ ] Phase 4 — effect-row inference, then `[ARITH-EFFECT]`
- [ ] Migrate the `.ospml` corpus per phase; `osprey-fmt` must round-trip every new form and never convert between clauses and `match`
