# osprey: flavor=python
# osprey: namespace=transformers
# osprey: module=Qwen3_5Mlp
"""`Qwen3_5MLP`: `down_proj(silu(gate_proj(x)) * up_proj(x))` for one token.

Implements [LLM-MODEL-ATTENTION], [LLM-MODEL-DELTANET] (every layer ends in one).
"""

from transformers.Qwen3_5Device import Weight, at, linear, must, sigmoid

__all__ = ["Mlp", "swiglu"]


class Mlp:
    """The three projections of one layer's MLP."""
    gate: Weight
    up: Weight
    down: Weight


def swiglu(x, mlp, config, shapes) -> GpuBuffer[float32]:
    """`Qwen3_5MLP.forward` over the one hidden row `x`."""
    hidden = config.hidden_size
    inner = config.intermediate_size
    g = linear(x, mlp.gate, 1, hidden, inner)
    u = linear(x, mlp.up, 1, hidden, inner)
    s = sigmoid(g)
    h = must(gpuMap(shapes.intermediate, lambda i: at(g, i) * at(s, i) * at(u, i)))
    return linear(h, mlp.down, 1, inner, hidden)
