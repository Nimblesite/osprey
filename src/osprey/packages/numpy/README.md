# @osprey/numpy

An Osprey Python-flavor port of numpy: row-major float64 arrays, broadcasting, shape operations, reductions and linear algebra.

Storage is selected by an explicit host or GPU handler. Integer, object and complex dtype arrays are outside this port.

The [Python-flavor ports contract](../../../../docs/specs/ecosystem/0043-PythonFlavorPorts.md) defines [PORT-SUITE] and [PORT-TEST-ORACLE]. The [CPython oracle](oracle/numpy.cpython) computes the reference values in `tests/expected.py`. `src/` is Osprey source, not CPython; run it with the Osprey compiler.

```sh
target/release/osprey src/osprey/packages/numpy/tests --run
target/release/osprey src/osprey/packages/numpy/tests --run --memory=gc
OSPREY_ARC_DEBUG=1 target/release/osprey src/osprey/packages/numpy/tests --run --memory=arc
```

The Rust integration harness discovers `tests/osprey.toml`, compares stdout with `tests/expected_output.txt` under all three memory backends, and requires ARC to exit without live objects. Presence of a suite does not establish that it passes; compiler diagnostics, assertion failures and missing golden files remain failures.

## Origin, license and thanks

**Original project:** [NumPy](https://github.com/numpy/numpy).

**How it was ported.** Osprey's Python flavor reads like Python, so this port keeps NumPy's module, function and argument names wherever it can, and its tests compare results with NumPy 2.5.4 running under CPython. That keeps it closer to the original than a port to a different-looking language usually is. It is still a reimplementation: the code is written for Osprey's types, effects and runtime, and NumPy is not bundled, linked or needed at run time.

**License.** NumPy is BSD-3-Clause licensed. This package is MIT, Copyright NIMBLESITE PTY LTD, Australia; that grant covers Osprey's own code and does not relicense anything of NumPy's. [NOTICE.md](NOTICE.md) lists any third-party material the package carries and the terms that come with it.

**Thanks** to the NumPy developers for [NumPy](https://github.com/numpy/numpy). Naming NumPy credits it; it does not imply that its maintainers endorse this port.
