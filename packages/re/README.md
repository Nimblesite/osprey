# @osprey/re

An Osprey Python-flavor port of re: regular-expression parsing, backtracking matching, capture groups and Unicode classification.

Inputs and results are Osprey values. Error positions use code point indices; supported syntax is defined by the parser and oracle cases.

The [Python-flavor ports contract](../../docs/specs/ecosystem/0043-PythonFlavorPorts.md) defines [PORT-SUITE] and [PORT-TEST-ORACLE]. The [CPython oracle](../../scripts/oracles/re.py) computes the reference values in `tests/expected.py`. `src/` is Osprey source, not CPython; run it with the Osprey compiler.

```sh
target/release/osprey packages/re/tests --run
target/release/osprey packages/re/tests --run --memory=gc
OSPREY_ARC_DEBUG=1 target/release/osprey packages/re/tests --run --memory=arc
```

The Rust integration harness discovers `tests/osprey.toml`, compares stdout with `tests/expected_output.txt` under all three memory backends, and requires ARC to exit without live objects. Presence of a suite does not establish that it passes; compiler diagnostics, assertion failures and missing golden files remain failures.
