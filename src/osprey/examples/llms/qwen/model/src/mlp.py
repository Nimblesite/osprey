# osprey: flavor=python
# Adapted from Hugging Face Transformers' Qwen3.5 model, Copyright 2025 The Qwen Team and The HuggingFace Inc. team, Apache-2.0; translated and changed for Osprey. Terms: ../NOTICE.md.
# osprey: namespace=qwen
# osprey: module=Qwen3_5Mlp
"""`Qwen3_5MLP`: `down_proj(silu(gate_proj(x)) * up_proj(x))` for stacked rows.

Implements [LLM-MODEL-ATTENTION], [LLM-MODEL-DELTANET] (every layer ends in one).
"""

from qwen.Qwen3_5Device import Weight, at, linear, must, sigmoid

__all__ = ["Mlp", "swiglu"]


class Mlp:
    """The three projections of one layer's MLP."""
    gate: Weight
    up: Weight
    down: Weight


def swiglu(x, mlp, config, rows) -> GpuBuffer[float32]:
    """`Qwen3_5MLP.forward` over the `rows.count` hidden rows of `x`, each weight read once."""
    hidden = config.hidden_size
    inner = config.intermediate_size
    count = rows.count
    g = linear(x, mlp.gate, count, hidden, inner)
    u = linear(x, mlp.up, count, hidden, inner)
    s = sigmoid(g)
    h = must(gpuMap(rows.intermediate, lambda i: at(g, i) * at(s, i) * at(u, i)))
    return linear(h, mlp.down, count, inner, hidden)
