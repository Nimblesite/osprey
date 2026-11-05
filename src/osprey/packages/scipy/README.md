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

## Origin, license and thanks

**Original project:** [SciPy](https://github.com/scipy/scipy).

**How it was ported.** Osprey's Python flavor reads like Python, so this port keeps SciPy's module, function and argument names wherever it can, and its tests compare results with SciPy 1.18.1 running under CPython. That keeps it closer to the original than a port to a different-looking language usually is. It is still a reimplementation: apart from the Cephes, xsf and Boost numerical kernels and coefficients that NOTICE.md lists, the code is written for Osprey's types, effects and runtime, and SciPy is not bundled, linked or needed at run time.

**License.** SciPy is BSD-3-Clause licensed. This package is MIT, Copyright NIMBLESITE PTY LTD, Australia; that grant covers Osprey's own code and does not relicense anything of SciPy's. [NOTICE.md](NOTICE.md) lists any third-party material the package carries and the terms that come with it (copies in [LICENSES/](LICENSES/)).

**Thanks** to the SciPy developers for [SciPy](https://github.com/scipy/scipy), to Stephen L. Moshier for Cephes and the [xsf](https://github.com/scipy/xsf) maintainers, and to John Maddock and the Boost authors. Naming SciPy credits it; it does not imply that its maintainers endorse this port.
