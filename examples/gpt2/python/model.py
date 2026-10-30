# osprey: flavor=python
# osprey: namespace=gpt2
# osprey: module=Model
"""The GPT-2 124M transformer, written once against the `Tensor` effect in gpt2.Backends.

This is Python concrete syntax with Osprey semantics, not a CPython module. The
model performs `build`, `get`, `mat_mul` and friends and never names a buffer
type: the handler a program installs, `on_lists` or `on_gpu`, decides whether a
tensor is a host list or a GPU buffer, and both compute the same doubles in the
same order as examples/gpt2/reference.py. The forward pass keeps every block's
activations, and the KV-cached step runs the newest position alone over each
block's cached qkv rows ([STAGE-BACKEND], [GPU-DEVICE], [FLAVOR-PYTHON-GPU]).
"""
from gpt2.Backends import Tensor, stop

__all__ = [
    "WEIGHTS_DIR", "VOCAB_SIZE", "WIDTH", "HEADS", "HEAD_WIDTH", "QKV_WIDTH", "MLP_WIDTH",
    "LAYER_COUNT", "POSITIONS", "END_OF_TEXT", "EPSILON", "WIDTH_F", "SCORE_SCALE",
    "GELU_SCALE", "GELU_CUBIC", "Layer", "Weights", "Acts", "Attn", "Trace", "BlockStep",
    "Stepped", "at", "size", "empty", "device", "total", "plus", "add_f", "max_f", "gelu",
    "gelus", "loaded", "load_weights", "empty_layer", "layer", "transpose", "product", "sum_range",
    "squared_deviation", "row_means", "row_variances", "layer_norm", "linear", "q_at", "k_at",
    "v_at", "attention_from", "attention", "block", "embed", "forward", "block_step",
    "embed_one", "forward_step", "qkv_caches",
]

WEIGHTS_DIR = "examples/gpt2/weights"

# The GPT-2 small configuration (config.json in the same repository).
VOCAB_SIZE = 50257
WIDTH = 768
HEADS = 12
HEAD_WIDTH = 64
QKV_WIDTH = 2304
MLP_WIDTH = 3072
LAYER_COUNT = 12
POSITIONS = 1024
END_OF_TEXT = 50256
EPSILON = 0.00001
WIDTH_F = 768.0
# 1 / sqrt(HEAD_WIDTH)
SCORE_SCALE = 0.125
# sqrt(2 / pi) and the cubic coefficient of the tanh GELU approximation.
GELU_SCALE = 0.7978845608028654
GELU_CUBIC = 0.044715


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
    return perform(Tensor.get(m, i))


def size(m):
    return perform(Tensor.length(m))


def empty():
    return perform(Tensor.from_list([]))


def device():
    return perform(Tensor.device())


def total(b):
    return perform(Tensor.reduce(b, 0.0, add_f))


def plus(a, b):
    return perform(Tensor.zip_with(a, b, add_f))


def add_f(a, x):
    return a + x


def max_f(a, x):
    return x if x > a else a


def gelu(x):
    return 0.5 * x * (1.0 + tanh(GELU_SCALE * (x + GELU_CUBIC * x * x * x)))


def gelus(f):
    return perform(Tensor.build(size(f), lambda i: gelu(at(f, i))))


def or_stop(result, abandoned):
    """A tensor operation's buffer, or a fault that stops the run with the reason
    ([FAILURE-EXPLICIT]). The fault never resumes, so `abandoned` is never
    called; it only gives the stopped arm the buffer type."""
    match result:
        case Success(value=value):
            return value
        case Error(message=message):
            g = stop(message)
            return abandoned()


def loaded(path, name):
    """The tensor `name` of the safetensors file at `path`; a missing or
    mis-typed tensor stops the run rather than fabricate a runnable model."""
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
    wte = tensor("wte.weight")
    return Weights(
        wte=wte, wte_t=transpose(wte, VOCAB_SIZE, WIDTH), wpe=tensor("wpe.weight"),
        lnf_g=tensor("ln_f.weight"), lnf_b=tensor("ln_f.bias"),
        blocks=fold(range(LAYER_COUNT), [], lambda acc, i: listAppend(acc, layer_at(i))),
    )


def empty_layer():
    e = empty()
    return Layer(ln1_g=e, ln1_b=e, qkv_w=e, qkv_b=e, proj_w=e, proj_b=e, ln2_g=e, ln2_b=e, fc_w=e, fc_b=e, out_w=e, out_b=e)


def layer(w, i):
    return unwrap_or(listGet(w.blocks, i), empty_layer())


def transpose(m, rows, cols):
    """m [rows, cols] as [cols, rows]: tensors are rank-1, so a product against a
    transposed operand is a gather followed by `mat_mul`."""
    return perform(Tensor.build(rows * cols, lambda i: at(m, (i % rows) * cols + i // rows)))


def product(a, b, rows, inner, cols):
    """a [rows, inner] times b [inner, cols]; operands that disagree with the
    stated shape stop the run."""
    return or_stop(perform(Tensor.mat_mul(a, b, rows, inner, cols)), lambda: a)


def sum_range(x, start, n):
    """Sum of a contiguous run of a flat tensor."""
    return fold(range(start, start + n), 0.0, lambda a, i: a + at(x, i))


def squared_deviation(x, start, n, mean):
    return fold(range(start, start + n), 0.0, lambda a, i: a + (at(x, i) - mean) * (at(x, i) - mean))


def row_means(x, t):
    """Per-row mean of x [t, width], the first LayerNorm statistic."""
    return perform(Tensor.build(t, lambda r: sum_range(x, r * WIDTH, WIDTH) / WIDTH_F))


def row_variances(x, t, means):
    return perform(Tensor.build(t, lambda r: squared_deviation(x, r * WIDTH, WIDTH, at(means, r)) / WIDTH_F))


def normalised(x, means, variances, g, b, i):
    r = i // WIDTH
    c = i % WIDTH
    return (at(x, i) - at(means, r)) / sqrt(at(variances, r) + EPSILON) * at(g, c) + at(b, c)


def layer_norm(x, t, g, b):
    """LayerNorm over each of the t rows of x [t, width]."""
    means = row_means(x, t)
    variances = row_variances(x, t, means)
    return perform(Tensor.build(t * WIDTH, lambda i: normalised(x, means, variances, g, b, i)))


def linear(x, rows, inner, cols, w, bias):
    """x [rows, inner] times w [inner, cols], plus the bias on every row."""
    value = product(x, w, rows, inner, cols)
    return perform(Tensor.build(rows * cols, lambda i: at(value, i) + at(bias, i % cols)))


def q_at(qkv, r, h, d):
    """qkv is [t, 2304]: the query, key and value of head h occupy 64 columns each."""
    return at(qkv, r * QKV_WIDTH + h * HEAD_WIDTH + d)


def k_at(qkv, r, h, d):
    return at(qkv, r * QKV_WIDTH + WIDTH + h * HEAD_WIDTH + d)


def v_at(qkv, r, h, d):
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
    peaks = perform(Tensor.build(HEADS * n, lambda row: row_max(scores, row, t, first + row % n)))
    weights = perform(Tensor.build(HEADS * n * t, lambda i: causal_weight(scores, peaks, t, n, first, i)))
    sums = perform(Tensor.build(HEADS * n, lambda row: sum_range(weights, row * t, t)))
    return perform(Tensor.build(HEADS * n * t, lambda i: at(weights, i) / at(sums, i // t)))


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
    probabilities are [heads, t - first, t]."""
    n = t - first
    probs = probs_of(perform(Tensor.build(HEADS * n * t, lambda i: causal_score(qkv, t, n, first, i))), t, n, first)
    return Attn(probs=probs, out=perform(Tensor.build(n * WIDTH, lambda i: attended(probs, qkv, t, n, first, i))))


def attention(qkv, t):
    """Every position of qkv [t, 2304] at once, as [t, width]."""
    return attention_from(qkv, t, 0)


def block(x, t, l):
    """One transformer block: pre-norm attention and MLP, each with a residual."""
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
    position, so every kernel sees buffers and scalars only."""
    return fold(range(t), empty(), lambda rows, p: appended(rows, embed_one(w, unwrap_or(listGet(ids, p), 0), p)))


def forward(w, ids, t):
    """The twelve blocks over the embedded tokens [t, width]."""
    return blocks(w, embed(w, ids, t), t, 0, [])


def appended(rows, more):
    """`rows` followed by `more`: a cache growing by one position."""
    n = size(rows)
    return perform(Tensor.build(n + size(more), lambda i: at(rows, i) if i < n else at(more, i - n)))


def block_step(x, t, l, cached):
    """One block for the newest position t - 1 alone: its row x [1, width] and the
    block's qkv rows for the earlier positions [t - 1, 2304]. Each of the row's
    values is the same arithmetic in the same order as the full pass computes for
    that row, because LayerNorm, linear and the MLP are row-wise and attention
    reads the cached rows exactly where it did."""
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
    """The embedding of one token at position p, [1, width]."""
    return embed_row(w.wte, w.wpe, token, p)


def embed_row(wte, wpe, token, p):
    return perform(Tensor.build(WIDTH, lambda c: at(wte, token * WIDTH + c) + at(wpe, p * WIDTH + c)))


def forward_step(w, token, t, caches):
    """The newest position, token at index t - 1, through the twelve blocks over
    their caches of the t - 1 earlier positions."""
    return step_blocks(w, embed_one(w, token, t - 1), t, 0, caches, [])


def cache_at(trace, i):
    match listGet(trace.acts, i):
        case Success(value=value):
            return value.qkv
        case Error():
            return empty()


def qkv_caches(trace):
    """Each block's qkv rows from a full pass: the caches a first forward_step grows."""
    return fold(range(LAYER_COUNT), [], lambda acc, i: listAppend(acc, cache_at(trace, i)))
