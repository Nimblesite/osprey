# osprey: flavor=python
"""Check short deterministic sequences at GPT-2's actual tensor dimensions."""
from gpt2 import Backends, Model
from gpt2.Backends import Tensor


def vector(size, scale):
    return Model.or_stop(perform(Tensor.build(size, lambda i: toFloat32(i % 7) * toFloat32(scale))), Model.empty)


def line(name, values):
    size = perform(Tensor.length(values))
    text = join(toList(map(range(size), lambda i: toString(perform(Tensor.get(values, i))))), ",")
    return name + ":" + text + "\n"


def layer():
    return Model.Layer(
        ln1_g=vector(Model.WIDTH, 0.1), ln1_b=vector(Model.WIDTH, -0.05),
        qkv_w=vector(Model.WIDTH * Model.QKV_WIDTH, 0.01), qkv_b=vector(Model.QKV_WIDTH, -0.02),
        proj_w=vector(Model.WIDTH * Model.WIDTH, 0.02), proj_b=vector(Model.WIDTH, 0.03),
        ln2_g=vector(Model.WIDTH, 0.2), ln2_b=vector(Model.WIDTH, -0.01),
        fc_w=vector(Model.WIDTH * Model.MLP_WIDTH, 0.01), fc_b=vector(Model.MLP_WIDTH, 0.02),
        out_w=vector(Model.MLP_WIDTH * Model.WIDTH, -0.01), out_b=vector(Model.WIDTH, 0.01),
    )


def sequence(t):
    x = vector(t * Model.WIDTH, 0.25)
    g = vector(Model.WIDTH, 0.1)
    b = vector(Model.WIDTH, -0.05)
    qkv = vector(t * Model.QKV_WIDTH, 0.125)
    attn = Model.attention(qkv, t)
    return line(f"norm{t}", Model.layer_norm(x, t, g, b)) + line(f"probs{t}", attn.probs) + line(f"attention{t}", attn.out)


def block_lines():
    acts = Model.block(vector(3 * Model.WIDTH, 0.25), 3, layer())
    return line("block.x", acts.x) + line("block.h1", acts.h1) + line("block.qkv", acts.qkv) + line("block.probs", acts.probs) + line("block.a", acts.a) + line("block.x2", acts.x2) + line("block.h2", acts.h2) + line("block.f", acts.f) + line("block.u", acts.u) + line("block.y", acts.y)


def run():
    a = vector(6, 0.25)
    b = vector(6, -0.125)
    bias = vector(2, 0.1)
    gelus = Model.or_stop(perform(Tensor.build(7, lambda i: Model.gelu(toFloat32(i - 3) / 2.0))), Model.empty)
    small = line("transpose", Model.transpose(a, 2, 3)) + line("matmul", Model.product(a, b, 2, 3, 2)) + line("linear", Model.linear(a, 2, 3, 2, b, bias)) + line("residual", Model.plus(a, b)) + line("gelu", gelus)
    print("@device:" + Model.device() + "\n" + small + sequence(1) + sequence(2) + sequence(3) + block_lines())


def main():
    backend = input()
    match Backends.strictly(lambda: Backends.under(backend, run)):
        case Success(value=value):
            return value
        case Error(message=message):
            return print(f"check stopped: {message}")
