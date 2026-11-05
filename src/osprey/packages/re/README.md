# @osprey/re

An Osprey Python-flavor port of re: regular-expression parsing, backtracking matching, capture groups and Unicode classification.

Inputs and results are Osprey values. Error positions use code point indices; supported syntax is defined by the parser and oracle cases.

The [Python-flavor ports contract](../../../../docs/specs/ecosystem/0043-PythonFlavorPorts.md) defines [PORT-SUITE] and [PORT-TEST-ORACLE]. The [CPython oracle](oracle/re.cpython) computes the reference values in `tests/expected.py`. `src/` is Osprey source, not CPython; run it with the Osprey compiler.

```sh
target/release/osprey src/osprey/packages/re/tests --run
target/release/osprey src/osprey/packages/re/tests --run --memory=gc
OSPREY_ARC_DEBUG=1 target/release/osprey src/osprey/packages/re/tests --run --memory=arc
```

The Rust integration harness discovers `tests/osprey.toml`, compares stdout with `tests/expected_output.txt` under all three memory backends, and requires ARC to exit without live objects. Presence of a suite does not establish that it passes; compiler diagnostics, assertion failures and missing golden files remain failures.

## Origin, license and thanks

**Original project:** [CPython's `re` module](https://github.com/python/cpython).

**How it was ported.** Osprey's Python flavor reads like Python, so this port keeps CPython's `re` module's module, function and argument names wherever it can, and its tests compare results with CPython's `re` module 3.12.13 running under CPython. That keeps it closer to the original than a port to a different-looking language usually is. It is still a reimplementation: the code is written for Osprey's types, effects and runtime, and CPython's `re` module is not bundled, linked or needed at run time.

**License.** CPython's `re` module is PSF-2.0 licensed. This package is MIT, Copyright NIMBLESITE PTY LTD, Australia; that grant covers Osprey's own code and does not relicense anything of CPython's `re` module's. [NOTICE.md](NOTICE.md) lists any third-party material the package carries and the terms that come with it (copies in [LICENSES/](LICENSES/)).

**Thanks** to the CPython core developers and Fredrik Lundh, author of the SRE engine behind `re`, for [CPython's `re` module](https://github.com/python/cpython), and to the Unicode Consortium, whose character data the tables in NOTICE.md derive from. Naming CPython's `re` module credits it; it does not imply that its maintainers endorse this port.
