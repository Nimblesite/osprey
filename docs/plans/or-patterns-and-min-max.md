# Or-patterns and `min` / `max`

Two language gaps close here: a `match` giving several patterns one body ([PATTERN-OR](../specs/language/0007-PatternMatching.md#or-patterns--pattern-or)) and the `min` and `max` builtins ([BUILTIN-MIN-MAX](../specs/runtime/0012-Built-InFunctions.md)). Or-patterns are implemented in every flavor, the checker and every backend; `min` and `max` are registered, documented, lowered and covered by the numeric corpus. What remains is the or-pattern tooling, the `min`/`max` diagnostic, folding, `float32`/`float16`, shadowing, GPU, Python and rejection coverage, and the source sweep.

Tick an item only with a corpus regression that passes under GC, ARC and wasm32. Expand existing suites; add no new test file. The rules themselves are [PATTERN-OR-*] in the pattern-matching spec and [BUILTIN-MIN-MAX-*] in the built-ins spec.

## Or-patterns: tooling

- [ ] Formatter (`src/toolchain/crates/osprey-fmt`): one space each side of `|`; an or-pattern too long for the line breaks before each `|`.
- [ ] LSP (`src/toolchain/crates/osprey-lsp`): rename on a binder that appears in every alternative acts on all of its occurrences; the server has no rename request yet (go-to-definition, references and hover cover every alternative and the body). Check: a language-server test applies rename and asserts every occurrence changes.
- [ ] Flavor conversion ([plan 0031](0031-python.md)): an or-pattern converts between all four spellings and back without change. Check: conversion tests in both directions for an arm-level and a nested or-pattern.
- [ ] Symbolic execution (the symbolic bug hunting section of [plan 0045](0045-compile-time-ownership.md#symbolic-bug-hunting)) explores each alternative as its own path. Check: a `src/osprey/tests/hunt/` program with an or-pattern lists one finding path per alternative.
- [ ] Debugger and profiler line tables point at the alternative that matched. Check: a paused step and a profile frame report the matched alternative's line.

## `min` and `max`: specs

- [ ] TypeScript: say whether `Math.min` and `Math.max` with two arguments lower to the builtins, in [FLAVOR-TS-EXPRESSIONS](../specs/language/0054-TypeScriptFlavorSyntax.md).

## `min` and `max`: checker and backends

- [ ] A mixed call is rejected naming the function and both types: "`min` needs two values of one numeric type: got int and float" [BUILTIN-MIN-MAX-TYPE]. Check: `src/osprey/examples/failscompilation/ml_sized_mixed_width.ospo` pins that wording (its golden shows the generic unification error today).
- [ ] Constant folding of two literal arguments, NaN and signed zeros included [ARITH-EFFECT-CONST].
- [ ] GPU (`src/toolchain/crates/osprey-codegen/src/gpu_device/numeric.rs`): a kernel may call or name both; WGSL `min` and `max` leave NaN undefined, so the device form selects explicitly and the device result equals the host's [BUILTIN-MIN-MAX-VALUE].
- [ ] Python frontend: `min(a, b)` and `max(a, b)` resolve to the builtins unless shadowed; one argument, three arguments, `key=` and `default=` are rejected with a diagnostic naming the two-argument form.

## `min` and `max`: tests

- [ ] The `float32` and `float16` rows of the [BUILTIN-MIN-MAX-FLOAT] table in `src/osprey/tests/regressions/basics/math/math_builtins.test.*`: NaN on either side, `-0.0` with `0.0` in both orders through `float32Bits` and `float16Bits`, and both infinities.
- [ ] Function values: an alias, a stored and a returned `min`, and `min` itself as the `mapList` callback (`foldList([4, 9, 2], 0, max)` is covered).
- [ ] Shadowing: a top-level `fn min`, a module member `max` and an `extern` each win over the builtin [BUILTIN-MIN-MAX-SHADOWING].
- [ ] `kernel_frontier` twins: `gpuMap` and `gpuFold` with `min` and `max`, host and device, NaN included.
- [ ] `src/osprey/examples/failscompilation/`: `min` on a `string`, on a `Result`, with one argument and with three arguments (int with float and two integer widths are already rejected by `ml_sized_mixed_width.ospo`).

## `min` and `max`: use it

Each hand-written copy is deleted in the same change that uses the builtin.

- [ ] `src/osprey/examples/statefulhttp/tui.osp` and `src/osprey/packages/manager/core/src/arithmetic.osp`: write `clamp` as `max(low, min(x, high))`.
- [ ] `src/osprey/examples/cleanarch_todo/test/cases/multishot_matrix_cases.osp`: write `mxClamp` with the builtins.
- [ ] `src/osprey/packages/numpy/src/ndarray.py`: `np.min` and `np.max` stay as shadowing module members; their element reductions (`MAX_OP` and `MIN_OP`) use the builtins.
- [ ] Sweep `src/osprey/` for `if a < b { a } else { b }` and its `match` form; replace each with the builtin.

## Gate

- [ ] No spec carries a "not implemented" status for either construct; the corpus, the multi-shot suites and the Clean Architecture sample pass under GC, ARC and wasm32 with identical output; `make ci` is green and `src/osprey/tests/coverage-thresholds.json` has not gone down.
