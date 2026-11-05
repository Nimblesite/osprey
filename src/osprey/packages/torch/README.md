# @osprey/torch

An Osprey Python-flavor port of torch: float64 tensor operations, autograd, optimizers and learning-rate schedules.

Tensors use the Osprey NumPy storage capability. This is not a binding to the PyTorch runtime or its complete dtype/operator catalogue.

Autograd uses an explicit immutable tape: operations return the grown tape and a node ID, and `backward` accumulates gradients over that graph. GELU uses the erf formula corresponding to PyTorch's default `approximate="none"` behavior. Gradient-norm clipping implements L2 (`norm_type = 2.0`) and infinity norms. Other orders fail through the NumPy error effect instead of silently using L2.

The [Python-flavor ports contract](../../../../docs/specs/ecosystem/0043-PythonFlavorPorts.md) defines [PORT-SUITE] and [PORT-TEST-ORACLE]. The [CPython oracle](oracle/torch.cpython) computes the reference values in `tests/expected.py`. `src/` is Osprey source, not CPython; run it with the Osprey compiler.

```sh
target/release/osprey src/osprey/packages/torch/tests --run
target/release/osprey src/osprey/packages/torch/tests --run --memory=gc
OSPREY_ARC_DEBUG=1 target/release/osprey src/osprey/packages/torch/tests --run --memory=arc
```

The Rust integration harness discovers `tests/osprey.toml`, compares stdout with `tests/expected_output.txt` under all three memory backends, and requires ARC to exit without live objects. On 2026-10-25, all 354 checks passed under Default, GC and ARC with byte-identical output and zero live ARC objects. The committed golden was captured from that execution; compiler diagnostics, assertion failures and missing golden files remain failures.

## Origin, license and thanks

**Original project:** [PyTorch](https://github.com/pytorch/pytorch).

**How it was ported.** Osprey's Python flavor reads like Python, so this port keeps PyTorch's module, function and argument names wherever it can, and its tests compare results with PyTorch running under CPython. That keeps it closer to the original than a port to a different-looking language usually is. It is still a reimplementation: the code is written for Osprey's types, effects and runtime, and PyTorch is not bundled, linked or needed at run time.

**License.** PyTorch is BSD-3-Clause licensed. This package is MIT, Copyright NIMBLESITE PTY LTD, Australia; that grant covers Osprey's own code and does not relicense anything of PyTorch's. [NOTICE.md](NOTICE.md) lists any third-party material the package carries and the terms that come with it.

**Thanks** to the PyTorch contributors for [PyTorch](https://github.com/pytorch/pytorch). Naming PyTorch credits it; it does not imply that its maintainers endorse this port.
