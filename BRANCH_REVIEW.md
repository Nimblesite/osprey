# Branch re-audit

## Scope

- Compared `origin/main` (`713366f9e9e6`) with the current `resume` branch (`f13d2efdbc23`) plus the staged changes present on 2026-08-21.
- Static inspection only. No tests, builds, linters, formatters, generators, or executables were run.
- The staged changes are part of this review because they materially change the proposed branch and fix several defects that still exist at `HEAD` alone.

## Summary

The branch is not ready to merge. The re-audit found three production regressions and one test-infrastructure failure. Test coverage is also materially weaker in the ML twins of six expanded suites. The earlier collection-owner findings have been cleared by the staged fixes and are not repeated below.

## Findings

### 1. High — seven test sources cannot match their checked-in TAP goldens

The corpus runner executes every `.test.osp` and `.test.ospml`, and an ML twin falls back to the Default golden (`crates/run_test_corpus.sh:126-143`). The current registration counts and checked-in TAP plans disagree:

| Suite | Default registrations | ML registrations | Golden plan |
| --- | ---: | ---: | ---: |
| `tests/core/collections/list_basics` | 11 | 5 | 11 |
| `tests/effects/errors/direct_recovery` | 16 | 10 | 16 |
| `tests/effects/resume/resume_lifo_audit` | 6 | 1 | 1 |
| `tests/effects/resume/resume_value_rewrite` | 8 | 1 | 8 |
| `tests/framework/verdict.test.ospml` | — | 10 | 4 |
| `tests/regressions/effects/fiber_effects` | 7 | 1 | 7 |
| `tests/regressions/effects/retry_until_valid` | 7 | 1 | 7 |

Examples:

- `resume_lifo_audit.test.osp` registers six tests through line 880, while `resume_lifo_audit.test.osp.expectedoutput:2-3` still says `1..1` and `tests=1`.
- `verdict.test.ospml` registers ten tests through line 997, while `verdict.test.ospml.expectedoutput:5-6` says `1..4` and `tests=4`.
- The staged golden updates make the expanded Default versions of `list_basics`, `direct_recovery`, `resume_value_rewrite`, `fiber_effects`, and `retry_until_valid` internally consistent, but their smaller ML twins share those same larger goldens.

These are deterministic golden mismatches, independent of whether the individual assertions pass. The corpus gate will reject the affected flavor for each row.

### 2. High — test weakening: six expanded Default suites omit the equivalent ML assertions

The branch adds substantial regression coverage only to the Default side of these twin pairs:

- `list_basics`: 11 Default registrations versus 5 ML registrations.
- `direct_recovery`: 16 versus 10.
- `resume_lifo_audit`: 6 versus 1.
- `resume_value_rewrite`: 8 versus 1.
- `fiber_effects`: 7 versus 1.
- `retry_until_valid`: 7 versus 1.

This is a material coverage weakening for ML. The changed production code is shared after parsing, while the missing ML scenarios include currying, effect handling, collection element recovery, fiber interaction, and retry behavior. The repository explicitly uses one shared golden to prove that twin flavors run byte-identically (`crates/run_test_corpus.sh:129-132`), but the branch no longer supplies equivalent programs or assertion sets.

No pre-existing assertion deletion was found in the reviewed diffs. The weakening is the omission of the newly claimed regression cases from one required flavor, not the removal of an old assertion.

### 3. High — an unrelated ML parameter name can break builtin calls elsewhere in the program

`crates/osprey-syntax/src/ml/lower.rs:113-121` now adds every binding parameter to the process-wide `BOUND_NAMES` set. `lower_application` then consults that global set at lines 1394-1404 to decide whether *every* identifier with that spelling is a curried user callable.

The set is not lexical. A parameter named after a multi-argument builtin therefore changes unrelated calls outside the parameter's scope. For example:

```text
unused contains = 0
answer = contains "alpha" "ph"
```

Merely declaring the unused parameter `contains` causes the second line to lower as nested calls equivalent to `contains("alpha")("ph")`, rather than the required flat builtin call `contains("alpha", "ph")`. The builtin then receives the wrong arity. The same collision applies to names such as `mapSet`, `listGet`, `checkAll`, and extern functions.

Parameter callability must be tracked in the lexical lowering context of the expression being lowered; adding parameter names to a program-global spelling set is not scope-safe.

### 4. High — generic file-scope function values read by functions still cannot be initialized

The new global-storage pass creates a module slot whenever a function reads a file-scope binding (`crates/osprey-codegen/src/globals.rs:54-84`). However, `gen_bind` deliberately leaves generic lambdas inline-only and generic named functions as call aliases without binding a runtime value (`crates/osprey-codegen/src/stmt.rs:209-245`). `publish_binding` subsequently requires `cg.lookup(name)` and returns an unknown-name error when no value was materialized (`stmt.rs:86-98`).

A valid shape such as this therefore reaches code generation but cannot publish `alias`:

```text
fn identity(x) = x
let alias = identity
fn useAlias() = alias(1)
print("${useAlias()}")
```

The same failure applies to a generic file-scope lambda read from a function. The staged `identifier_fn_type` changes recover an alias's function type, but they do not create a value for the global or change the failing publication path.

### 5. Medium — mixed handlers grow the native stack once per non-resuming perform

The per-arm handler fix correctly distinguishes substituting arms from resuming arms, but its continuation path is recursive. After a non-resuming arm supplies its operation result, `emit_substitute_and_continue` calls the generated drive function again and immediately returns its result (`crates/osprey-codegen/src/effects.rs:1059-1077`).

Consequently, a handler with at least one resuming arm and a different non-resuming arm grows one native dispatcher frame for every perform handled by the non-resuming arm. Release builds may optimize this tail position, but debug builds intentionally compile at `-O0` (`crates/osprey-debug/src/lib.rs:77-80`), where that optimization is not guaranteed. A long loop of otherwise constant-space performs can therefore exhaust the host stack only in mixed-handler mode.

The dispatcher should continue through an explicit loop/back-edge, or emit a guaranteed tail call, rather than relying on an optimizer to remove recursion. The new tests exercise only small fixed numbers of performs and do not cover this resource regression.

## Re-audit disposition of earlier findings

- Nested list/map element owners across parameters, returns, fields, literals, conversions, and reads: fixed by the staged descriptor changes.
- `mapKeys`/`mapValues` result typing: fixed by the staged list owner tags.
- Generic file-scope function values: still open as finding 4.
- TAP output validity: partially repaired, but still open in the broader form described by finding 1.

## Recommendation

Do not merge until all five findings are addressed. In particular, restore Default/ML test parity without deleting registrations or loosening assertions, then regenerate or update goldens from the actual complete output rather than editing TAP counts in isolation.
