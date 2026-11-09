//! Fiber and channel built-ins ([CONCURRENCY-SPAWN-AWAIT], [CONCURRENCY-YIELD]).

use super::{i, mono, poly, u, Type, TypeEnv};

pub(super) fn declare(e: &mut TypeEnv) {
    let t = || Type::Var(0);
    // Fiber operations [CONCURRENCY-SPAWN-AWAIT], [CONCURRENCY-YIELD].
    // await : (Fiber<t>) -> t
    poly(
        e,
        "await",
        vec![0],
        vec![Type::con("Fiber", vec![t()])],
        t(),
    );
    poly(
        e,
        "fiberDone",
        vec![0],
        vec![Type::con("Fiber", vec![t()])],
        i(),
    );
    mono(e, "yield", vec![], u());
    mono(e, "fiber_yield", vec![i()], i());
    // Channel<t>: create with a buffer size, send/recv values.
    poly(
        e,
        "Channel",
        vec![0],
        vec![i()],
        Type::con("Channel", vec![t()]),
    );
    poly(
        e,
        "send",
        vec![0],
        vec![Type::con("Channel", vec![t()]), t()],
        u(),
    );
    poly(
        e,
        "recv",
        vec![0],
        vec![Type::con("Channel", vec![t()])],
        t(),
    );
}
