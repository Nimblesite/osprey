# Review: `tuples` branch

**Base:** `main` (`4019dcf1`)  
**Head:** `tuples` (`6b8a2840`)  
**Verdict:** Changes requested — three material regressions remain.

This review is limited to regressions introduced by the branch. It does not list style nits or pre-existing defects.

## Findings

### P1 — A list round-trip destroys the new `any` representation

**Locations:** `crates/osprey-codegen/src/listlit.rs:115-149`, `crates/osprey-codegen/src/collections.rs:180-212`

`LType::Any` is a managed `i8*` box on this branch, but the flat-list lowering maps every element type other than `Double`, `I1`, `Str`, or `Ptr` to `I64`. It therefore converts an `Any` box pointer back to an integer word, tags the list as an integer list, and marks its elements unmanaged. Reading the element later produces an `I64`, so rendering prints the box address instead of dispatching through its descriptor.

Minimal reproducer:

```osprey
fn erased() -> any = 42
let xs = [erased()]
match xs {
    [x] => print("x=${x}")
    _ => print("miss")
}
```

The direct control case `print("x=${erased()}")` prints `x=42`. After the list round-trip, the probe printed `x=4312882896` under the default backend and another address-like integer under ARC. On `main`, `any` is still represented by the original word, so this scalar case stores and reads `42`; the wrong output is introduced by the branch's new pointer representation.

The runtime-container path has the same incomplete classification: `collections::managed_flag` excludes `Any`. `listAppend(List(), erased())` renders `42`, but its ARC run exits with one live 16-byte object because the list never releases the boxed element.

**Recommendation:** Preserve `LType::Any` in flat-list element selection, element owner tags, literal-to-runtime conversion, and collection managed flags. Add scalar and heap-valued `any` round-trips through list literals and runtime list/map operations, with exact output checks on every backend and a zero-live-object ARC oracle.

### P1 — Reassigning captured `mut any` state leaks the previous box under ARC

**Location:** `crates/osprey-codegen/src/lower.rs:444-462`

`gen_cell_store` retains the incoming value, but releases the outgoing cell value only for `Str | Ptr`. `Any` is now managed too, so every reassignment of an `Any`-typed cell leaves the old box alive. If its payload is managed, that payload remains alive with it.

The following accepted program replaces a box containing a runtime-built string with a boxed integer:

```osprey
effect Put { put: fn(any) -> Unit }

fn erased() -> any = "old" + "box"
fn emit() -> Unit !Put = perform Put.put(42)

fn run() -> int = {
    mut slot = erased()
    handle Put
        put item => { slot = item }
    in emit()
    0
}

print("${run()}")
```

With `OSPREY_ARC_DEBUG=1 --memory=arc`, it exits successfully but reports two live objects: the old 16-byte `any` box and its 7-byte string payload. Repeated handler operations make the leak unbounded.

**Recommendation:** Treat `LType::Any` as managed in the cell replacement's release path. Audit the remaining hand-written `Str | Ptr` ownership checks for the same assumption, and add a captured-mutable replacement case whose ARC sentinel must be zero.

### P1 — Default-flavor calls became whitespace-sensitive

**Location:** `tree-sitter-osprey/grammar.js:442-461`

Changing the call suffix from `'('` to `token.immediate('(')` rejects valid Default-flavor source whenever formatting leaves whitespace between a callee and its argument list. `main` accepts this syntax, and this branch already had to rewrite two existing calls in `string_pipeline.test.osp` to keep the corpus green.

```osprey
fn id(x) = x
print(id (1))
```

The branch rejects line 2 with `syntax error near "id"`; changing only `id (1)` to `id(1)` passes. This is a broad source-compatibility regression unrelated to whether the call appears beside a tuple-pattern arm.

**Recommendation:** Resolve the match-arm/tuple ambiguity in the match grammar or with precedence/conflict handling while retaining ordinary whitespace before a call's `(`. Add parser coverage for both `f(args)` and `f (args)`, including a match whose following arm starts with a tuple pattern.

## GitHub issues clearly fixed

These issues are still open on GitHub, but the branch clearly resolves their reported behavior:

- [#175 — cannot destructure a single-variant union](https://github.com/Nimblesite/osprey/issues/175): `union_owner` now routes single-variant record-like constructors through the destructuring backend. The issue's named-payload shape compiles and runs (`7`), and `single_variant_record_destructures_in_match` passes.
- [#208 — ARC frees a heap value returned as `any`](https://github.com/Nimblesite/osprey/issues/208): `any` now has a distinct managed boxed representation. The formerly unsafe `any -> string` recovery is rejected during checking, and erasing/forwarding values no longer relies on the ambiguous integer-word ownership rule.
- [#209 — `let` recovery from `any` prints a pointer](https://github.com/Nimblesite/osprey/issues/209): recovery through both return and `let` annotations is now rejected with `cannot recover ... from an erased any`, which is one of the issue's stated acceptable outcomes. Both flavors have negative fixtures.

## Verification performed

- `cargo fmt --all --check`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- Rust workspace tests excluding the environment-dependent CLI WASM smoke test: passed.
- CLI tests with `wasm::tests::build_and_run_end_to_end_when_toolchain_present` skipped: passed. The unskipped test fails because the installed Node is 22.22.2 and the test requires Node 24+, not because of this branch.
- `make language-test`: 179 suites passed, 0 failed.
- Targeted runtime probes reproduced every finding above.
