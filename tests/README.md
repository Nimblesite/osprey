# Osprey language test corpus

This directory contains executable, assertion-driven tests for core Osprey
language behavior. `osprey test` runs every `*.test.osp` and `*.test.ospml` file and the
assertions inspect values produced inside the program.

- `core/` groups foundational arithmetic, collections, strings, types, and
  mixed-feature behavior by language concept.
- `flavors/` keeps Default/ML twins side by side and proves both spellings over
  the same dense behavior tables.
- `effects/` groups handler, resume, and cross-effect interactions.
- `workflows/` contains real-world scenarios that deliberately combine several
  language features.
- `framework/` exercises the test framework's own special behavior.

Each named test should model several interactions and make several assertions.
Prefer asserting derived state and behavior over adding one-feature smoke tests.
Every type, effect, helper, case, and assertion batch must have a one-line
description: this corpus is executable language documentation.

Use grouped assertions to keep dense suites readable while retaining one soft
assertion result per condition:

```osprey-ml
checkAll "order state" [
    total == 42,
    itemCount == 3,
    paid
]
```

ML twins use the current compact surface: whitespace currying, adjacent
equation clauses for parameter matches, inline unions and positional payloads,
and `?:` for simple `Result` fallbacks. Integer `+`, `-`, and `*` return checked
`Result` values; preserve that channel or handle it explicitly with `?:` or a
`match` before a plain value is required.

`expectAll([condition, ...])` is the unlabeled equivalent. Both require a
non-empty list literal, evaluate every condition, and continue after failures.

Run the complete corpus with:

```sh
make _language-test
# or, after building the compiler:
target/release/osprey test tests
```

Suites run concurrently by default, while their TAP output is replayed in
sorted file order. Set `OSPREY_TEST_JOBS` to a positive worker limit, or use
`OSPREY_TEST_JOBS=1` when a constrained or diagnostic run must be serial:

```sh
OSPREY_TEST_JOBS=1 target/release/osprey test tests
```

CI also runs every suite under the tracing GC and ARC memory backends, so moving
an assertion suite here does not reduce its backend-conformance coverage.
