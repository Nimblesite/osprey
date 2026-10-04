use super::{b, i, mono, poly, res, s, Type, TypeEnv};

/// The GPU computation surface (docs/specs/0034-GPUComputation.md). Element
/// scalarity is a call-site representation constraint [GPU-BUFFER-ELEM]
/// (`builtin_constraints`); kernel purity is proven by the effect checker
/// [GPU-KERNEL-PURE] (`effect_rows`).
pub(super) fn declare(e: &mut TypeEnv) {
    let t = || Type::Var(0);
    let v = || Type::Var(1);
    let buf_t = || Type::gpu_buffer(t());
    // [GPU-BUFFER-FROM-LIST] / [GPU-BUFFER-TO-LIST] / [GPU-BUFFER-LENGTH]
    poly(e, "toGpu", vec![0], vec![Type::list(t())], buf_t());
    poly(e, "fromGpu", vec![0], vec![buf_t()], Type::list(t()));
    poly(e, "gpuLength", vec![0], vec![buf_t()], i());
    // [GPU-MAP] / [GPU-FOLD]
    poly(
        e,
        "gpuMap",
        vec![0, 1],
        vec![buf_t(), Type::fun(vec![t()], v())],
        Type::gpu_buffer(v()),
    );
    poly(
        e,
        "gpuFold",
        vec![0, 1],
        vec![buf_t(), v(), Type::fun(vec![v(), t()], v())],
        v(),
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
        Type::gpu_buffer(w()),
    );
    // [GPU-IOTA] The index buffer: gather, stencil, and matrix addressing all
    // start from element indices.
    mono(e, "gpuIota", vec![i()], Type::gpu_buffer(i()));
    // [GPU-GET] Bounds-checked indexed read, usable inside a kernel to gather.
    poly(e, "gpuGet", vec![0], vec![buf_t(), i()], res(t()));
    // [GPU-SCAN] Inclusive prefix scan — the classic parallel primitive.
    poly(
        e,
        "gpuScan",
        vec![0],
        vec![buf_t(), t(), Type::fun(vec![t(), t()], t())],
        buf_t(),
    );
    // [GPU-FILTER] Stream compaction.
    poly(
        e,
        "gpuFilter",
        vec![0],
        vec![buf_t(), Type::fun(vec![t()], b())],
        buf_t(),
    );
    // [GPU-DEVICE] The active execution backend's name. Selection between
    // devices arrives with the `Gpu` effect at roadmap stage 5.
    mono(e, "gpuDevice", vec![], s());
}
