# Osprey language test corpus

This directory contains executable, assertion-driven tests for core Osprey
language behavior. Unlike `examples/tested`, these files are not stdout golden
examples: `osprey test` runs every `*.test.osp` and `*.test.ospml` file and the
assertions inspect values produced inside the program.

- `core/default/` exercises the Default flavor.
- `core/ml/` exercises the ML flavor and its pure `Verdict` surface.
- `interactions/` contains paired, real-world scenarios that deliberately mix
  records, unions, results, collections, lambdas, pipes, matching, mutation,
  and string operations.

Each named test should model several interactions and make several assertions.
Prefer asserting derived state and behavior over adding one-feature smoke tests.

Run the complete corpus with:

```sh
make language-test
# or, after building the compiler:
target/release/osprey test tests
```
