# osprey: flavor=python
# osprey: namespace=transformers
# osprey: module=Qwen3_5Mlp
"""`Qwen3_5MLP`: the SwiGLU feed-forward block every decoder layer ends with.

`Qwen3_5MLP.forward` is `down_proj(silu(gate_proj(x)) * up_proj(x))`: one gate
projection, one up projection, their element-wise product after SiLU on the gate half,
then the down projection back to `hidden_size`. The activations are binary32 and
`sigmoid` is written out as `1 / (1 + exp(-z))`, which is what `ACT2FN["silu"]` computes
for a float32 tensor. Implements [LLM-MODEL-ATTENTION]'s feed-forward half.
"""

__all__ = ["Weights", "silu", "swiglu", "projections", "activated"]

from transformers.Qwen3_5Tensor import Matrix, at, linear, matrix, then


class Weights:
    """The block's three weight matrices."""
    gate: Matrix
    up: Matrix
    down: Matrix


def silu(z) -> float32:
    """`z * sigmoid(z)` at binary32."""
    return z * (toFloat32(1.0) / (toFloat32(1.0) + exp(toFloat32(0.0) - z)))


def swiglu(rows, params, config, index):
    """The block's output for every row of a batch: `down_proj(silu(gate) * up)`."""
    return then(projections(params, config, index), lambda weights: Success(value=mapList(rows, lambda row: linear(activated(row, weights), weights.down))))


def projections(params, config, index):
    """Layer `index`'s `gate_proj`, `up_proj` and `down_proj`, shape-checked."""
    prefix = "layers." + toString(index) + ".mlp."
    return then(matrix(params, prefix + "gate_proj.weight", config.intermediate_size, config.hidden_size), lambda gate: then(matrix(params, prefix + "up_proj.weight", config.intermediate_size, config.hidden_size), lambda up: then(matrix(params, prefix + "down_proj.weight", config.hidden_size, config.intermediate_size), lambda down: Success(value=Weights(gate=gate, up=up, down=down)))))


def activated(row, weights):
    """`silu(gate_proj(x)) * up_proj(x)`, the block's element-wise product."""
    gated = linear(row, weights.gate)
    upper = linear(row, weights.up)
    return toList(map(range(len(gated)), lambda i: silu(at(gated, i)) * at(upper, i)))
