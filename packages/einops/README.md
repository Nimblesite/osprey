# @osprey/einops

An Osprey Python-flavor port of einops: pattern parsing, transformation recipes and concrete shape reconstruction.

It operates on shapes and recipes; it does not expose the upstream tensor-backend execution API.

The [Python-flavor ports contract](../../docs/specs/ecosystem/0043-PythonFlavorPorts.md) defines [PORT-SUITE] and [PORT-TEST-ORACLE]. The [CPython oracle](../../scripts/oracles/einops.py) computes the reference values in `tests/expected.py`. `src/` is Osprey source, not CPython; run it with the Osprey compiler.

```sh
target/release/osprey packages/einops/tests --run
target/release/osprey packages/einops/tests --run --memory=gc
OSPREY_ARC_DEBUG=1 target/release/osprey packages/einops/tests --run --memory=arc
```

The Rust integration harness discovers `tests/osprey.toml`, compares stdout with `tests/expected_output.txt` under all three memory backends, and requires ARC to exit without live objects. Presence of a suite does not establish that it passes; compiler diagnostics, assertion failures and missing golden files remain failures.
