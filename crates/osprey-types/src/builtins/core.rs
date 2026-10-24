use super::{any, i, mono, poly, res, runtime_mono, s, u, Type, TypeEnv};

pub(super) fn declare(e: &mut TypeEnv) {
    runtime_mono(e, "print", vec![any()], u());
    runtime_mono(e, "input", vec![], s());
    mono(e, "toString", vec![any()], s());
    mono(e, "length", vec![any()], i());
    // [CONCURRENCY-SLEEP] The native status is not part of the Unit surface.
    runtime_mono(e, "sleep", vec![i()], u());
    // A range is a fused iterator handle, not a materialized List [BUILTIN-ITER].
    mono(e, "range", vec![i(), i()], Type::iterator(i()));
    signals(e);
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
    // The unary float functions every numeric kernel is written from. Total
    // over IEEE doubles (a domain error yields NaN, an overflow infinity), so
    // bare `float`; pure, so a kernel may call or name them [BUILTIN-MATH],
    // [BUILTIN-MATH-TRIG], [BUILTIN-MATH-HYPERBOLIC], [BUILTIN-MATH-LOG],
    // [BUILTIN-MATH-ROUNDING], [BUILTIN-MATH-SPECIAL].
    for name in [
        "sqrt", "exp", "ln", "tanh", "sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh",
        "asinh", "acosh", "atanh", "log2", "log10", "log1p", "expm1", "erf", "erfc", "floor",
        "ceil", "trunc", "round",
    ] {
        mono(e, name, vec![Type::float()], Type::float());
    }
    // Total binary float functions [BUILTIN-MATH-ROUNDING], [BUILTIN-MATH-TRIG],
    // [BUILTIN-MATH-LOG]: C pow/atan2/hypot/fmod/copysign semantics, so no
    // Arith operation and no Result.
    for name in ["pow", "atan2", "hypot", "fmod", "copysign"] {
        mono(e, name, vec![Type::float(), Type::float()], Type::float());
    }
    // Binary64 → binary32 emulation and the bit-level views of both formats
    // [BUILTIN-FLOAT32], [BUILTIN-FLOAT-BITS]. All total and pure.
    mono(e, "float32", vec![Type::float()], Type::float());
    mono(e, "floatBits", vec![Type::float()], i());
    mono(e, "floatFromBits", vec![i()], Type::float());
    mono(e, "float32Bits", vec![Type::float()], i());
    mono(e, "float32FromBits", vec![i()], Type::float());
    // Checked truncation float → int [BUILTIN-FLOATTOINT]: NaN, ±inf and
    // magnitudes outside the i64 range are Error, so it is a Result and not a
    // kernel (is_gpu_scalar rejects the Error return).
    mono(e, "floatToInt", vec![Type::float()], res(i()));
    // Bitwise integer builtins [BUILTIN-BITWISE]. Total — a shift count
    // outside 0..63 is the limit of repeated shifting, never a fault and
    // never an Arith operation.
    for name in ["bitAnd", "bitOr", "bitXor"] {
        mono(e, name, vec![i(), i()], i());
    }
    mono(e, "bitNot", vec![i()], i());
    for name in ["shiftLeft", "shiftRight", "shiftRightLogical"] {
        mono(e, name, vec![i(), i()], i());
    }
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

fn signals(e: &mut TypeEnv) {
    let work = Type::fun(vec![], Type::Var(0));
    let snapshot = Type::signal_snapshot(Type::Var(0));
    poly(
        e,
        "signalDependencies",
        vec![0],
        vec![work.clone()],
        Type::list(s()),
    );
    poly(
        e,
        "signalSnapshot",
        vec![0],
        vec![work.clone()],
        snapshot.clone(),
    );
    poly(
        e,
        "signalRefresh",
        vec![0],
        vec![snapshot.clone(), Type::list(s()), work],
        snapshot,
    );
}
