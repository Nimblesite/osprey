# osprey: flavor=python
# osprey: namespace=gpt2
# osprey: module=Model
"""The GPT-2 124M transformer, written once against the `Tensor` effect in gpt2.Backends.

This is Python concrete syntax with Osprey semantics, not a CPython module. The
model performs `build`, `get`, `mat_mul` and friends and never names a buffer
type: the handler a program installs, `on_lists` or `on_gpu`, decides whether a
tensor is a host list or a GPU buffer, and both compute the same binary32 values
in the same order as src/osprey/examples/llms/gpt2/reference.py. The forward pass keeps every block's
activations, and the KV-cached step runs the newest position alone over each
block's cached qkv rows ([STAGE-BACKEND], [GPU-DEVICE], [FLAVOR-PYTHON-GPU]).
"""
from gpt2.Backends import Tensor, stop

__all__ = [
    "WEIGHTS_DIR", "VOCAB_SIZE", "WIDTH", "HEADS", "HEAD_WIDTH", "QKV_WIDTH", "MLP_WIDTH",
    "LAYER_COUNT", "POSITIONS", "END_OF_TEXT", "EPSILON", "WIDTH_F", "SCORE_SCALE",
    "GELU_SCALE", "GELU_CUBIC", "Layer", "Weights", "Acts", "Attn", "Trace", "BlockStep",
    "Stepped", "at", "size", "empty", "device", "total", "plus", "add_f", "max_f", "peak", "gelu",
    "gelus", "or_stop", "index_of", "loaded", "load_weights", "empty_layer", "layer", "transpose", "product", "sum_range",
    "squared_deviation", "row_means", "row_variances", "layer_norm", "linear", "q_at", "k_at",
    "v_at", "attention_from", "attention", "block", "embed", "forward", "block_step",
    "embed_one", "forward_step", "qkv_caches",
]

WEIGHTS_DIR = "src/osprey/examples/llms/gpt2/weights"
"""The directory holding the checkpoint: `model.safetensors` for the weights
and `merges.txt` for the tokenizer.

# Examples
```osprey
print(WEIGHTS_DIR)
```
```output
src/osprey/examples/llms/gpt2/weights
```
"""

# The GPT-2 small configuration (config.json in the same repository).
VOCAB_SIZE = 50257
"""The size of the vocabulary: token ids run from 0 to 50256.

# Examples
```osprey
print(f"ids run from 0 to {VOCAB_SIZE}, so {VOCAB_SIZE} tokens")
```
```output
ids run from 0 to 50257, so 50257 tokens
```
"""
WIDTH = 768
"""The residual width: every block's input and output row has 768 elements.

# Examples
```osprey
print(f"every block's rows hold {WIDTH} elements")
```
```output
every block's rows hold 768 elements
```
"""
HEADS = 12
"""The attention heads each block splits its width into.

# Examples
```osprey
print(f"{HEADS} heads of {HEAD_WIDTH} elements")
```
```output
12 heads of 64 elements
```
"""
HEAD_WIDTH = 64
"""One head's width, `WIDTH / HEADS`.

# Examples
```osprey
print(f"head width {HEAD_WIDTH}, width {WIDTH}, heads {HEADS}")
```
```output
head width 64, width 768, heads 12
```
"""
QKV_WIDTH = 2304
"""The width of a block's fused query-key-value projection, `3 * WIDTH`.

# Examples
```osprey
print(f"{QKV_WIDTH} = 3 * {WIDTH}")
```
```output
2304 = 3 * 768
```
"""
MLP_WIDTH = 3072
"""The hidden width of a block's MLP, `4 * WIDTH`.

# Examples
```osprey
print(f"{MLP_WIDTH} = 4 * {WIDTH}")
```
```output
3072 = 4 * 768
```
"""
LAYER_COUNT = 12
"""How many transformer blocks the model stacks.

# Examples
```osprey
print(f"{LAYER_COUNT} blocks")
```
```output
12 blocks
```
"""
POSITIONS = 1024
"""The longest sequence the position embeddings cover.

# Examples
```osprey
print(f"positions 0 to {POSITIONS}")
```
```output
positions 0 to 1024
```
"""
END_OF_TEXT = 50256
"""The token id that ends generated text.

# Examples
```osprey
print(f"{END_OF_TEXT} ends generation")
```
```output
50256 ends generation
```
"""
EPSILON: float32 = 0.00001
"""The constant added to a LayerNorm row's variance before its square root.

# Examples
```osprey
print(EPSILON)
```
```output
1e-05
```
"""
WIDTH_F: float32 = 768.0
"""`WIDTH` as a float32, so row sums divide into means.

# Examples
```osprey
print(WIDTH_F)
```
```output
768.0
```
"""
# 1 / sqrt(HEAD_WIDTH)
SCORE_SCALE: float32 = 0.125
"""The attention scores' scale, 1 / sqrt(HEAD_WIDTH).

# Examples
```osprey
print(f"{SCORE_SCALE} = 1 / sqrt({HEAD_WIDTH})")
```
```output
0.125 = 1 / sqrt(64)
```
"""
# sqrt(2 / pi) and the cubic coefficient of the tanh GELU approximation.
GELU_SCALE: float32 = 0.7978845608028654
"""sqrt(2 / pi), the slope of the tanh GELU approximation.

# Examples
```osprey
print(f"{GELU_SCALE} tanh GELU slope, {GELU_CUBIC} cubic")
```
```output
0.7978845608028654 tanh GELU slope, 0.044715 cubic
```
"""
GELU_CUBIC: float32 = 0.044715
"""The cubic coefficient inside the tanh GELU approximation.

# Examples
```osprey
print(f"{GELU_CUBIC} x^3 inside the tanh")
```
```output
0.044715 x^3 inside the tanh
```
"""


class Layer[B]:
    """One block's parameters. `B` is whatever the installed `Tensor` handler
    represents a tensor as; the model never looks inside."""
    ln1_g: B
    ln1_b: B
    qkv_w: B
    qkv_b: B
    proj_w: B
    proj_b: B
    ln2_g: B
    ln2_b: B
    fc_w: B
    fc_b: B
    out_w: B
    out_b: B


class Weights[B]:
    """wte_t is the token embedding transposed to [width, vocab], so the tied output
    projection is one product instead of fifty thousand gathers per step."""
    wte: B
    wte_t: B
    wpe: B
    lnf_g: B
    lnf_b: B
    blocks: list[Layer[B]]


class Acts[B]:
    """Everything one block computes, in forward order. `y` is the block's output
    and the next block's `x`."""
    x: B
    h1: B
    qkv: B
    probs: B
    a: B
    x2: B
    h2: B
    f: B
    u: B
    y: B


class Attn[B]:
    """Causal attention keeps its probabilities [heads, t, t] beside its output."""
    probs: B
    out: B


class Trace[B]:
    """The forward trace: one Acts per block, and the last block's output."""
    acts: list[Acts[B]]
    x: B


class BlockStep[B]:
    """One block's output row for the newest position, and its qkv rows for every
    position so far [t, 2304]: the KV cache a later position attends to."""
    y: B
    qkv: B


class Stepped[B]:
    """The newest position through every block: its final row [1, width] and each
    block's grown cache."""
    x: B
    caches: list[B]


def at(m, i):
    """Element `i` of tensor `m`, read through the installed handler.

    # Examples
    ```osprey
    from gpt2 import Backends, Cpu
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(f"{at([toFloat32(5.0), toFloat32(6.0)], 0)} {at([toFloat32(5.0)], 7)}"))), "at")
    ```
    ```output
    5.0 0.0
    ```
    """
    return perform(Tensor.get(m, i))


def size(m):
    """The element count of tensor `m`.

    # Examples
    ```osprey
    from gpt2 import Backends, Cpu
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(size(empty())))), "size")
    ```
    ```output
    0
    ```
    """
    return perform(Tensor.length(m))


def empty():
    """The empty tensor: where a fold over rows starts, and what the unused
    arm of a stopped operation answers.

    # Examples
    ```osprey
    from gpt2 import Backends, Cpu
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(toString(empty())))), "empty")
    ```
    ```output
    []
    ```
    """
    return or_stop(perform(Tensor.from_list([])), empty)


def device():
    """The installed handler's own name for where the arithmetic runs.

    # Examples
    ```osprey
    from gpt2 import Backends, Cpu
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(device()))), "device")
    ```
    ```output
    lists
    ```
    """
    return perform(Tensor.device())


def total(b):
    """The sum of every element of `b`, or a fault from the backend.

    # Examples
    ```osprey
    from gpt2 import Backends, Cpu
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(f"{total([toFloat32(1.0), toFloat32(2.0), toFloat32(3.0)])} {total(empty())}"))), "total")
    ```
    ```output
    6.0 0.0
    ```
    """
    return or_stop(perform(Tensor.reduce(b, 0.0, add_f)), lambda: at(b, 0))


def plus(a, b):
    """`a` and `b` added element by element, or a fault from the backend.

    # Examples
    ```osprey
    from gpt2 import Backends, Cpu
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(toString(plus([toFloat32(1.0), toFloat32(2.0)], [toFloat32(10.0), toFloat32(20.0)]))))), "plus")
    ```
    ```output
    [11.0, 22.0]
    ```
    """
    return or_stop(perform(Tensor.zip_with(a, b, add_f)), lambda: a)


def add_f(a, x):
    """Two elements to one by addition: the `Combine` of `total` and `plus`.

    # Examples
    ```osprey
    from gpt2 import Backends
    Backends.reported(Backends.strictly(lambda: print(add_f(toFloat32(1.0), toFloat32(2.0)))), "added")
    ```
    ```output
    3.0
    ```
    """
    return a + x


def max_f(a, x):
    """The larger of two elements: the `Combine` of `peak` and of every
    softmax's row peak.

    # Examples
    ```osprey
    from gpt2 import Backends
    Backends.reported(Backends.strictly(lambda: print(f"{max_f(toFloat32(1.0), toFloat32(2.0))} {max_f(toFloat32(3.0), toFloat32(2.0))}")), "larger")
    ```
    ```output
    2.0 3.0
    ```
    """
    return x if x > a else a


def peak(b):
    """The largest element of a nonempty tensor, or a fault from the backend.

    # Examples
    ```osprey
    from gpt2 import Backends, Cpu
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(peak([toFloat32(1.0), toFloat32(3.0), toFloat32(2.0)])))), "peak")
    ```
    ```output
    3.0
    ```
    """
    return or_stop(perform(Tensor.reduce(b, at(b, 0), max_f)), lambda: at(b, 0))


def gelu(x):
    """The GELU of one element by the tanh approximation.

    # Examples
    ```osprey
    from gpt2 import Backends
    Backends.reported(Backends.strictly(lambda: print(f"{gelu(toFloat32(1.0))} {gelu(toFloat32(0.0))}")), "gelu")
    ```
    ```output
    0.841192 0.0
    ```
    """
    return toFloat32(0.5) * x * (toFloat32(1.0) + tanh(GELU_SCALE * (x + GELU_CUBIC * x * x * x)))


def gelus(f):
    """The GELU of every element of `f`, or a fault from the backend.

    # Examples
    ```osprey
    from gpt2 import Backends, Cpu
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(toString(gelus([toFloat32(0.0), toFloat32(1.0)]))))), "gelus")
    ```
    ```output
    [0.0, 0.841192]
    ```
    """
    return or_stop(perform(Tensor.build(size(f), lambda i: gelu(at(f, i)))), empty)


def or_stop(result, abandoned):
    """A tensor operation's value, or a fault that stops the run with the reason
    ([FAILURE-EXPLICIT]). The fault never resumes, so `abandoned` is never
    called; it only gives the stopped arm the result type.

    # Examples
    ```osprey
    from gpt2 import Backends
    Backends.reported(Backends.strictly(lambda: print(or_stop(Success(value=toFloat32(1.0)), lambda: toFloat32(0.0)))), "kept")
    Backends.reported(Backends.strictly(lambda: print(f"{or_stop(Error(message='no such tensor'), lambda: toFloat32(0.0))}")), "run")
    ```
    ```output
    1.0
    run stopped: no such tensor
    ```
    """
    match result:
        case Success(value=value):
            return value
        case Error(message=message):
            _g = stop(message)
            return abandoned()


def index_of(buffer, value):
    """Find an index or stop before using a failed index allocation.

    # Examples
    ```osprey
    from gpt2 import Backends, Cpu
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(f"{index_of([toFloat32(3.0), toFloat32(1.0), toFloat32(4.0)], toFloat32(4.0))} {index_of([toFloat32(3.0), toFloat32(1.0), toFloat32(4.0)], toFloat32(7.0))}"))), "index")
    ```
    ```output
    2 3
    ```
    """
    return or_stop(perform(Tensor.index_of(buffer, value)), lambda: size(buffer))


def loaded(path, name):
    """The tensor `name` of the safetensors file at `path`; a missing or
    mis-typed tensor stops the run rather than fabricate a runnable model.

    # Examples
    ```osprey
    from gpt2 import Backends, Cpu
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(toString(loaded("no-such-weights.safetensors", "wte.weight"))))), "loaded")
    ```
    ```output
    loaded stopped: loadTensor: no-such-weights.safetensors: No such file or directory
    ```
    """
    return or_stop(perform(Tensor.load(path, name)), empty)


def tensor(name):
    """One tensor of the checkpoint."""
    return loaded(WEIGHTS_DIR + "/model.safetensors", name)


def layer_at(i):
    p = f"h.{i}."
    return Layer(
        ln1_g=tensor(p + "ln_1.weight"), ln1_b=tensor(p + "ln_1.bias"),
        qkv_w=tensor(p + "attn.c_attn.weight"), qkv_b=tensor(p + "attn.c_attn.bias"),
        proj_w=tensor(p + "attn.c_proj.weight"), proj_b=tensor(p + "attn.c_proj.bias"),
        ln2_g=tensor(p + "ln_2.weight"), ln2_b=tensor(p + "ln_2.bias"),
        fc_w=tensor(p + "mlp.c_fc.weight"), fc_b=tensor(p + "mlp.c_fc.bias"),
        out_w=tensor(p + "mlp.c_proj.weight"), out_b=tensor(p + "mlp.c_proj.bias"),
    )


def load_weights():
    """The whole checkpoint from `WEIGHTS_DIR`: the embeddings, the final
    LayerNorm and every block, the token embedding transposed for the output
    projection; a missing tensor stops the run."""
    wte = tensor("wte.weight")
    return Weights(
        wte=wte, wte_t=transpose(wte, VOCAB_SIZE, WIDTH), wpe=tensor("wpe.weight"),
        lnf_g=tensor("ln_f.weight"), lnf_b=tensor("ln_f.bias"),
        blocks=fold(range(LAYER_COUNT), [], lambda acc, i: listAppend(acc, layer_at(i))),
    )


def empty_layer():
    """A block whose twelve parameters are all the empty tensor: what a
    missing block or gradient stands in as.

    # Examples
    ```osprey
    from gpt2 import Backends, Cpu
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(size(empty_layer().fc_w)))), "empty layer")
    ```
    ```output
    0
    ```
    """
    e = empty()
    return Layer(ln1_g=e, ln1_b=e, qkv_w=e, qkv_b=e, proj_w=e, proj_b=e, ln2_g=e, ln2_b=e, fc_w=e, fc_b=e, out_w=e, out_b=e)


def layer(w, i):
    """Block `i` of `w`, or an empty block when `w` has none there.

    # Examples
    ```osprey
    from gpt2 import Backends, Cpu
    def no_weights():
        return Weights(wte=empty(), wte_t=empty(), wpe=empty(), lnf_g=empty(), lnf_b=empty(), blocks=[])
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(size(layer(no_weights(), 0).qkv_w)))), "layer")
    ```
    ```output
    0
    ```
    """
    return unwrap_or(listGet(w.blocks, i), empty_layer())


def transpose(m, rows, cols):
    """m [rows, cols] as [cols, rows]: tensors are rank-1, so a product against a
    transposed operand is a gather followed by `mat_mul`.

    # Examples
    ```osprey
    from gpt2 import Backends, Cpu
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(toString(transpose([toFloat32(1.0), toFloat32(2.0), toFloat32(3.0), toFloat32(4.0)], 2, 2))))), "transposed")
    ```
    ```output
    [1.0, 3.0, 2.0, 4.0]
    ```
    """
    return or_stop(perform(Tensor.build(rows * cols, lambda i: at(m, (i % rows) * cols + i // rows))), empty)


def product(a, b, rows, inner, cols):
    """a [rows, inner] times b [inner, cols]; operands that disagree with the
    stated shape stop the run.

    # Examples
    ```osprey
    from gpt2 import Backends, Cpu
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(toString(product([toFloat32(1.0), toFloat32(2.0), toFloat32(3.0), toFloat32(4.0)], [toFloat32(5.0), toFloat32(6.0), toFloat32(7.0), toFloat32(8.0)], 2, 2, 2))))), "product")
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(toString(product([toFloat32(1.0), toFloat32(2.0)], [toFloat32(1.0)], 1, 1, 1))))), "misfit")
    ```
    ```output
    [19.0, 22.0, 43.0, 50.0]
    misfit stopped: gpuMatMul: left operand: has 2 elements, the stated shape needs 1
    ```
    """
    return or_stop(perform(Tensor.mat_mul(a, b, rows, inner, cols)), lambda: a)


def sum_range(x, start, n):
    """Sum of a contiguous run of a flat tensor.

    # Examples
    ```osprey
    from gpt2 import Backends, Cpu
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(sum_range([toFloat32(1.0), toFloat32(2.0), toFloat32(3.0), toFloat32(4.0)], 1, 2)))), "sum")
    ```
    ```output
    5.0
    ```
    """
    return fold(range(start, start + n), 0.0, lambda a, i: a + at(x, i))


def squared_deviation(x, start, n, mean):
    """The sum of the squared distances from `mean` of the `n` elements of
    `x` from `start`: a row's variance before it is divided by the width.

    # Examples
    ```osprey
    from gpt2 import Backends, Cpu
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(squared_deviation([toFloat32(1.0), toFloat32(3.0)], 0, 2, toFloat32(2.0))))), "spread")
    ```
    ```output
    2.0
    ```
    """
    return fold(range(start, start + n), 0.0, lambda a, i: a + (at(x, i) - mean) * (at(x, i) - mean))


def row_means(x, t):
    """Per-row mean of x [t, width], the first LayerNorm statistic.

    # Examples
    ```osprey
    from gpt2 import Backends, Cpu
    def ones():
        return or_stop(perform(Tensor.build(WIDTH, lambda i: toFloat32(i * 0 + 1))), empty)
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(toString(row_means(ones(), 1))))), "means")
    ```
    ```output
    [1.0]
    ```
    """
    return or_stop(perform(Tensor.build(t, lambda r: sum_range(x, r * WIDTH, WIDTH) / WIDTH_F)), empty)


def row_variances(x, t, means):
    """The variance of each of the t rows of x [t, width] about its mean in
    `means`, or a fault from the backend.

    # Examples
    ```osprey
    from gpt2 import Backends, Cpu
    def ones():
        return or_stop(perform(Tensor.build(WIDTH, lambda i: toFloat32(i * 0 + 1))), empty)
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(toString(row_variances(ones(), 1, ones()))))), "variances")
    ```
    ```output
    [0.0]
    ```
    """
    return or_stop(perform(Tensor.build(t, lambda r: squared_deviation(x, r * WIDTH, WIDTH, at(means, r)) / WIDTH_F)), empty)


def normalised(x, means, variances, g, b, i):
    r = i // WIDTH
    c = i % WIDTH
    return (at(x, i) - at(means, r)) / sqrt(at(variances, r) + EPSILON) * at(g, c) + at(b, c)


def layer_norm(x, t, g, b):
    """LayerNorm over each of the t rows of x [t, width].

    # Examples
    ```osprey
    from gpt2 import Backends, Cpu
    def ones():
        return or_stop(perform(Tensor.build(WIDTH, lambda i: toFloat32(i * 0 + 1))), empty)
    def zeros():
        return or_stop(perform(Tensor.build(WIDTH, lambda i: toFloat32(i * 0))), empty)
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(total(layer_norm(ones(), 1, ones(), zeros()))))), "normed")
    ```
    ```output
    0.0
    ```
    """
    means = row_means(x, t)
    variances = row_variances(x, t, means)
    return or_stop(perform(Tensor.build(t * WIDTH, lambda i: normalised(x, means, variances, g, b, i))), empty)


def linear(x, rows, inner, cols, w, bias):
    """x [rows, inner] times w [inner, cols], plus the bias on every row.

    # Examples
    ```osprey
    from gpt2 import Backends, Cpu
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(toString(linear([toFloat32(1.0), toFloat32(2.0)], 1, 2, 1, [toFloat32(1.0), toFloat32(3.0)], [toFloat32(10.0)]))))), "linear")
    ```
    ```output
    [17.0]
    ```
    """
    value = product(x, w, rows, inner, cols)
    return or_stop(perform(Tensor.build(rows * cols, lambda i: at(value, i) + at(bias, i % cols))), empty)


def q_at(qkv, r, h, d):
    """qkv is [t, 2304]: the query, key and value of head h occupy 64 columns each.

    # Examples
    ```osprey
    from gpt2 import Backends, Cpu
    def ramp():
        return or_stop(perform(Tensor.build(QKV_WIDTH, lambda i: toFloat32(i * 1))), empty)
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(f"{q_at(ramp(), 0, 0, 0)} {q_at(ramp(), 0, 1, 2)}"))), "query")
    ```
    ```output
    0.0 66.0
    ```
    """
    return at(qkv, r * QKV_WIDTH + h * HEAD_WIDTH + d)


def k_at(qkv, r, h, d):
    """Element `d` of head `h`'s key for row `r` of qkv [t, 2304].

    # Examples
    ```osprey
    from gpt2 import Backends, Cpu
    def ramp():
        return or_stop(perform(Tensor.build(QKV_WIDTH, lambda i: toFloat32(i * 1))), empty)
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(f"{k_at(ramp(), 0, 0, 0)} {k_at(ramp(), 0, 1, 2)}"))), "key")
    ```
    ```output
    768.0 834.0
    ```
    """
    return at(qkv, r * QKV_WIDTH + WIDTH + h * HEAD_WIDTH + d)


def v_at(qkv, r, h, d):
    """Element `d` of head `h`'s value for row `r` of qkv [t, 2304].

    # Examples
    ```osprey
    from gpt2 import Backends, Cpu
    def ramp():
        return or_stop(perform(Tensor.build(QKV_WIDTH, lambda i: toFloat32(i * 1))), empty)
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(f"{v_at(ramp(), 0, 0, 0)} {v_at(ramp(), 0, 1, 2)}"))), "value")
    ```
    ```output
    1536.0 1602.0
    ```
    """
    return at(qkv, r * QKV_WIDTH + 2 * WIDTH + h * HEAD_WIDTH + d)


def score(qkv, h, r, c):
    return fold(range(HEAD_WIDTH), 0.0, lambda a, d: a + q_at(qkv, r, h, d) * k_at(qkv, c, h, d)) * SCORE_SCALE


def row_max(scores, row, t, r):
    return fold(range(r + 1), at(scores, row * t), lambda a, c: max_f(a, at(scores, row * t + c)))


def weighted(probs, qkv, t, n, first, r, h, d):
    """Row r of n attended rows is position first + r; probs are [heads, n, t]."""
    return fold(range(first + r + 1), 0.0, lambda a, c: a + at(probs, (h * n + r) * t + c) * v_at(qkv, c, h, d))


def causal_score(qkv, t, n, first, i):
    """Element i of the causal score tensor [heads, n, t]: zero above the diagonal."""
    h = i // (n * t)
    r = first + (i % (n * t)) // t
    c = i % t
    return score(qkv, h, r, c) if c <= r else 0.0


def causal_weight(scores, peaks, t, n, first, i):
    """Element i of the unnormalised softmax: exp against the row's peak, zero above
    the diagonal."""
    row = i // t
    return exp(at(scores, i) - at(peaks, row)) if i % t <= first + row % n else 0.0


def probs_of(scores, t, n, first):
    """Softmax of each causal row of scores [heads, n, t], where row r of a head is
    position first + r and attends to columns c <= first + r only."""
    peaks = or_stop(perform(Tensor.build(HEADS * n, lambda row: row_max(scores, row, t, first + row % n))), empty)
    weights = or_stop(perform(Tensor.build(HEADS * n * t, lambda i: causal_weight(scores, peaks, t, n, first, i))), empty)
    sums = or_stop(perform(Tensor.build(HEADS * n, lambda row: sum_range(weights, row * t, t))), empty)
    return or_stop(perform(Tensor.build(HEADS * n * t, lambda i: at(weights, i) / at(sums, i // t))), empty)


def attended(probs, qkv, t, n, first, i):
    """Element i of the attention output [n, width]."""
    r = i // WIDTH
    h = (i % WIDTH) // HEAD_WIDTH
    return weighted(probs, qkv, t, n, first, r, h, i % HEAD_WIDTH)


def attention_from(qkv, t, first):
    """Causal multi-head self-attention for positions first..t-1 of qkv [t, 2304], as
    [t - first, width]: the whole sequence when first is 0, the newest position
    alone when first is t - 1 (the KV-cached step, which reads the earlier
    positions' keys and values from qkv instead of recomputing them). Scores and
    probabilities are [heads, t - first, t].

    # Examples
    ```osprey
    from gpt2 import Backends, Cpu
    def ones_qkv():
        return or_stop(perform(Tensor.build(QKV_WIDTH, lambda i: toFloat32(i * 0 + 1))), empty)
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(f"{at(attention_from(ones_qkv(), 1, 0).probs, 0)} {total(attention_from(ones_qkv(), 1, 0).out)}"))), "attention")
    ```
    ```output
    1.0 768.0
    ```
    """
    n = t - first
    probs = probs_of(or_stop(perform(Tensor.build(HEADS * n * t, lambda i: causal_score(qkv, t, n, first, i))), empty), t, n, first)
    return Attn(probs=probs, out=or_stop(perform(Tensor.build(n * WIDTH, lambda i: attended(probs, qkv, t, n, first, i))), empty))


def attention(qkv, t):
    """Every position of qkv [t, 2304] at once, as [t, width].

    # Examples
    ```osprey
    from gpt2 import Backends, Cpu
    def ones_qkv():
        return or_stop(perform(Tensor.build(QKV_WIDTH, lambda i: toFloat32(i * 0 + 1))), empty)
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(f"{at(attention(ones_qkv(), 1).probs, 0)} {size(attention(ones_qkv(), 1).out)}"))), "attention")
    ```
    ```output
    1.0 768
    ```
    """
    return attention_from(qkv, t, 0)


def block(x, t, l):
    """One transformer block: pre-norm attention and MLP, each with a residual.

    # Examples
    ```osprey
    from gpt2 import Backends, Cpu
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(toString(size(block(empty(), 1, empty_layer()).y))))), "block")
    ```
    ```output
    block stopped: gpuMatMul: right operand: has 0 elements, the stated shape needs 1769472
    ```
    """
    h1 = layer_norm(x, t, l.ln1_g, l.ln1_b)
    qkv = linear(h1, t, WIDTH, QKV_WIDTH, l.qkv_w, l.qkv_b)
    attn = attention(qkv, t)
    x2 = plus(x, linear(attn.out, t, WIDTH, WIDTH, l.proj_w, l.proj_b))
    h2 = layer_norm(x2, t, l.ln2_g, l.ln2_b)
    f = linear(h2, t, WIDTH, MLP_WIDTH, l.fc_w, l.fc_b)
    u = gelus(f)
    y = plus(x2, linear(u, t, MLP_WIDTH, WIDTH, l.out_w, l.out_b))
    return Acts(x=x, h1=h1, qkv=qkv, probs=attn.probs, a=attn.out, x2=x2, h2=h2, f=f, u=u, y=y)


def blocks(w, x, t, i, acc):
    if i == LAYER_COUNT:
        return Trace(acts=acc, x=x)
    else:
        acts = block(x, t, layer(w, i))
        return blocks(w, acts.y, t, i + 1, listAppend(acc, acts))


def embed(w, ids, t):
    """Token plus position embeddings of the host-side ids, [t, width]: one row per
    position, so every kernel sees buffers and scalars only.

    # Examples
    ```osprey
    from gpt2 import Backends, Cpu
    def ones():
        return or_stop(perform(Tensor.build(WIDTH, lambda i: toFloat32(i * 0 + 1))), empty)
    def twos():
        return or_stop(perform(Tensor.build(2 * WIDTH, lambda i: toFloat32(i * 0 + 2))), empty)
    def toy():
        return Weights(wte=ones(), wte_t=empty(), wpe=twos(), lnf_g=empty(), lnf_b=empty(), blocks=[])
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(at(embed(toy(), [0, 0], 2), WIDTH)))), "embed")
    ```
    ```output
    3.0
    ```
    """
    return fold(range(t), empty(), lambda rows, p: appended(rows, embed_one(w, unwrap_or(listGet(ids, p), 0), p)))


def forward(w, ids, t):
    """The twelve blocks over the embedded tokens [t, width].

    # Examples
    ```osprey
    from gpt2 import Backends, Cpu
    def no_weights():
        return Weights(wte=empty(), wte_t=empty(), wpe=empty(), lnf_g=empty(), lnf_b=empty(), blocks=[])
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(toString(forward(no_weights(), [0], 1))))), "forward")
    ```
    ```output
    forward stopped: gpuMatMul: right operand: has 0 elements, the stated shape needs 1769472
    ```
    """
    return blocks(w, embed(w, ids, t), t, 0, [])


def appended(rows, more):
    """`rows` followed by `more`: a cache growing by one position."""
    n = size(rows)
    return or_stop(perform(Tensor.build(n + size(more), lambda i: at(rows, i) if i < n else at(more, i - n))), empty)


def block_step(x, t, l, cached):
    """One block for the newest position t - 1 alone: its row x [1, width] and the
    block's qkv rows for the earlier positions [t - 1, 2304]. Each of the row's
    values is the same arithmetic in the same order as the full pass computes for
    that row, because LayerNorm, linear and the MLP are row-wise and attention
    reads the cached rows exactly where it did.

    # Examples
    ```osprey
    from gpt2 import Backends, Cpu
    def no_weights():
        return Weights(wte=empty(), wte_t=empty(), wpe=empty(), lnf_g=empty(), lnf_b=empty(), blocks=[])
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(toString(block_step(empty(), 1, empty_layer(), empty()))))), "block step")
    ```
    ```output
    block step stopped: gpuMatMul: right operand: has 0 elements, the stated shape needs 1769472
    ```
    """
    qkv = appended(cached, linear(layer_norm(x, 1, l.ln1_g, l.ln1_b), 1, WIDTH, QKV_WIDTH, l.qkv_w, l.qkv_b))
    x2 = plus(x, linear(attention_from(qkv, t, t - 1).out, 1, WIDTH, WIDTH, l.proj_w, l.proj_b))
    u = gelus(linear(layer_norm(x2, 1, l.ln2_g, l.ln2_b), 1, WIDTH, MLP_WIDTH, l.fc_w, l.fc_b))
    return BlockStep(y=plus(x2, linear(u, 1, MLP_WIDTH, WIDTH, l.out_w, l.out_b)), qkv=qkv)


def step_blocks(w, x, t, i, caches, acc):
    if i == LAYER_COUNT:
        return Stepped(x=x, caches=acc)
    else:
        s = block_step(x, t, layer(w, i), unwrap_or(listGet(caches, i), empty()))
        return step_blocks(w, s.y, t, i + 1, caches, listAppend(acc, s.qkv))


def embed_one(w, token, p):
    """The embedding of one token at position p, [1, width].

    # Examples
    ```osprey
    from gpt2 import Backends, Cpu
    def ones():
        return or_stop(perform(Tensor.build(WIDTH, lambda i: toFloat32(i * 0 + 1))), empty)
    def twos():
        return or_stop(perform(Tensor.build(2 * WIDTH, lambda i: toFloat32(i * 0 + 2))), empty)
    def toy():
        return Weights(wte=ones(), wte_t=empty(), wpe=twos(), lnf_g=empty(), lnf_b=empty(), blocks=[])
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(f"{at(embed_one(toy(), 0, 1), 0)} {size(embed_one(toy(), 0, 1))}"))), "embed one")
    ```
    ```output
    3.0 768
    ```
    """
    return embed_row(w.wte, w.wpe, token, p)


def embed_row(wte, wpe, token, p):
    return or_stop(perform(Tensor.build(WIDTH, lambda c: at(wte, token * WIDTH + c) + at(wpe, p * WIDTH + c))), empty)


def forward_step(w, token, t, caches):
    """The newest position, token at index t - 1, through the twelve blocks over
    their caches of the t - 1 earlier positions.

    # Examples
    ```osprey
    from gpt2 import Backends, Cpu
    def no_weights():
        return Weights(wte=empty(), wte_t=empty(), wpe=empty(), lnf_g=empty(), lnf_b=empty(), blocks=[])
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(toString(forward_step(no_weights(), 0, 2, []))))), "forward step")
    ```
    ```output
    forward step stopped: gpuMatMul: right operand: has 0 elements, the stated shape needs 1769472
    ```
    """
    return step_blocks(w, embed_one(w, token, t - 1), t, 0, caches, [])


def cache_at(trace, i):
    match listGet(trace.acts, i):
        case Success(value=value):
            return value.qkv
        case Error():
            return empty()


def qkv_caches(trace):
    """Each block's qkv rows from a full pass: the caches a first forward_step grows.

    # Examples
    ```osprey
    from gpt2 import Backends, Cpu
    Backends.reported(Backends.strictly(lambda: Cpu.on_lists(lambda: print(toString(qkv_caches(Trace(acts=[], x=empty())))))), "caches")
    ```
    ```output
    [[], [], [], [], [], [], [], [], [], [], [], []]
    ```
    """
    return fold(range(LAYER_COUNT), [], lambda acc, i: listAppend(acc, cache_at(trace, i)))
