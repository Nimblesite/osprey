use super::{any, i, mono, poly, res, runtime_mono, s, u, Type, TypeEnv};
use crate::arithmetic::{open_float_scheme, open_integer_scheme, open_numeric_scheme};
use crate::numeric::Numeric;

/// `toInt8` for `int8`: the builtin named for a numeric type.
fn conversion_name(target: Numeric) -> String {
    let name = target.name();
    let (first, rest) = name.split_at(1);
    format!("to{}{rest}", first.to_uppercase())
}

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
    // Open over the integer types [TYPE-NUMERIC-ARITH-WIDTH], one overload per
    // use like `abs`.
    for name in [
        "intDiv",
        "wrapAdd",
        "wrapSub",
        "wrapMul",
        "satAdd",
        "satSub",
        "satMul",
        "bitAnd",
        "bitOr",
        "bitXor",
        "shiftLeft",
        "shiftRight",
        "shiftRightLogical",
    ] {
        e.insert(name, open_integer_scheme(name, 2, Type::Var(0)));
    }
    e.insert("bitNot", open_integer_scheme("bitNot", 1, Type::Var(0)));
    // Conversions named for their target, over any integer source
    // [BUILTIN-CONVERT-INT]; out of range performs `Arith.overflow`.
    for target in Numeric::ALL.into_iter().filter(|n| n.is_integer()) {
        let name = conversion_name(target);
        e.insert(
            &name,
            open_integer_scheme(&name, 1, Type::prim(target.name())),
        );
    }
    // Every numeric source converts to the target's float width. Total, so no
    // Result [BUILTIN-CONVERT-FLOAT] [GPU-CONVERT].
    for target in Numeric::ALL.into_iter().filter(|n| n.is_float()) {
        let name = conversion_name(target);
        e.insert(
            &name,
            open_numeric_scheme(&name, 1, Type::prim(target.name())),
        );
    }
    // The unary float functions every numeric kernel is written from. Total
    // over IEEE doubles (a domain error yields NaN, an overflow infinity), so
    // bare `float`; pure, so a kernel may call or name them [BUILTIN-MATH],
    // [BUILTIN-MATH-TRIG], [BUILTIN-MATH-HYPERBOLIC], [BUILTIN-MATH-LOG],
    // [BUILTIN-MATH-ROUNDING], [BUILTIN-MATH-SPECIAL].
    for name in crate::arithmetic::UNARY_FLOAT_BUILTINS {
        e.insert(name, open_float_scheme(name, 1, Type::Var(0)));
    }
    // Total binary float functions [BUILTIN-MATH-ROUNDING], [BUILTIN-MATH-TRIG],
    // [BUILTIN-MATH-LOG]: C pow/atan2/hypot/fmod/copysign semantics, so no
    // Arith operation and no Result.
    for name in crate::arithmetic::BINARY_FLOAT_BUILTINS {
        e.insert(name, open_float_scheme(name, 2, Type::Var(0)));
    }
    // Bit-level views at each width [BUILTIN-FLOAT-BITS]. All total and pure.
    mono(e, "floatBits", vec![Type::float()], i());
    mono(e, "floatFromBits", vec![i()], Type::float());
    mono(
        e,
        "float32Bits",
        vec![Type::prim("float32")],
        Type::prim("int32"),
    );
    mono(
        e,
        "float32FromBits",
        vec![Type::prim("int32")],
        Type::prim("float32"),
    );
    mono(
        e,
        "float16Bits",
        vec![Type::prim("float16")],
        Type::prim("int16"),
    );
    mono(
        e,
        "float16FromBits",
        vec![Type::prim("int16")],
        Type::prim("float16"),
    );
    // Checked truncation float → int [BUILTIN-FLOATTOINT]: NaN, ±inf and
    // magnitudes outside the i64 range are Error, so it is a Result and not a
    // kernel (is_gpu_scalar rejects the Error return).
    e.insert("floatToInt", open_float_scheme("floatToInt", 1, res(i())));
    // Named equivalents of the overflow-checked integer operators, carrying the
    // runtime builtins' generic Error channel.
    for checked in ["checkedAdd", "checkedSub", "checkedMul"] {
        e.insert(checked, open_integer_scheme(checked, 2, res(Type::Var(0))));
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
