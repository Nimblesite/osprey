#!/usr/bin/env python3
"""Independent GPT-2 (124M) reference in plain Python — no numpy, no torch.

Reads the same `model.safetensors` and `merges.txt` the Osprey program reads,
runs the same binary32 (float32) forward pass with the same left-to-right
accumulation order, and prints the greedy continuation — or, with --train, the
same loss, gradient sums and post-step loss as src/osprey/examples/llms/gpt2/train. Every
arithmetic result is rounded to binary32 exactly where the compiled port rounds
it; `exp`/`ln`/`tanh` call the single-precision libm symbols (`expf`/`logf`/
`tanhf`) the compiler emits for a `float32` argument, and `sqrt` is the
correctly rounded single-precision root. It exists only to check
src/osprey/examples/llms/gpt2: run both on one prompt and compare the output.

    python3 src/osprey/examples/llms/gpt2/reference.py src/osprey/examples/llms/gpt2/weights "The capital of France is" 20
    python3 src/osprey/examples/llms/gpt2/reference.py src/osprey/examples/llms/gpt2/weights --train "The capital of France is"
"""
import ctypes
import ctypes.util
import json
import math
import re
import struct
import sys


def f32(value):
    """Round `value` to IEEE 754 binary32 (round to nearest, ties to even)."""
    return struct.unpack("<f", struct.pack("<f", value))[0]


WIDTH, HEADS, HEAD_WIDTH, LAYERS, VOCAB, END, POSITIONS = 768, 12, 64, 12, 50257, 50256, 1024
QKV, MLP = 2304, 3072
SCALE = f32(0.125)
EPS = f32(1e-5)
WIDTH_F = f32(768.0)
GELU_SCALE = f32(0.7978845608028654)
GELU_CUBIC = f32(0.044715)
LEARNING_RATE = f32(0.0001)
PATTERN = re.compile(
    r"""'s|'t|'re|'ve|'m|'ll|'d| ?[A-Za-z\u0080-￿]+| ?[0-9]+| ?[^\sA-Za-z0-9\u0080-￿]+|\s+(?!\S)|\s+"""
)


def _load_libm():
    """The system math library whose single-precision entries the compiled
    program calls: clang lowers `llvm.exp.f32`, `llvm.log.f32` and `tanhf` to
    `expf`, `logf` and `tanhf` on this host ([BUILTIN-MATH])."""
    for name in (ctypes.util.find_library("m"), "libm.so.6", "/usr/lib/libSystem.B.dylib"):
        if not name:
            continue
        try:
            libm = ctypes.CDLL(name)
            libm.expf.restype = ctypes.c_float
            libm.expf.argtypes = [ctypes.c_float]
            libm.logf.restype = ctypes.c_float
            libm.logf.argtypes = [ctypes.c_float]
            libm.tanhf.restype = ctypes.c_float
            libm.tanhf.argtypes = [ctypes.c_float]
            return libm
        except (OSError, AttributeError, TypeError):
            continue
    return None


_LIBM = _load_libm()


def _libm():
    if _LIBM is None:
        raise RuntimeError("the system math library (libm) could not be loaded for binary32 exp/ln/tanh")
    return _LIBM


def exp32(x):
    """`llvm.exp.f32`, the float32 argument's `exp`: libm `expf`."""
    return float(_libm().expf(x))


def log32(x):
    """`llvm.log.f32`, the float32 argument's `ln`: libm `logf`."""
    return float(_libm().logf(x))


def tanh32(x):
    """`tanh` on a float32 argument: libm `tanhf`."""
    return float(_libm().tanhf(x))


def sqrt32(x):
    """`llvm.sqrt.f32`: the hardware's correctly rounded single-precision
    root, which a correctly rounded binary64 `math.sqrt` reproduces exactly
    after one rounding back to binary32."""
    return f32(math.sqrt(x))


def byte_order():
    return list(range(33, 127)) + list(range(161, 173)) + list(range(174, 256))


def bytes_to_unicode():
    bs = byte_order()
    cs = bs[:]
    n = 0
    for b in range(256):
        if b not in bs:
            bs.append(b)
            cs.append(256 + n)
            n += 1
    return dict(zip(bs, map(chr, cs)))


class Tokenizer:
    def __init__(self, merges_path):
        lines = open(merges_path, encoding="utf-8").read().split("\n")[1:]
        self.merges = [tuple(m.split(" ")) for m in lines if m]
        self.ranks = {m: i for i, m in enumerate(self.merges)}
        self.b2u = bytes_to_unicode()
        self.u2b = {u: b for b, u in self.b2u.items()}
        base = [self.b2u[b] for b in sorted(self.b2u, key=lambda b: ord(self.b2u[b]) if self.b2u[b] >= "Ā" else 0)]
        self.ids = {t: i for i, t in enumerate(self._base_order())}
        for i, (a, b) in enumerate(self.merges):
            self.ids[a + b] = 256 + i
        self.tokens = {i: t for t, i in self.ids.items()}

    def _base_order(self):
        order = byte_order()
        order += [b for b in range(256) if b not in order]
        return [self.b2u[b] for b in order]

    def bpe(self, word):
        word = tuple(word)
        while len(word) > 1:
            rank, at = min((self.ranks.get(p, 1 << 62), i) for i, p in enumerate(zip(word, word[1:])))
            if rank == 1 << 62:
                break
            a, b = word[at], word[at + 1]
            out, j = [], 0
            while j < len(word):
                if j < len(word) - 1 and word[j] == a and word[j + 1] == b:
                    out.append(a + b)
                    j += 2
                else:
                    out.append(word[j])
                    j += 1
            word = tuple(out)
        return word

    def encode(self, text):
        ids = []
        for piece in PATTERN.findall(text):
            stand_in = "".join(self.b2u[b] for b in piece.encode("utf-8"))
            ids.extend(self.ids[w] for w in self.bpe(stand_in))
        return ids

    def decode(self, ids):
        raw = bytes(self.u2b[c] for i in ids for c in self.tokens[i])
        return raw.decode("utf-8", errors="replace")


class Safetensors:
    def __init__(self, path):
        self.f = open(path, "rb")
        n = struct.unpack("<Q", self.f.read(8))[0]
        self.header = json.loads(self.f.read(n))
        self.start = 8 + n

    def tensor(self, name):
        e = self.header[name]
        assert e["dtype"] == "F32", name
        begin, end = e["data_offsets"]
        self.f.seek(self.start + begin)
        return list(struct.unpack("<%df" % ((end - begin) // 4), self.f.read(end - begin)))


def add_all(values):
    """Left-to-right binary32 addition from 0.0: the host runtime's order, which
    the compensated summation of Python 3.12's built-in sum() would not
    reproduce."""
    acc = f32(0.0)
    for v in values:
        acc = f32(acc + v)
    return acc


def dot(a, b):
    return add_all(f32(x * y) for x, y in zip(a, b))


def checksum(values, cols):
    """A matrix gradient's summary as train.osp prints it: each row added
    left-to-right, then the row sums — a shape a device computes in parallel."""
    return add_all(add_all(values[r * cols:(r + 1) * cols]) for r in range(len(values) // cols))


# The column count of each matrix gradient; every other gradient is a vector.
COLUMNS = {"attn.c_attn.weight": QKV, "attn.c_proj.weight": WIDTH, "mlp.c_fc.weight": MLP, "mlp.c_proj.weight": WIDTH}


def summary(name, values):
    cols = COLUMNS.get(name)
    return checksum(values, cols) if cols else add_all(values)


def matmul(x, w, rows, inner, cols):
    # Same per-element k order as osprey_gpu_matmul: out[r][c] = Σ_k x[r][k] w[k][c].
    wcols = [w[c::cols] for c in range(cols)]
    return [dot(x[r * inner:(r + 1) * inner], wcols[c]) for r in range(rows) for c in range(cols)]


def transpose(m, rows, cols):
    return [m[(i % rows) * cols + i // rows] for i in range(rows * cols)]


def linear(x, rows, inner, cols, w, b):
    y = matmul(x, w, rows, inner, cols)
    return [f32(y[i] + b[i % cols]) for i in range(rows * cols)]


def column_sums(dy, rows, cols):
    return [add_all((dy[r * cols + c] for r in range(rows))) for c in range(cols)]


def linear_back(dy, x, rows, inner, cols, w):
    """y = x · w + b: (dx, dw, db), each product in the k order of osprey_gpu_matmul."""
    return (matmul(dy, transpose(w, inner, cols), rows, cols, inner),
            matmul(transpose(x, rows, inner), dy, inner, rows, cols),
            column_sums(dy, rows, cols))


def row_stats(x, t):
    means = [f32(add_all(x[r * WIDTH:(r + 1) * WIDTH]) / WIDTH_F) for r in range(t)]
    variances = [f32(add_all(f32(f32(v - means[r]) * f32(v - means[r])) for v in x[r * WIDTH:(r + 1) * WIDTH]) / WIDTH_F) for r in range(t)]
    return means, variances


def layer_norm(x, t, g, b):
    means, variances = row_stats(x, t)
    out = []
    for r in range(t):
        s = sqrt32(f32(variances[r] + EPS))
        out.extend(f32(f32(f32(f32(v - means[r]) / s) * g[c]) + b[c]) for c, v in enumerate(x[r * WIDTH:(r + 1) * WIDTH]))
    return out


def layer_norm_back(dy, x, t, g):
    """(dx, dg, db), recomputing the normalised rows exactly as layer_norm did."""
    means, variances = row_stats(x, t)
    xhat = [f32(f32(x[i] - means[i // WIDTH]) / sqrt32(f32(variances[i // WIDTH] + EPS))) for i in range(t * WIDTH)]
    dxhat = [f32(dy[i] * g[i % WIDTH]) for i in range(t * WIDTH)]
    m1 = [f32(add_all(dxhat[r * WIDTH:(r + 1) * WIDTH]) / WIDTH_F) for r in range(t)]
    m2 = [f32(add_all(f32(dxhat[r * WIDTH + c] * xhat[r * WIDTH + c]) for c in range(WIDTH)) / WIDTH_F) for r in range(t)]
    dx = [f32(f32(f32(dxhat[i] - m1[i // WIDTH]) - f32(xhat[i] * m2[i // WIDTH])) / sqrt32(f32(variances[i // WIDTH] + EPS))) for i in range(t * WIDTH)]
    dg = [add_all(f32(dy[r * WIDTH + c] * xhat[r * WIDTH + c]) for r in range(t)) for c in range(WIDTH)]
    return dx, dg, column_sums(dy, t, WIDTH)


def gelu_tanh_argument(x):
    """The tanh argument of the GELU approximation, rounded at every step."""
    return f32(GELU_SCALE * f32(x + f32(f32(f32(GELU_CUBIC * x) * x) * x)))


def gelu(x):
    th = tanh32(gelu_tanh_argument(x))
    return f32(f32(f32(0.5) * x) * f32(f32(1.0) + th))


def gelu_slope(x):
    th = tanh32(gelu_tanh_argument(x))
    first = f32(f32(0.5) * f32(f32(1.0) + th))
    chain = f32(f32(0.5) * x)
    chain = f32(chain * f32(f32(1.0) - f32(th * th)))
    chain = f32(chain * GELU_SCALE)
    last = f32(f32(1.0) + f32(f32(f32(f32(3.0) * GELU_CUBIC) * x) * x))
    return f32(first + f32(chain * last))


def q_at(qkv, r, h, d):
    return qkv[r * QKV + h * HEAD_WIDTH + d]


def k_at(qkv, r, h, d):
    return qkv[r * QKV + WIDTH + h * HEAD_WIDTH + d]


def v_at(qkv, r, h, d):
    return qkv[r * QKV + 2 * WIDTH + h * HEAD_WIDTH + d]


def head_slice(qkv, row, h, offset):
    return qkv[row * QKV + offset + h * HEAD_WIDTH:row * QKV + offset + (h + 1) * HEAD_WIDTH]


def attention(qkv, t):
    """Causal attention: (probs [heads, t, t] with zeros above the diagonal, out [t, width])."""
    probs = [f32(0.0)] * (HEADS * t * t)
    out = [f32(0.0)] * (t * WIDTH)
    for h in range(HEADS):
        for r in range(t):
            q = head_slice(qkv, r, h, 0)
            scores = [f32(dot(q, head_slice(qkv, c, h, WIDTH)) * SCALE) for c in range(r + 1)]
            peak = max(scores)
            weights = [exp32(f32(s - peak)) for s in scores]
            total = add_all(weights)
            for c in range(r + 1):
                probs[(h * t + r) * t + c] = f32(weights[c] / total)
            for d in range(HEAD_WIDTH):
                out[r * WIDTH + h * HEAD_WIDTH + d] = add_all(f32(probs[(h * t + r) * t + c] * v_at(qkv, c, h, d)) for c in range(r + 1))
    return probs, out


def scaled_add_all(pairs):
    return f32(add_all(f32(a * b) for a, b in pairs) * SCALE)


def attention_back(dout, probs, qkv, t):
    """dout [t, width] back through causal attention, as dqkv [t, 2304]."""
    dp = [f32(0.0)] * (HEADS * t * t)
    for h in range(HEADS):
        for r in range(t):
            for c in range(r + 1):
                dp[(h * t + r) * t + c] = add_all(f32(dout[r * WIDTH + h * HEAD_WIDTH + d] * v_at(qkv, c, h, d)) for d in range(HEAD_WIDTH))
    dots = [add_all(f32(dp[row * t + c] * probs[row * t + c]) for c in range(row % t + 1)) for row in range(HEADS * t)]
    ds = [f32(probs[i] * f32(dp[i] - dots[i // t])) for i in range(HEADS * t * t)]
    dqkv = []
    for i in range(t * QKV):
        r, col = i // QKV, i % QKV
        h, d = (col % WIDTH) // HEAD_WIDTH, col % HEAD_WIDTH
        if col < WIDTH:
            dqkv.append(scaled_add_all((ds[(h * t + r) * t + c], k_at(qkv, c, h, d)) for c in range(r + 1)))
        elif col < 2 * WIDTH:
            dqkv.append(scaled_add_all((ds[(h * t + q) * t + r], q_at(qkv, q, h, d)) for q in range(r, t)))
        else:
            dqkv.append(add_all(f32(probs[(h * t + q) * t + r] * dout[q * WIDTH + h * HEAD_WIDTH + d]) for q in range(r, t)))
    return dqkv


def residual(x, part):
    return [f32(u + v) for u, v in zip(x, part)]


def block(x, t, l):
    """One block's activations in forward order; y is its output."""
    h1 = layer_norm(x, t, l["ln_1.weight"], l["ln_1.bias"])
    qkv = linear(h1, t, WIDTH, QKV, l["attn.c_attn.weight"], l["attn.c_attn.bias"])
    probs, a = attention(qkv, t)
    x2 = residual(x, linear(a, t, WIDTH, WIDTH, l["attn.c_proj.weight"], l["attn.c_proj.bias"]))
    h2 = layer_norm(x2, t, l["ln_2.weight"], l["ln_2.bias"])
    f = linear(h2, t, WIDTH, MLP, l["mlp.c_fc.weight"], l["mlp.c_fc.bias"])
    u = [gelu(v) for v in f]
    y = residual(x2, linear(u, t, MLP, WIDTH, l["mlp.c_proj.weight"], l["mlp.c_proj.bias"]))
    return dict(x=x, h1=h1, qkv=qkv, probs=probs, a=a, x2=x2, h2=h2, f=f, u=u, y=y)


def block_back(dy, acts, t, l):
    """dy is the gradient of the block's output; returns (dx, gradients by tensor name)."""
    dm, d_out_w, d_out_b = linear_back(dy, acts["u"], t, MLP, WIDTH, l["mlp.c_proj.weight"])
    df = [f32(g * gelu_slope(x)) for g, x in zip(dm, acts["f"])]
    dh2, d_fc_w, d_fc_b = linear_back(df, acts["h2"], t, WIDTH, MLP, l["mlp.c_fc.weight"])
    dx2_ln, d_ln2_g, d_ln2_b = layer_norm_back(dh2, acts["x2"], t, l["ln_2.weight"])
    dx2 = residual(dy, dx2_ln)
    da, d_proj_w, d_proj_b = linear_back(dx2, acts["a"], t, WIDTH, WIDTH, l["attn.c_proj.weight"])
    dh1, d_qkv_w, d_qkv_b = linear_back(attention_back(da, acts["probs"], acts["qkv"], t), acts["h1"], t, WIDTH, QKV, l["attn.c_attn.weight"])
    dx_ln, d_ln1_g, d_ln1_b = layer_norm_back(dh1, acts["x"], t, l["ln_1.weight"])
    grads = {"ln_1.weight": d_ln1_g, "ln_1.bias": d_ln1_b, "attn.c_attn.weight": d_qkv_w, "attn.c_attn.bias": d_qkv_b,
             "attn.c_proj.weight": d_proj_w, "attn.c_proj.bias": d_proj_b, "ln_2.weight": d_ln2_g, "ln_2.bias": d_ln2_b,
             "mlp.c_fc.weight": d_fc_w, "mlp.c_fc.bias": d_fc_b, "mlp.c_proj.weight": d_out_w, "mlp.c_proj.bias": d_out_b}
    return residual(dx2, dx_ln), grads


def forward_trace(st, layers, ids):
    """Every block's activations and the final block's output."""
    t = len(ids)
    x = [f32(st["wte"][ids[i // WIDTH] * WIDTH + i % WIDTH] + st["wpe"][i]) for i in range(t * WIDTH)]
    trace = []
    for l in layers:
        trace.append(block(x, t, l))
        x = trace[-1]["y"]
    return trace, x


def final_norm(st, layers, ids):
    return layer_norm(forward_trace(st, layers, ids)[1], len(ids), st["ln_f.weight"], st["ln_f.bias"])


def forward(st, layers, ids):
    t = len(ids)
    xf = final_norm(st, layers, ids)
    last = xf[(t - 1) * WIDTH:]
    return [dot(last, st["wte"][v * WIDTH:(v + 1) * WIDTH]) for v in range(VOCAB)]


def softmax_rows(logits, t):
    probs = []
    for r in range(t):
        row = logits[r * VOCAB:(r + 1) * VOCAB]
        peak = max(row)
        weights = [exp32(f32(v - peak)) for v in row]
        total = add_all(weights)
        probs.extend(f32(w / total) for w in weights)
    return probs


def loss_head(st, xf, ids):
    """Mean next-token cross-entropy over the first t - 1 positions: (loss, dxf, dwte)."""
    t = len(ids)
    probs = softmax_rows(matmul(xf, transpose(st["wte"], VOCAB, WIDTH), t, WIDTH, VOCAB), t)
    count = f32(t - 1)
    loss = f32(0.0)
    for r in range(t - 1):
        loss = f32(loss + log32(probs[r * VOCAB + ids[r + 1]]))
    loss = f32(f32(f32(0.0) - loss) / count)
    dlogits = [f32(0.0) if i // VOCAB == t - 1 else f32(f32(probs[i] - (f32(1.0) if i % VOCAB == ids[i // VOCAB + 1] else f32(0.0))) / count)
               for i in range(t * VOCAB)]
    return (loss, matmul(dlogits, st["wte"], t, VOCAB, WIDTH),
            matmul(transpose(dlogits, t, VOCAB), xf, VOCAB, t, WIDTH))


def embed_back(dx0, dwte_out, ids):
    t = len(ids)
    dwte = list(dwte_out)
    for i in range(VOCAB * WIDTH):
        a = dwte_out[i]
        for r in range(t):
            if ids[r] == i // WIDTH:
                a = f32(a + dx0[r * WIDTH + i % WIDTH])
        dwte[i] = a
    return dwte, [dx0[i] if i // WIDTH < t else f32(0.0) for i in range(POSITIONS * WIDTH)]


def descend(p, g):
    return [f32(a - f32(LEARNING_RATE * d)) for a, d in zip(p, g)]


LAYER_NAMES = ["ln_1.weight", "ln_1.bias", "attn.c_attn.weight", "attn.c_attn.bias", "attn.c_proj.weight",
               "attn.c_proj.bias", "ln_2.weight", "ln_2.bias", "mlp.c_fc.weight", "mlp.c_fc.bias",
               "mlp.c_proj.weight", "mlp.c_proj.bias"]


def train_step(st, layers, ids):
    """(loss, per-layer gradients, dwte, dwpe, d ln_f weight, d ln_f bias)."""
    t = len(ids)
    trace, x = forward_trace(st, layers, ids)
    loss, dxf, dwte_out = loss_head(st, layer_norm(x, t, st["ln_f.weight"], st["ln_f.bias"]), ids)
    dy, d_lnf_g, d_lnf_b = layer_norm_back(dxf, x, t, st["ln_f.weight"])
    grads = []
    for i in range(LAYERS - 1, -1, -1):
        dy, g = block_back(dy, trace[i], t, layers[i])
        grads.insert(0, g)
    dwte, dwpe = embed_back(dy, dwte_out, ids)
    return loss, grads, dwte, dwpe, d_lnf_g, d_lnf_b


def train(st, layers, ids, prompt):
    print("gpt2 (124M) on host — one training step over %d tokens, learning rate %r\nprompt: %s\ntokens: %s"
          % (len(ids), LEARNING_RATE, prompt, " ".join(map(str, ids))), flush=True)
    loss, grads, dwte, dwpe, d_lnf_g, d_lnf_b = train_step(st, layers, ids)
    lines = ["loss %r" % loss, "grad wte %r wpe %r ln_f %r %r" % (checksum(dwte, WIDTH), checksum(dwpe, WIDTH), add_all(d_lnf_g), add_all(d_lnf_b))]
    lines.extend("h.%d %s" % (i, " ".join(repr(summary(n, g[n])) for n in LAYER_NAMES)) for i, g in enumerate(grads))
    print("\n".join(lines), flush=True)
    st = {"wte": descend(st["wte"], dwte), "wpe": descend(st["wpe"], dwpe),
          "ln_f.weight": descend(st["ln_f.weight"], d_lnf_g), "ln_f.bias": descend(st["ln_f.bias"], d_lnf_b)}
    layers = [{n: descend(l[n], g[n]) for n in LAYER_NAMES} for l, g in zip(layers, grads)]
    t = len(ids)
    xf = final_norm(st, layers, ids)
    print("loss after one step %r" % loss_head(st, xf, ids)[0])


def main():
    weights, training = sys.argv[1], sys.argv[2] == "--train"
    prompt, count = (sys.argv[3], 0) if training else (sys.argv[2], int(sys.argv[3]))
    tok = Tokenizer(weights + "/merges.txt")
    sf = Safetensors(weights + "/model.safetensors")
    st = {k: sf.tensor(k + ".weight" if k in ("wte", "wpe") else k) for k in ("wte", "wpe", "ln_f.weight", "ln_f.bias")}
    layers = [{n: sf.tensor("h.%d.%s" % (i, n)) for n in LAYER_NAMES} for i in range(LAYERS)]
    ids = tok.encode(prompt)
    if training:
        return train(st, layers, ids, prompt)
    print("tokens:", " ".join(map(str, ids)))
    for _ in range(count):
        logits = forward(st, layers, ids)
        nxt = max(range(VOCAB), key=lambda v: (logits[v], -v))
        print("next", nxt, repr(tok.decode([nxt])), "logit", repr(logits[nxt]), flush=True)
        if nxt == END:
            break
        ids.append(nxt)
    print(tok.decode(ids))


if __name__ == "__main__":
    main()
