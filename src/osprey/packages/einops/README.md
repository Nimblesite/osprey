# @osprey/einops

An Osprey Python-flavor port of einops: pattern parsing, transformation recipes and concrete shape reconstruction.

It operates on shapes and recipes; it does not expose the upstream tensor-backend execution API.

The [Python-flavor ports contract](../../../../docs/specs/ecosystem/0043-PythonFlavorPorts.md) defines [PORT-SUITE] and [PORT-TEST-ORACLE]. The [CPython oracle](oracle/einops.cpython) computes the reference values in `tests/expected.py`. `src/` is Osprey source, not CPython; run it with the Osprey compiler.

```sh
target/release/osprey src/osprey/packages/einops/tests --run --memory=gc
OSPREY_ARC_DEBUG=1 target/release/osprey src/osprey/packages/einops/tests --run --memory=arc
```

The Rust integration harness discovers `tests/osprey.toml`, compares stdout with `tests/expected_output.txt` under GC and ARC, and requires ARC to exit without live objects. Presence of a suite does not establish that it passes; compiler diagnostics, assertion failures and missing golden files remain failures.

The suite checks 258 assertions, including 155 additional upstream-derived parser, recipe and shape reconstruction cases: empty/unit groups, ellipsis placement, axis permutations, every reduction operation, repeat sizes, zero shapes and invalid inputs. Both backends match the golden exactly; measured own-source coverage is 432/448 lines (96.4%). Reference values and rejection messages come from einops 0.8.2, not the port.

This remains a shape/recipe subset. Unicode axis identifiers accepted by Python are not supported by the current ASCII identifier scanner; upstream tensor execution and full upstream-suite parity are unfinished. Internal recipe ordering for multiple added repeat axes varies with upstream hash randomization, so the fixed reference corpus does not yet cover that ordering family.
