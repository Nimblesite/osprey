use super::{any, i, mono, res, runtime_mono, s, u, Type, TypeEnv};

pub(super) fn declare(e: &mut TypeEnv) {
    runtime_mono(e, "print", vec![any()], u());
    runtime_mono(e, "input", vec![], s());
    mono(e, "toString", vec![any()], s());
    mono(e, "length", vec![any()], i());
    // [CONCURRENCY-SLEEP] The native status is not part of the Unit surface.
    runtime_mono(e, "sleep", vec![i()], u());
    // A range is a fused iterator handle, not a materialized List [BUILTIN-ITER].
    mono(e, "range", vec![i(), i()], Type::iterator(i()));
    numbers(e);
    randomness(e);
}

fn numbers(e: &mut TypeEnv) {
    e.insert("abs", crate::arithmetic::absolute_scheme());
    mono(e, "intDiv", vec![i(), i()], i());
    for name in [
        "wrapAdd", "wrapSub", "wrapMul", "satAdd", "satSub", "satMul",
    ] {
        mono(e, name, vec![i(), i()], i());
    }
    // Widening int → float. Total, so it is bare `float` rather than a Result:
    // every i64 has a nearest double. Implements [BUILTIN-TOFLOAT] and the GPU
    // surface's explicit element conversion [GPU-CONVERT].
    mono(e, "toFloat", vec![i()], Type::float());
    // Named equivalents of the overflow-checked integer operators. These retain
    // the runtime builtins' generic Error channel for compatibility.
    for checked in ["checkedAdd", "checkedSub", "checkedMul"] {
        mono(e, checked, vec![i(), i()], res(i()));
    }
}

fn randomness(e: &mut TypeEnv) {
    // Cryptographically-secure randomness (random_runtime.c). `random` yields a
    // uniform non-negative int; `randomBelow(n)` an unbiased int in [0, n),
    // Error when n <= 0. Implements [BUILTIN-RANDOM], [BUILTIN-RANDOM-BELOW].
    mono(e, "random", vec![], i());
    mono(e, "randomBelow", vec![i()], res(i()));
}
