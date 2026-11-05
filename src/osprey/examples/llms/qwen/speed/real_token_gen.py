"""Writes real_token.osp: one decode step's quantized products over tensors of the real
GGUF, for the chosen layers plus the output head, in each layer's real shapes and types.
Usage: real_token_gen.py <gguf> <tokens> <layer> [layer ...] > real_token.osp
Run with the oracle venv: it needs the `gguf` package. The file stays mapped, read-only."""
import sys
import gguf

PRODUCTS = {
    "attn_q", "attn_k", "attn_v", "attn_output", "attn_gate", "attn_qkv",
    "ssm_alpha", "ssm_beta", "ssm_out", "ffn_gate", "ffn_up", "ffn_down",
}


def lines(path, tokens, layers):
    reader = gguf.GGUFReader(path)
    yield f'let path = "{path}"'
    yield "fn mapped(offset, bytes) -> Result<GpuBuffer<uint8>, Error> = gpuMapFile(path, offset, bytes)"
    yield "fn ramp(n) -> Result<GpuBuffer<float32>, Error> = match gpuIota(n) {"
    yield "    Success { value } => gpuMap(value, fn(i) => toFloat32(i % 7) * toFloat32(0.25) - toFloat32(0.75))"
    yield "    Error { message } => Error { message: message }"
    yield "}"
    yield "fn failed(message, x) = {"
    yield "    print(message)"
    yield "    x"
    yield "}"
    yield "fn product(x, offset, bytes, kind, inner, cols) = match mapped(offset, bytes) {"
    yield "    Success { value } => match gpuMatMulQuant(x, value, kind, 1, inner, cols) {"
    yield "        Success { value } => value"
    yield "        Error { message } => failed(message, x)"
    yield "    }"
    yield "    Error { message } => failed(message, x)"
    yield "}"
    yield "fn step(a, b, c) = {"
    count = 0
    for t in reader.tensors:
        parts = t.name.split(".")
        wanted = t.name == "output.weight" or (parts[0] == "blk" and int(parts[1]) in layers and parts[2] in PRODUCTS)
        if wanted:
            inner, cols = int(t.shape[0]), int(t.shape[1])
            x = {5120: "a", 6144: "b", 17408: "c"}[inner]
            yield f'    let p{count} = product({x}, {t.data_offset}, {t.n_bytes}, "{t.tensor_type.name}", {inner}, {cols})'
            count += 1
    yield f"    toFloat(gpuGet(p{count - 1}, 0) ?: toFloat32(0.0))"
    yield "}"
    yield "fn with17408(a, b, c) = print(\"products: ${fold(range(0, %d), 0.0, fn(acc, _t) => acc + step(a, b, c))}\")" % tokens
    yield "fn with6144(a, b) = match ramp(17408) {"
    yield "    Error { message } => print(message)"
    yield "    Success { value } => with17408(a, b, value)"
    yield "}"
    yield "fn with5120(a) = match ramp(6144) {"
    yield "    Error { message } => print(message)"
    yield "    Success { value } => with6144(a, value)"
    yield "}"
    yield "fn run() = match ramp(5120) {"
    yield "    Error { message } => print(message)"
    yield "    Success { value } => with5120(value)"
    yield "}"
    yield "fn main() = {"
    yield "    handle Arith { overflow _ _ _ wrapped => wrapped }"
    yield "    run()"
    yield "}"


if __name__ == "__main__":
    print("\n".join(lines(sys.argv[1], int(sys.argv[2]), {int(n) for n in sys.argv[3:]})))
