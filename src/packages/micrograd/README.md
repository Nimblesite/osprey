# @osprey/micrograd

A port of [karpathy/micrograd](https://github.com/karpathy/micrograd) (PyPI 0.1.0, MIT), plus the `tanh`/`exp` ops Karpathy adds in the nn-zero-to-hero micrograd lecture, to Osprey's Python flavor.

micrograd's `Value` is a mutable graph of `grad` fields and `_backward` closures. Osprey values are immutable, so the port keeps the same semantics in one immutable graph **arena**: a `Graph` record holds every node ever built, in creation order, and a node's id is its index. Every operation appends its nodes exactly as micrograd's operators construct `Value`s — including the leaf a bare Python number is wrapped into — and returns the extended graph. `backward` computes the gradient of every node using micrograd's own depth-first topological order and chain-rule accumulation, so float64 gradients match micrograd bit for bit (compared at rtol 1e-12, 1e-9 for `tanh`/`exp` cases).

## Engine (`micrograd.Engine`)

| Function | Original | Notes |
| --- | --- | --- |
| `value(g, x)` | `Value(x)` | A leaf node wrapping a constant or input |
| `add(g, a, b)` / `add_const(g, a, k)` | `a + b` / `a + k` | `add_const` wraps the bare number in a leaf first, as `__add__` does |
| `mul(g, a, b)` / `mul_const(g, a, k)` | `a * b` / `a * k` | Same leaf-wrapping rule |
| `pow_const(g, a, n)` | `a ** n` | One `**` node; integer exponent `n` only (see divergences) |
| `neg(g, a)` | `-a` | `a * -1`: a leaf and a `*` node |
| `sub(g, a, b)` / `sub_const(g, a, k)` / `rsub_const(g, a, k)` | `a - b` / `a - k` / `k - a` | `sub` is `a + (-b)`; `sub_const` negates the number itself (two nodes) |
| `truediv(g, a, b)` / `truediv_const(g, a, k)` / `rtruediv_const(g, a, k)` | `a / b` / `a / k` / `k / a` | `a / b` is `a * b**-1`; the const forms precompute `1/k` on the number |
| `relu(g, a)` | `a.relu()` | `0` below zero, else the value |
| `tanh(g, a)` / `exp(g, a)` | `a.tanh()` / `a.exp()` (lecture) | `tanh` is `(e^2x - 1) / (e^2x + 1)` |
| `backward(g, root)` | `Value.backward()` after `zero_grad` | Fresh zero gradients, root gradient 1.0, then reversed topo order |
| `backward_into(g, root, grads)` | `Value.backward()` without `zero_grad` | Accumulates into `grads`; calling it again doubles gradients, exactly as micrograd |
| `zero_grad(g)` | `Module.zero_grad()` | A fresh zero gradient per node (the state `backward` starts from) |
| `ipow(x, n)` | `x ** n` | Repeated multiplication; integer exponents only, as micrograd's suite uses; `pow` is a builtin since the math builtins landed |
| `empty`, `size`, `top`, `node`, `data` | — | Arena accessors; `top` is the latest node's id |

## NN (`micrograd.NN`)

| Function | Original | Notes |
| --- | --- | --- |
| `init_mlp(g, nin, nouts, params)` | `MLP(nin, nouts)` construction | Appends parameter leaves in `parameters()` order, taking data from `params` |
| `mlp_of(nin, nouts)` | `MLP`'s layer/neuron records | Parameter ids over the graph `init_mlp` builds |
| `call_neuron(g, n, xs)` / `call_neuron_values(g, n, x_ids)` | `Neuron.__call__` | `sum(wi*xi, b)` then `relu` when nonlinear |
| `call_layer(g, l, xs)` / `call_layer_values(g, l, x_ids)` | `Layer.__call__` | Returns a `Forward(graph, outputs)` (see divergences) |
| `call_mlp(g, m, xs)` | `MLP.__call__` | Layer 0 over the inputs, each later layer over the previous outputs |
| `parameters(m)` | `MLP.parameters()` | Weights then bias, per neuron, per layer — ids 0..`param_count`-1 on a fresh graph |
| `sgd_step(params, grads, ids, lr)` | `p.data += -lr * p.grad` | Returns the new parameter list instead of mutating |

The oracle in `scripts/oracles/micrograd.py` installs real micrograd into the CPython venv, seeds `random.seed(1337)`, and writes `tests/expected.py` from real micrograd + torch. The suite covers micrograd's own `test_engine.py` cases (`sanity_check`, `more_ops`), an `MLP(3, [4, 4, 1])` forward/backward, 20 README-demo SGD steps, a 12-step tanh network, and a double-backward case.

## Divergences

- **Graph arena instead of mutable `Value`s.** Node ids stand in for object references; `backward` returns the gradients of every node as a list.
- **`call_layer`/`call_layer_values` return `Forward(graph, outputs)`** rather than a list of `Value`s, because output ids cannot be recovered from graph position once a layer has several neurons.
- **`**` supports only integer exponents (and `-1`)** by repeated multiplication; micrograd uses Python's `**`. Float exponents can now use the `pow` builtin and are left for a later release.
- **Deterministic traversal.** micrograd's `build_topo` walks a Python `set` whose order follows object hashes and varies run to run; the port (and the oracle) walk the deduplicated operands in operand order, so every gradient is reproducible while keeping micrograd's algorithm.
- **`sgd_step` is functional** (returns new parameters) rather than mutating `p.data`.

## Compiler workarounds

- `sgd_step`'s parameters are annotated (`list[float]`, `list[int]`, `float`) to dodge an inference bug where a call through a `..`-prefixed source root unifies the `ids` list with `float` (see the report's Compiler bugs section).
