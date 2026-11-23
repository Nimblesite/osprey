# Or-patterns and `min` / `max`

Osprey lacks two constructs every top-tier language has. A `match` cannot give several patterns one body, so arms that answer alike are copied. There is no `min` or `max`, so each program writes its own. Both gaps force duplicated code, and duplication is a bug.

This plan adds both. The specs are written; nothing is built yet.

- **Or-patterns**: `Space | Tab | Newline => "blank"`. Specified in [PATTERN-OR](../specs/language/0007-PatternMatching.md#or-patterns--pattern-or).
- **`min` and `max`**: `max(low, min(x, high))`. Specified in [BUILTIN-MIN-MAX](../specs/runtime/0012-Built-InFunctions.md).

## 🚨 Priority

This is a language gap that blocks top tier, ranked with the gaps in [generic error payloads](generic-error-payload.md). It was found while writing the multi-shot test matrix for the Clean Architecture sample: the tests needed a clamp helper and one arm per request variant where a single or-pattern arm would do. Tick an item only with a corpus regression that passes under GC, ARC and wasm32.

## Rules being implemented

- [PATTERN-OR-SYNTAX]: `|` joins two or more patterns, as a whole arm or in any nested slot, in every flavor.
- [PATTERN-OR-ORDER]: alternatives are tried left to right; the first match supplies the bindings.
- [PATTERN-OR-BINDINGS]: every alternative binds the same names at the same types.
- [PATTERN-OR-TYPE]: each alternative is typed against the scrutinee or slot.
- [PATTERN-OR-EXHAUSTIVE]: an or-pattern counts as one arm per alternative; an unreachable alternative is rejected.
- [PATTERN-OR-BODY]: the body is compiled once; a `many` continuation captured in it resumes alike on GC, ARC and wasm32.
- [BUILTIN-MIN-MAX-INT]: integer `min` and `max` use the type's own order, signed or unsigned.
- [BUILTIN-MIN-MAX-FLOAT]: float `min` and `max` are IEEE 754-2019 `minimum` and `maximum`; NaN is never dropped and `-0.0` is below `0.0`.
- [BUILTIN-MIN-MAX-TYPE]: both arguments share one numeric type; a mix is a compile error.
- [BUILTIN-MIN-MAX-VALUE]: each is a function value and is legal in a GPU kernel.
- [BUILTIN-MIN-MAX-SHADOWING]: a user declaration named `min` or `max` shadows the builtin.

## Or-patterns

### Specs

- [x] [PATTERN-OR] written in `docs/specs/language/0007-PatternMatching.md`, with its seven rules
- [x] [TYPE-MATCH-EXHAUSTIVE] says an or-pattern counts as one arm per alternative
- [x] ML: `docs/specs/language/0024-MLFlavorSyntax.md` no longer says or-patterns are rejected
- [x] Python: `case A() | B():` row in `docs/specs/language/0041-PythonFlavorSyntax.md`
- [x] TypeScript: adjacent `case` labels in `docs/specs/language/0054-TypeScriptFlavorSyntax.md`
- [ ] Remove every "not implemented" status from those five places once the last item below is ticked

### Syntax

- [x] `osprey-ast`: add `Pattern::Or(Vec<Pattern>)` in `src/pattern.rs`. Every flavor lowers to it. No frontend copies an arm
- [x] Tree-sitter grammar (`src/toolchain/tree-sitter-osprey/grammar/patterns.js`): `pattern` takes `sep1('|', …)` at the lowest precedence; inside `(…)`, `{…}` and `[…]` slots the alternatives end at `,` or the closing delimiter
- [x] Grammar test proving `|` after a pattern never starts the lambda form `|x| => body`, including an arm whose body is such a lambda followed by an or-pattern arm
- [ ] Default parser (`osprey-syntax/src/default`): build `Pattern::Or`; reject a leading or trailing `|`
- [x] ML parser (`osprey-syntax/src/ml`): the same at arm level; a nested or-pattern in a juxtaposed slot needs parentheses; `|` in a type declaration stays a variant separator
- [ ] Python parser (`osprey-syntax/src/python`): `case A() | B():` to `Pattern::Or`
- [x] TypeScript parser (`osprey-syntax/src/typescript`): adjacent `case` labels with no statements between them to `Pattern::Or`; fallthrough out of a case with statements stays rejected
- [x] Highlight queries and both VS Code grammars colour `|` in a pattern as an operator. The Tree-sitter capture test and TextMate token tests distinguish pattern operators from union and lambda delimiters.

### Checker

- [x] `osprey-types/src/pattern.rs`: type every alternative against the scrutinee or slot type ([PATTERN-OR-TYPE])
- [x] Binding agreement ([PATTERN-OR-BINDINGS]): the same name set in every alternative, one unified type per name. The diagnostic names the missing variable and the alternative that omits it
- [x] `osprey-types/src/pattern/usefulness.rs`: expand an or-pattern into its alternatives for exhaustiveness and reachability; report an unreachable alternative at that alternative's range, with the usual witness for a non-exhaustive match
- [x] Binder walks handle the new node: `osprey-ast/src/lower_static/hygiene.rs`, `osprey-ast/src/stage_rows/callables.rs`, `osprey-project/src/patterns.rs` and `osprey-project/src/opaque.rs`
- [ ] An opaque record's constructor stays unmatchable outside its module through an or-pattern alternative
- [ ] `any` scrutinee: an or-pattern of type-annotation patterns follows [TYPE-ANY]; it never makes the match exhaustive

### Backend

- [x] `osprey-codegen`: each alternative runs its tests and jumps to one shared body block; a binder is a phi of the alternatives' values. The body is emitted once ([PATTERN-OR-BODY])
- [x] Nested alternatives go through `nested::capture`: a failed alternative falls to the next alternative, then to the next arm; bindings install only when one alternative matched whole
- [x] ARC: a binder taken from either alternative is retained and released once, with no leak and no double release on the alternative that failed part-way
- [x] wasm32: the same blocks and phis; corpus green under `OSPREY_TARGET=wasm32`
- [ ] `many` lowering (`osprey-types/src/many`): a `match` with or-pattern arms inside a `many` region is rewritten with the arm kept whole; a request captured in the body resumes once per `resume` call with the alternative's bindings ([MULTI-HANDLE-MANY], [MULTI-REPLAY])
- [ ] Static handler discharge and staged effects ([STAGE-WASM]) pass the node through unchanged

### Tooling

- [ ] Formatter (`osprey-fmt`): one space each side of `|`; an or-pattern too long for the line breaks before each `|`
- [ ] LSP (`osprey-lsp`): hover, go-to-definition and rename on a binder that appears in every alternative act on all of its occurrences
- [ ] Flavor conversion ([plan 0057](0057-flavor-conversion.md)): an or-pattern converts between all four spellings and back without change
- [ ] Symbolic execution ([plan 0034](0034-symbolic-execution.md)) explores each alternative as its own path
- [ ] Debugger and profiler line tables point at the alternative that matched

### Tests

Expand existing suites. Add no new test file.

- [ ] `src/osprey/tests/flavors/matching/matching.test` in both flavors, loaded with permutations: payload-free variants, literals (`int`, `float`, `string`, `bool`), constructor payloads, `Success` and `Error`, tuples, lists with a rest binder, rows, and or-patterns nested three levels deep, each asserted for every alternative and for the values that reach no alternative
- [x] Binding cases: the same binder taken from each alternative, a function-valued binder, a closure that captures the binder and escapes the arm, a binder shadowing a `mut` cell ([PATTERN-BINDING-SCOPE])
- [x] Order cases: overlapping alternatives where only the first can supply the binding ([PATTERN-OR-ORDER])
- [ ] Multi-shot: an or-pattern arm whose body performs a `control many` operation, resumed two and three times, in `osprey-cli/tests/common/many_permutations.rs` and the multi-shot corpus, with identical output under GC, ARC and wasm32
- [ ] Python and TypeScript twins share the Default golden
- [ ] `src/osprey/examples/failscompilation/`: an alternative that omits a binder, binders of two types, an unreachable alternative after an earlier arm, an alternative repeated inside one pattern, a leading `|`, a trailing `|`, an alternative of another union, and a non-exhaustive match whose witness is shown. Expand `match_not_exhaustive` and `match_unreachable_after_catch_all`
- [ ] Regression in `src/osprey/tests/regressions/` for the grammar's lambda case
- [ ] Corpus runs through `src/toolchain/crates/run_test_corpus.sh` under GC, ARC and wasm32

### Use it

- [ ] `src/osprey/examples/cleanarch_todo`: join the arms of `TodoRequest`, `TodoDispatch` and the test matrix that share a body, then recount the "Half the code" table in its README
- [ ] Sweep `src/osprey/` for adjacent arms with identical bodies and join them; deslop `top-offenders` goes down

## `min` and `max`

### Specs

- [x] [BUILTIN-MIN-MAX] and its five rules written in `docs/specs/runtime/0012-Built-InFunctions.md`
- [x] `min`, `max` row in the operator table of `docs/specs/language/0037-ArithmeticEffects.md`
- [x] Python: the two-argument row and the still-rejected iterable forms in `docs/specs/language/0041-PythonFlavorSyntax.md`
- [ ] TypeScript: say whether `Math.min` and `Math.max` with two arguments lower to the builtins, in [FLAVOR-TS-EXPRESSIONS]
- [ ] Remove the "not implemented" statuses once the last item below is ticked

### Checker

- [ ] `osprey-types/src/builtins/core.rs`: register `min` and `max` with a scheme built as `arithmetic::absolute_scheme` is, over every numeric type, two arguments of one type ([BUILTIN-MIN-MAX-TYPE])
- [ ] A mixed call is rejected naming both types: "`min` needs two values of one numeric type: got int and float"
- [ ] `osprey-types/src/builtins.rs`: add both names to `SHADOWABLE_BUILTINS` ([BUILTIN-MIN-MAX-SHADOWING])
- [ ] `osprey-types/src/effect_rows/value_shapes.rs` and `arithmetic.rs`: both are total, with an empty `Arith` row, called directly or through an alias, callback, stored or returned function value
- [ ] `osprey-types/src/builtin_docs_core.rs`: documentation for hover, completion and `--docs`
- [ ] Constant folding of two literal arguments, NaN and signed zeros included

### Backend

- [ ] `osprey-codegen/src/expr/arguments.rs` and `arithmetic.rs`: integers lower to `llvm.smin`, `llvm.smax`, `llvm.umin` and `llvm.umax` at the operand width; floats lower to `llvm.minimum` and `llvm.maximum`
- [ ] Function-value form: one specialized wrapper per numeric type, as `abs` has
- [ ] wasm32: `f64.min`, `f64.max` and the `f32` forms; assert the bytes equal native for NaN, both zeros and both infinities
- [ ] GPU (`osprey-codegen/src/gpu_device/numeric.rs`): a kernel may call or name both. WGSL `min` and `max` leave NaN undefined, so the device form selects explicitly and the device result equals the host's ([BUILTIN-MIN-MAX-VALUE])
- [ ] Python frontend: `min(a, b)` and `max(a, b)` resolve to the builtins unless shadowed; one argument, three arguments, `key=` and `default=` are rejected with a diagnostic naming the two-argument form

### Tests

Expand existing suites. Add no new test file.

- [ ] Numeric corpus in both flavors: every integer type at its limits (`int8` `-128` and `127`, `uint8` `0` and `255`, `int` minimum and maximum), equal arguments, both argument orders, each asserted with `expect`
- [ ] Float table from [BUILTIN-MIN-MAX-FLOAT] for `float`, `float32` and `float16`: NaN on either side, `-0.0` with `0.0` in both orders checked through `floatBits`, both infinities
- [ ] Unsigned order: `max` of `uint8` `200` and `100` is `200`, where a signed compare would answer `100`
- [ ] Function values: `foldList(xs, first, max)`, an alias, a stored and a returned `min`, a callback in `mapList`
- [ ] No `Arith` handler is needed anywhere `min` or `max` is the only arithmetic, proved by a program with no handler
- [ ] Shadowing: a top-level `fn min`, a module member `max` and an `extern` each win over the builtin
- [ ] `kernel_frontier` twins: `gpuMap` and `gpuFold` with `min` and `max`, host and device, NaN included
- [ ] `src/osprey/examples/failscompilation/`: `int` with `float`, two integer widths, a `string`, a `Result`, one argument, three arguments
- [ ] Corpus green under GC, ARC and wasm32

### Use it

Each hand-written copy is deleted in the same change that lands the builtin.

- [ ] `src/osprey/examples/tui/api_browser.osp`: delete `fn min`
- [ ] `src/osprey/packages/synth/src/exciters.ospml`: delete the local `min`
- [ ] `src/osprey/examples/statefulhttp/tui.osp` and `src/osprey/packages/manager/core/src/arithmetic.osp`: write `clamp` as `max(low, min(x, high))`
- [ ] `src/osprey/examples/cleanarch_todo/test/cases/multishot_matrix_cases.osp`: write `mxClamp` with the builtins
- [ ] `src/osprey/packages/numpy/src/ndarray.py`: `np.min` and `np.max` stay as shadowing module members; their element reductions use the builtins
- [ ] Sweep `src/osprey/` for `if a < b { a } else { b }` and its `match` form; replace each with the builtin

## Done when

- Every box above is ticked and no spec carries a "not implemented" status for either construct.
- The corpus, the multi-shot suites and the Clean Architecture sample pass under GC, ARC and wasm32 with identical output.
- `make ci` is green and `src/osprey/tests/coverage-thresholds.json` has not gone down.
