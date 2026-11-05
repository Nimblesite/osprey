# osprey: flavor=python
"""Qwen3.8-27B's text model on GPU buffers: one forward step of a token, greedy."""

GGUF = "/Users/christianfindlay/.cache/osprey-llm/gguf/Qwen3.8-27B-UD-Q4_K_XL.gguf"


class DeltaSlot:
    conv: GpuBuffer[float32]
    state: GpuBuffer[float32]


class AttnSlot:
    keys: GpuBuffer[float32]
    values: GpuBuffer[float32]


class Walk:
    x: GpuBuffer[float32]
    deltas: list[DeltaSlot]
    attns: list[AttnSlot]


@effect
class Fault:
    """A failed GPU step ends the run: its arm answers the whole region."""
    @control
    def halt(message: str) -> GpuBuffer[float32]: ...
    @control
    def halt_bytes(message: str) -> GpuBuffer[uint8]: ...
    @control
    def halt_walk(message: str) -> Walk: ...
    @control
    def halt_grid(message: str) -> GpuBuffer[int]: ...


@handler(Fault)
class strictly:
    def halt(message):
        return Error(message=message)
    def halt_bytes(message):
        return Error(message=message)
    def halt_walk(message):
        return Error(message=message)
    def halt_grid(message):
        return Error(message=message)


def must(result: Result[GpuBuffer[float32], Error]) -> GpuBuffer[float32]:
    match result:
        case Success(value=value):
            return value
        case Error(message=message):
            return perform(Fault.halt(toString(message)))


def fresh_grid(n) -> GpuBuffer[int]:
    match gpuIota(n):
        case Success(value=value):
            return value
        case Error(message=message):
            return perform(Fault.halt_grid(toString(message)))


class Grids:
    """Each shape's index buffer, built once and left on the device."""
    one: GpuBuffer[int]
    heads16: GpuBuffer[int]
    q_heads: GpuBuffer[int]
    v_heads: GpuBuffer[int]
    hidden: GpuBuffer[int]
    v_width: GpuBuffer[int]
    channels: GpuBuffer[int]
    history: GpuBuffer[int]
    state: GpuBuffer[int]
    ffn: GpuBuffer[int]
    kv: GpuBuffer[int]
    kv_heads: GpuBuffer[int]
    chunks: GpuBuffer[int]
    vocab_chunks: GpuBuffer[int]


def grids():
    return Grids(one=fresh_grid(1), heads16=fresh_grid(16), q_heads=fresh_grid(24), v_heads=fresh_grid(48), hidden=fresh_grid(5120), v_width=fresh_grid(6144), channels=fresh_grid(10240), history=fresh_grid(30720), state=fresh_grid(792576), ffn=fresh_grid(17408), kv=fresh_grid(1024), kv_heads=fresh_grid(4), chunks=fresh_grid(20), vocab_chunks=fresh_grid(485))


def at(b: GpuBuffer[float32], i) -> float32:
    return unwrap_or(gpuGet(b, i), toFloat32(0.0))


# Index arithmetic in a kernel: every index is in range by construction, so it uses the operations
# that cannot fault, and a kernel with no fault site stays in the open batch [LLM-DISPATCH-BATCH].
def plus(a, b):
    return wrapAdd(a, b)


def times(a, b):
    return wrapMul(a, b)


def third(i):
    """i // 3 for any index below 2^32: multiplication by the inverse, exact on that range."""
    return shiftRightLogical(wrapMul(i, 2863311531), 33)


def sixth(i):
    return shiftRightLogical(third(i), 1)


def softplus(x: float32) -> float32:
    return ln(toFloat32(1.0) + exp(x))


def rsqrt(x: float32) -> float32:
    return exp(toFloat32(-0.5) * ln(x))


def squares(x, start, n) -> float32:
    return fold(range(0, n), toFloat32(0.0), lambda a, i: a + at(x, plus(start, i)) * at(x, plus(start, i)))


class Weight:
    buffer: GpuBuffer[uint8]
    kind: str


def linear(w, x, inner, cols):
    return must(gpuMatMulQuant(x, w.buffer, w.kind, 1, inner, cols))


def added(a, b, g):
    return must(gpuMap(g, lambda i: at(a, i) + at(b, i)))


def rms_hidden(x, w, g):
    """`Qwen3_5RMSNorm` over the one hidden vector of 5120."""
    chunks: GpuBuffer[float32] = must(gpuMap(g.chunks, lambda c: squares(x, shiftLeft(c, 8), 256)))
    s: GpuBuffer[float32] = must(gpuMap(g.one, lambda _: rsqrt(fold(range(0, 20), toFloat32(0.0), lambda a, c: a + at(chunks, c)) * toFloat32(0.0001953125) + toFloat32(0.000001))))
    return must(gpuMap(g.hidden, lambda i: at(x, i) * at(s, 0) * at(w, i)))


def rms_heads(x, w, heads, cells):
    """`Qwen3_5RMSNorm` per attention head of 256."""
    s: GpuBuffer[float32] = must(gpuMap(heads, lambda h: rsqrt(squares(x, shiftLeft(h, 8), 256) * toFloat32(0.00390625) + toFloat32(0.000001))))
    return must(gpuMap(cells, lambda i: at(x, i) * at(s, shiftRightLogical(i, 8)) * at(w, bitAnd(i, 255))))


class Mlp:
    gate: Weight
    up: Weight
    down: Weight


def mlp(x, l, grids):
    g = linear(l.gate, x, 5120, 17408)
    u = linear(l.up, x, 5120, 17408)
    gate = must(gpuSigmoid(g))
    h: GpuBuffer[float32] = must(gpuMap(grids.ffn, lambda i: at(g, i) * at(gate, i) * at(u, i)))
    return linear(l.down, h, 17408, 5120)


class DeltaParams:
    norm: GpuBuffer[float32]
    post: GpuBuffer[float32]
    qkv: Weight
    z: Weight
    b: Weight
    a: Weight
    out: Weight
    conv: GpuBuffer[float32]
    a_log: GpuBuffer[float32]
    dt: GpuBuffer[float32]
    ssm_norm: GpuBuffer[float32]
    ffn: Mlp


class AttnParams:
    norm: GpuBuffer[float32]
    post: GpuBuffer[float32]
    q: Weight
    k: Weight
    v: Weight
    o: Weight
    q_norm: GpuBuffer[float32]
    k_norm: GpuBuffer[float32]
    ffn: Mlp


# Gated DeltaNet, one token, the recurrent rule: 10240 convolved channels hold 16 query heads, 16
# key heads, then 48 value heads, each of 128; every value head uses key head h % 16, the order llama.cpp's converter writes (tiled). The state is
# [48, 128, 128] and a [h, k, v] cell is (h << 14) + (k << 7) + v.
class DeltaMixed:
    x: GpuBuffer[float32]
    slot: DeltaSlot


class AttnMixed:
    x: GpuBuffer[float32]
    slot: AttnSlot


def tap(w, old, qkv, c) -> float32:
    k = shiftLeft(c, 2)
    o = times(c, 3)
    return at(w, k) * at(old, o) + at(w, plus(k, 1)) * at(old, plus(o, 1)) + at(w, plus(k, 2)) * at(old, plus(o, 2)) + at(w, plus(k, 3)) * at(qkv, c)


def convolved(qkv, old, w, g):
    tapped: GpuBuffer[float32] = must(gpuMap(g.channels, lambda c: tap(w, old, qkv, c)))
    gate = must(gpuSigmoid(tapped))
    return must(gpuMap(g.channels, lambda c: at(tapped, c) * at(gate, c)))


def conv_history(qkv, old, g):
    return must(gpuMap(g.history, lambda i: at(qkv, third(i)) if wrapSub(i, times(third(i), 3)) == 2 else at(old, plus(i, 1))))


def unit_scale(mixed, first, g):
    return must(gpuMap(g.heads16, lambda h: rsqrt(squares(mixed, plus(first, shiftLeft(h, 7)), 128) + toFloat32(0.000001))))


def scaled_heads(mixed, first, scales, g):
    """A key or query head per value head, `dim` values each, scaled to unit length."""
    return must(gpuMap(g.v_width, lambda i: at(mixed, plus(first, plus(shiftLeft(bitAnd(shiftRightLogical(i, 7), 15), 7), bitAnd(i, 127)))) * at(scales, bitAnd(shiftRightLogical(i, 7), 15))))


def delta_rule(mixed, old, b, a, a_log, dt, g):
    qs = unit_scale(mixed, 0, g)
    ks = unit_scale(mixed, 2048, g)
    decay: GpuBuffer[float32] = must(gpuMap(g.v_heads, lambda h: exp(at(a_log, h) * softplus(at(a, h) + at(dt, h)))))
    beta = must(gpuSigmoid(b))
    values: GpuBuffer[float32] = must(gpuMap(g.v_width, lambda i: at(mixed, plus(4096, i))))
    unit_q = scaled_heads(mixed, 0, qs, g)
    queries: GpuBuffer[float32] = must(gpuMap(g.v_width, lambda i: at(unit_q, i) * toFloat32(0.08838834764831845)))
    return must(gpuDeltaRule(old, queries, scaled_heads(mixed, 2048, ks, g), values, beta, decay, 48, 128))


def delta_mix(x, slot, p, g):
    qkv = linear(p.qkv, x, 5120, 10240)
    z = linear(p.z, x, 5120, 6144)
    b = linear(p.b, x, 5120, 48)
    a = linear(p.a, x, 5120, 48)
    mixed = convolved(qkv, slot.conv, p.conv, g)
    state = delta_rule(mixed, slot.state, b, a, p.a_log, p.dt, g)
    out: GpuBuffer[float32] = must(gpuMap(g.v_width, lambda i: at(state, plus(786432, i))))
    w = p.ssm_norm
    s: GpuBuffer[float32] = must(gpuMap(g.v_heads, lambda h: rsqrt(squares(out, shiftLeft(h, 7), 128) * toFloat32(0.0078125) + toFloat32(0.000001))))
    gate = must(gpuSigmoid(z))
    gated: GpuBuffer[float32] = must(gpuMap(g.v_width, lambda i: at(out, i) * at(s, shiftRightLogical(i, 7)) * at(w, bitAnd(i, 127)) * (at(z, i) * at(gate, i))))
    return DeltaMixed(x=linear(p.out, gated, 6144, 5120), slot=DeltaSlot(conv=conv_history(qkv, slot.conv, g), state=state))


# Gated attention, one token at position `pos`, `pos` entries cached: 24 query heads over 4
# key/value heads of 256, each query head reading key head h // 6, rotary over the first 64.
# Scores are [position, head], so a kernel never divides by the context length.
THETA = 10000000.0
NEW_TOKENS = 8


def freq(j):
    return 1.0 / pow(THETA, toFloat(2 * (j % 32)) / 64.0)


def angle_table(pos, of):
    return must(toGpu(toList(map(range(64), lambda j: toFloat32(of(toFloat(pos) * freq(j)))))))


def half_turned(x, i) -> float32:
    return toFloat32(0.0) - at(x, plus(i, 32)) if bitAnd(i, 255) < 32 else at(x, wrapSub(i, 32))


def rotated(x, c, s, cells):
    return must(gpuMap(cells, lambda i: at(x, i) if bitAnd(i, 255) >= 64 else at(x, i) * at(c, bitAnd(i, 255)) + half_turned(x, i) * at(s, bitAnd(i, 255))))


def grown(old, count, fresh):
    edge = count * 1024
    return must(gpuMap(fresh_grid(edge + 1024), lambda i: at(old, i) if i < edge else at(fresh, wrapSub(i, edge))))


def score(queries, keys, h, t) -> float32:
    return fold(range(0, 256), toFloat32(0.0), lambda a, d: a + at(queries, plus(shiftLeft(h, 8), d)) * at(keys, plus(shiftLeft(t, 10), plus(shiftLeft(sixth(h), 8), d)))) * toFloat32(0.0625)


def peak_of(scores, h, n) -> float32:
    return fold(range(0, n), at(scores, h), lambda a, t: a if a > at(scores, plus(times(t, 24), h)) else at(scores, plus(times(t, 24), h)))


def total_of(raw, h, n) -> float32:
    return fold(range(0, n), toFloat32(0.0), lambda a, t: a + at(raw, plus(times(t, 24), h)))


def attended(raw, inverse, values, n, h, d) -> float32:
    return fold(range(0, n), toFloat32(0.0), lambda a, t: a + (at(raw, plus(times(t, 24), h)) * at(inverse, h)) * at(values, plus(shiftLeft(t, 10), plus(shiftLeft(sixth(h), 8), d))))


def attend(queries, keys, values, n, g):
    cells = fresh_grid(24 * n)
    scores: GpuBuffer[float32] = must(gpuMap(cells, lambda i: score(queries, keys, wrapSub(i, times(shiftRightLogical(third(i), 3), 24)), shiftRightLogical(third(i), 3))))
    peaks: GpuBuffer[float32] = must(gpuMap(g.q_heads, lambda h: peak_of(scores, h, n)))
    raw: GpuBuffer[float32] = must(gpuMap(cells, lambda i: exp(at(scores, i) - at(peaks, wrapSub(i, times(shiftRightLogical(third(i), 3), 24))))))
    inverse: GpuBuffer[float32] = must(gpuMap(g.q_heads, lambda h: exp(toFloat32(0.0) - ln(total_of(raw, h, n)))))
    return must(gpuMap(g.v_width, lambda i: attended(raw, inverse, values, n, shiftRightLogical(i, 8), bitAnd(i, 255))))


def attn_mix(x, slot, p, pos, g):
    both = linear(p.q, x, 5120, 12288)
    raw_q: GpuBuffer[float32] = must(gpuMap(g.v_width, lambda i: at(both, plus(shiftLeft(shiftRightLogical(i, 8), 9), bitAnd(i, 255)))))
    c = angle_table(pos, lambda a: cos(a))
    s = angle_table(pos, lambda a: sin(a))
    queries = rotated(rms_heads(raw_q, p.q_norm, g.q_heads, g.v_width), c, s, g.v_width)
    fresh_k = rotated(rms_heads(linear(p.k, x, 5120, 1024), p.k_norm, g.kv_heads, g.kv), c, s, g.kv)
    keys = grown(slot.keys, pos, fresh_k)
    values = grown(slot.values, pos, linear(p.v, x, 5120, 1024))
    out = attend(queries, keys, values, pos + 1, g)
    gates: GpuBuffer[float32] = must(gpuMap(g.v_width, lambda i: at(both, plus(shiftLeft(shiftRightLogical(i, 8), 9), plus(256, bitAnd(i, 255))))))
    gate = must(gpuSigmoid(gates))
    gated: GpuBuffer[float32] = must(gpuMap(g.v_width, lambda i: at(out, i) * at(gate, i)))
    return AttnMixed(x=linear(p.o, gated, 6144, 5120), slot=AttnSlot(keys=keys, values=values))


class Model:
    deltas: list[DeltaParams]
    attns: list[AttnParams]
    final: GpuBuffer[float32]
    head: Weight
    basis: GpuBuffer[float32]
    grids: Grids
    vocab: GpuBuffer[int]


def attn_layer(walk, p, slot, pos, g):
    r = attn_mix(rms_hidden(walk.x, p.norm, g), slot, p, pos, g)
    y = added(walk.x, r.x, g.hidden)
    return Walk(x=added(y, mlp(rms_hidden(y, p.post, g), p.ffn, g), g.hidden), deltas=walk.deltas, attns=listAppend(walk.attns, r.slot))


def delta_layer(walk, p, slot, g):
    r = delta_mix(rms_hidden(walk.x, p.norm, g), slot, p, g)
    y = added(walk.x, r.x, g.hidden)
    return Walk(x=added(y, mlp(rms_hidden(y, p.post, g), p.ffn, g), g.hidden), deltas=listAppend(walk.deltas, r.slot), attns=walk.attns)


def attn_at(walk, model, old, pos, i):
    match listGet(model.attns, i // 4):
        case Success(value=p):
            match listGet(old.attns, i // 4):
                case Success(value=slot):
                    return attn_layer(walk, p, slot, pos, model.grids)
                case Error(message=m):
                    return perform(Fault.halt_walk(toString(m)))
        case Error(message=m):
            return perform(Fault.halt_walk(toString(m)))


def delta_at(walk, model, old, i):
    d = i - i // 4
    match listGet(model.deltas, d):
        case Success(value=p):
            match listGet(old.deltas, d):
                case Success(value=slot):
                    return delta_layer(walk, p, slot, model.grids)
                case Error(message=m):
                    return perform(Fault.halt_walk(toString(m)))
        case Error(message=m):
            return perform(Fault.halt_walk(toString(m)))


def embed(model, token):
    match gpuMapFile(GGUF, EMBED_AT + token * EMBED_ROW, EMBED_ROW):
        case Success(value=row):
            return must(gpuMatMulQuant(model.basis, row, "Q4_K", 5120, 5120, 1))
        case Error(message=m):
            return perform(Fault.halt(toString(m)))


def forward(model, old, token, pos):
    start = Walk(x=embed(model, token), deltas=[], attns=[])
    return fold(range(0, 64), start, lambda w, i: attn_at(w, model, old, pos, i) if i % 4 == 3 else delta_at(w, model, old, i))


def chunk_best(logits, c) -> float32:
    """The first index holding chunk c's peak, of 512 logits, as a float32: exact below 2^24."""
    first = shiftLeft(c, 9)
    peak = fold(range(0, 512), at(logits, first), lambda a, i: a if a > at(logits, plus(first, i)) else at(logits, plus(first, i)))
    return fold(range(0, 512), toFloat32(0.0) - toFloat32(1.0), lambda a, i: toFloat32(plus(first, i)) if at(logits, plus(first, i)) == peak and a < toFloat32(0.0) else a)


def best_token(logits, g):
    """Argmax over 248320 logits, in 485 chunks of 512, a thread each, then over their winners on the host."""
    winners: GpuBuffer[float32] = must(gpuMap(g.vocab_chunks, lambda c: chunk_best(logits, c)))
    indices = fold(range(0, 485), [], lambda acc, c: listAppend(acc, unwrap_or(floatToInt(at(winners, c)), 0)))
    return fold(range(0, 485), 0, lambda best, c: unwrap_or(listGet(indices, c), 0) if at(logits, unwrap_or(listGet(indices, c), 0)) > at(logits, best) else best)


# ---- Loading from the tensor table.
def named(name):
    table = entries()
    return fold(range(0, len(table)), Entry(name="", offset=0, size=0, kind=""), lambda hit, i: unwrap_or(listGet(table, i), hit) if unwrap_or(listGet(table, i), hit).name == name else hit)


def dense(name):
    e = named(name)
    return must(gpuMapFile(GGUF, e.offset, e.size // 4))


def weight(name):
    e = named(name)
    match gpuMapFile(GGUF, e.offset, e.size):
        case Success(value=b):
            return Weight(buffer=b, kind=e.kind)
        case Error(message=m):
            return Weight(buffer=perform(Fault.halt_bytes(toString(m))), kind=e.kind)


def blk(i, rest):
    return "blk." + toString(i) + "." + rest


def ffn(i):
    return Mlp(gate=weight(blk(i, "ffn_gate.weight")), up=weight(blk(i, "ffn_up.weight")), down=weight(blk(i, "ffn_down.weight")))


def delta_params(i):
    return DeltaParams(norm=dense(blk(i, "attn_norm.weight")), post=dense(blk(i, "post_attention_norm.weight")), qkv=weight(blk(i, "attn_qkv.weight")), z=weight(blk(i, "attn_gate.weight")), b=weight(blk(i, "ssm_beta.weight")), a=weight(blk(i, "ssm_alpha.weight")), out=weight(blk(i, "ssm_out.weight")), conv=dense(blk(i, "ssm_conv1d.weight")), a_log=dense(blk(i, "ssm_a")), dt=dense(blk(i, "ssm_dt.bias")), ssm_norm=dense(blk(i, "ssm_norm.weight")), ffn=ffn(i))


def attn_params(i):
    return AttnParams(norm=dense(blk(i, "attn_norm.weight")), post=dense(blk(i, "post_attention_norm.weight")), q=weight(blk(i, "attn_q.weight")), k=weight(blk(i, "attn_k.weight")), v=weight(blk(i, "attn_v.weight")), o=weight(blk(i, "attn_output.weight")), q_norm=dense(blk(i, "attn_q_norm.weight")), k_norm=dense(blk(i, "attn_k_norm.weight")), ffn=ffn(i))


def load():
    g = grids()
    basis: GpuBuffer[float32] = must(gpuMap(fresh_grid(5120 * 5120), lambda i: toFloat32(1.0) if i % 5121 == 0 else toFloat32(0.0)))
    return Model(deltas=fold(range(0, 64), [], lambda acc, i: acc if i % 4 == 3 else listAppend(acc, delta_params(i))), attns=fold(range(0, 16), [], lambda acc, c: listAppend(acc, attn_params(c * 4 + 3))), final=dense("output_norm.weight"), head=weight("output.weight"), basis=basis, grids=g, vocab=fresh_grid(248320))


def fresh(g):
    conv: GpuBuffer[float32] = must(gpuMap(g.history, lambda _: toFloat32(0.0)))
    state: GpuBuffer[float32] = must(gpuMap(g.state, lambda _: toFloat32(0.0)))
    empty: GpuBuffer[float32] = must(gpuMap(g.kv, lambda _: toFloat32(0.0)))
    return Walk(x=empty, deltas=toList(map(range(0, 48), lambda _: DeltaSlot(conv=conv, state=state))), attns=toList(map(range(0, 16), lambda _: AttnSlot(keys=empty, values=empty))))


class Run:
    cache: Walk
    best: int


def step(model, state, pos):
    """One token through the model: the prompt's next token, else the last best."""
    token = unwrap_or(listGet(PROMPT, pos), state.best)
    walk = forward(model, state.cache, token, pos)
    logits = linear(model.head, rms_hidden(walk.x, model.final, model.grids), 5120, VOCAB)
    best = best_token(logits, model.grids)
    print(f"{pos} {token} -> {best}")
    return Run(cache=walk, best=best)


def run():
    model = load()
    print("loaded")
    final = fold(range(0, len(PROMPT) + NEW_TOKENS), Run(cache=fresh(model.grids), best=0), lambda state, i: step(model, state, i))
    return final.best


@handler(Arith)
class wrapping:
    """Every index is in range by construction, so these arms never run: a kernel's own
    faults report through the device's fault word."""
    def overflow(_op, _l, _r, wrapped):
        return wrapped

    def divideByZero(_op, l):
        return toFloat(l)

    def remainderByZero(l):
        return l


def wrapped_run():
    with wrapping:
        return run()


def guarded() -> Result[int, str]:
    with strictly:
        return Success(value=wrapped_run())


def main():
    match guarded():
        case Success(value=n):
            print(f"done {n}")
        case Error(message=m):
            print(f"failed: {m}")
