use super::{b, i, mono, poly, res, s, Type, TypeEnv};

/// The GPU computation surface (docs/specs/runtime/0034-GPUComputation.md). Element
/// scalarity is a call-site representation constraint [GPU-BUFFER-ELEM]
/// (`builtin_constraints`); kernel purity is proven by the effect checker
/// [GPU-KERNEL-PURE] (`effect_rows`).
pub(super) fn declare(e: &mut TypeEnv) {
    let t = || Type::Var(0);
    let v = || Type::Var(1);
    let buf_t = || Type::gpu_buffer(t());
    // [GPU-BUFFER-FROM-LIST] / [GPU-BUFFER-TO-LIST] / [GPU-BUFFER-LENGTH]
    poly(e, "toGpu", vec![0], vec![Type::list(t())], res(buf_t()));
    poly(e, "fromGpu", vec![0], vec![buf_t()], res(Type::list(t())));
    poly(e, "gpuLength", vec![0], vec![buf_t()], i());
    // [GPU-MAP] / [GPU-FOLD]
    poly(
        e,
        "gpuMap",
        vec![0, 1],
        vec![buf_t(), Type::fun(vec![t()], v())],
        res(Type::gpu_buffer(v())),
    );
    poly(
        e,
        "gpuFold",
        vec![0, 1],
        vec![buf_t(), v(), Type::fun(vec![v(), t()], v())],
        res(v()),
    );
    // [GPU-ZIPWITH] Elementwise binary combination of two buffers — the
    // primitive every vector, tensor, and particle workload needs.
    let w = || Type::Var(2);
    poly(
        e,
        "gpuZipWith",
        vec![0, 1, 2],
        vec![
            buf_t(),
            Type::gpu_buffer(v()),
            Type::fun(vec![t(), v()], w()),
        ],
        res(Type::gpu_buffer(w())),
    );
    // [GPU-IOTA] The index buffer: gather, stencil, and matrix addressing all
    // start from element indices.
    mono(e, "gpuIota", vec![i()], res(Type::gpu_buffer(i())));
    // [GPU-GET] Bounds-checked indexed read, usable inside a kernel to gather.
    poly(e, "gpuGet", vec![0], vec![buf_t(), i()], res(t()));
    // [GPU-SCAN] Inclusive prefix scan — the classic parallel primitive.
    poly(
        e,
        "gpuScan",
        vec![0],
        vec![buf_t(), t(), Type::fun(vec![t(), t()], t())],
        res(buf_t()),
    );
    // [GPU-FILTER] Stream compaction.
    poly(
        e,
        "gpuFilter",
        vec![0],
        vec![buf_t(), Type::fun(vec![t()], b())],
        res(buf_t()),
    );
    // [GPU-MATMUL] The dense row-major product of two float buffers at a
    // caller-stated shape — one float type for all three buffers, `float` or
    // `float32`, fixed by this one variable; a length that disagrees with the
    // shape is an Error.
    let product = || Type::gpu_buffer(t());
    let mut matrix = super::Scheme::poly(
        vec![0],
        Type::fun(vec![product(), product(), i(), i(), i()], res(product())),
    );
    matrix.obligations = vec![("matrix gpuMatMul".into(), product())];
    e.insert("gpuMatMul", matrix);
    // [LLM-MATMUL-QUANT] A [rows, inner] float32 matrix against `cols` rows of
    // quantized weights in a GGUF block format named by `type` ("Q4_K"), as the
    // [rows, cols] float32 product; a length or type the shape disagrees with
    // is an Error.
    let f32 = || Type::gpu_buffer(Type::prim("float32"));
    mono(
        e,
        "gpuMatMulQuant",
        vec![f32(), Type::gpu_buffer(Type::prim("uint8")), s(), i(), i(), i()],
        res(f32()),
    );
    // [GPU-BUFFER-LOAD] Raw little-endian binary32 words at a byte offset,
    // byte for byte with no widening, as a `float32` buffer.
    mono(
        e,
        "gpuLoad",
        vec![s(), i(), i()],
        res(Type::gpu_buffer(Type::prim("float32"))),
    );
    // [GPU-BUFFER-TENSOR] One named tensor of a safetensors file, at the float
    // type `T` the program's own use of the result pins — F64 into `float`,
    // F32 and BF16 into `float32`, F16 into `float16`; never the file's dtype.
    let mut loader = super::Scheme::poly(vec![0], Type::fun(vec![s(), s()], res(buf_t())));
    loader.obligations = vec![("float gpuLoadTensor".into(), t())];
    e.insert("gpuLoadTensor", loader);
    // [LLM-WEIGHTS-MAPPED] A read-only mapped file region at the numeric type
    // `T` the program's use pins; the file stays the only copy.
    let mut mapper = super::Scheme::poly(vec![0], Type::fun(vec![s(), i(), i()], res(buf_t())));
    mapper.obligations = vec![("numeric gpuMapFile".into(), t())];
    e.insert("gpuMapFile", mapper);
    // [GPU-DEVICE] The active execution backend's name. Selection between
    // devices arrives with the `Gpu` effect at roadmap stage 5.
    mono(e, "gpuDevice", vec![], s());
}
