# @osprey/torch

An Osprey Python-flavor port of torch: float64 tensor operations, autograd, optimizers and learning-rate schedules.

Tensors use the Osprey NumPy storage capability. This is not a binding to the PyTorch runtime or its complete dtype/operator catalogue.

Autograd uses an explicit immutable tape: operations return the grown tape and a node ID, and `backward` accumulates gradients over that graph. GELU uses the erf formula corresponding to PyTorch's default `approximate="none"` behavior. Gradient-norm clipping implements L2 (`norm_type = 2.0`) and infinity norms. Other orders fail through the NumPy error effect instead of silently using L2.

The [Python-flavor ports contract](../../docs/specs/ecosystem/0043-PythonFlavorPorts.md) defines [PORT-SUITE] and [PORT-TEST-ORACLE]. The [CPython oracle](../../scripts/oracles/torch.py) computes the reference values in `tests/expected.py`. `src/` is Osprey source, not CPython; run it with the Osprey compiler.

```sh
target/release/osprey packages/torch/tests --run
target/release/osprey packages/torch/tests --run --memory=gc
OSPREY_ARC_DEBUG=1 target/release/osprey packages/torch/tests --run --memory=arc
```

The Rust integration harness discovers `tests/osprey.toml`, compares stdout with `tests/expected_output.txt` under all three memory backends, and requires ARC to exit without live objects. Full runtime verification of the current port is still pending; compiler diagnostics, assertion failures and missing golden files remain failures.
