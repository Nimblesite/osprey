# @osprey/scipy

An Osprey Python-flavor port of scipy: scalar special functions: gamma, zeta, beta, combinatorics, logistic, information and Box–Cox families.

This package implements the listed special functions; it does not expose SciPy's complete scientific-computing API.

The [Python-flavor ports contract](../../../../docs/specs/ecosystem/0043-PythonFlavorPorts.md) defines [PORT-SUITE] and [PORT-TEST-ORACLE]. The [CPython oracle](oracle/scipy.cpython) computes the reference values in `tests/expected.py`. `src/` is Osprey source, not CPython; run it with the Osprey compiler.

```sh
target/release/osprey src/osprey/packages/scipy/tests --run --memory=gc
OSPREY_ARC_DEBUG=1 target/release/osprey src/osprey/packages/scipy/tests --run --memory=arc
```

The Rust integration harness discovers `tests/osprey.toml`, compares stdout with `tests/expected_output.txt` under GC and ARC, and requires ARC to exit without live objects. Presence of a suite does not establish that it passes; compiler diagnostics, assertion failures and missing golden files remain failures.

The expanded suite has 2,334 upstream-derived assertions, verified under GC and ARC with zero live ARC objects. Exact combinatorics includes exhaustive small `n`/`k` grids and negative/out-of-range inputs; these cases exposed and guard 31 factorial, combination and permutation mismatches. Exact results remain limited to native signed 64-bit integers.
