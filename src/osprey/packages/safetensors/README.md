# @osprey/safetensors

An Osprey Python-flavor port of safetensors: safetensors headers, tensor metadata, byte decoding and serialization.

The public interface uses Osprey values and explicit file capabilities rather than Python tensor objects.

The [Python-flavor ports contract](../../../../docs/specs/ecosystem/0043-PythonFlavorPorts.md) defines [PORT-SUITE] and [PORT-TEST-ORACLE]. The [CPython oracle](oracle/safetensors.cpython) computes the reference values in `tests/expected.py`. `src/` is Osprey source, not CPython; run it with the Osprey compiler.

```sh
target/release/osprey src/osprey/packages/safetensors/tests --run
target/release/osprey src/osprey/packages/safetensors/tests --run --memory=gc
OSPREY_ARC_DEBUG=1 target/release/osprey src/osprey/packages/safetensors/tests --run --memory=arc
```

The Rust integration harness discovers `tests/osprey.toml`, compares stdout with `tests/expected_output.txt` under all three memory backends, and requires ARC to exit without live objects. Presence of a suite does not establish that it passes; compiler diagnostics, assertion failures and missing golden files remain failures.

## Origin, license and thanks

**Original project:** [safetensors](https://github.com/safetensors/safetensors).

**How it was ported.** Osprey's Python flavor reads like Python, so this port keeps safetensors's module, function and argument names wherever it can, and its tests compare results with safetensors 0.8.0 running under CPython. That keeps it closer to the original than a port to a different-looking language usually is. It is still a reimplementation: the code is written for Osprey's types, effects and runtime, and safetensors is not bundled, linked or needed at run time.

**License.** safetensors is Apache-2.0 licensed. This package is MIT, Copyright NIMBLESITE PTY LTD, Australia; that grant covers Osprey's own code and does not relicense anything of safetensors's. [NOTICE.md](NOTICE.md) lists any third-party material the package carries and the terms that come with it.

**Thanks** to Hugging Face and the safetensors contributors for [safetensors](https://github.com/safetensors/safetensors). Naming safetensors credits it; it does not imply that its maintainers endorse this port.
