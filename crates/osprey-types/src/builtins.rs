//! The canonical table of built-in function signatures. Each entry is a name
//! bound to a (possibly polymorphic) scheme. Polymorphic schemes use `Var(0)`,
//! `Var(1)` as their quantified variables; `instantiate` renames them per use,
//! so the concrete ids only need to be self-consistent within one scheme.
//!
//! Signatures favour `any` where the runtime accepts heterogeneous input
//! (`print`, `toString`, `length`) and stay precise where a wrong type is a
//! genuine bug. Result-returning builtins return `Result<T, Error>` — the
//! shape the C runtime actually returns — so the match/auto-unwrap paths agree
//! with the expected outputs in `examples/tested`.

use crate::env::TypeEnv;
use crate::ty::{Scheme, Type};

fn s() -> Type {
    Type::string()
}
fn i() -> Type {
    Type::int()
}
fn b() -> Type {
    Type::bool()
}
fn u() -> Type {
    Type::unit()
}
fn any() -> Type {
    Type::any()
}
fn err() -> Type {
    Type::prim("Error")
}
fn res(ok: Type) -> Type {
    Type::result(ok, err())
}

/// How many low variable ids the builtin schemes below use as hand-written
/// quantified binders (`Var(0)`, `Var(1)`). The checker's fresh-variable
/// supply must never allocate these ids as live inference variables: a
/// collision lets user unification bind an id that `TypeEnv::free_vars` then
/// resolves *through* a builtin's binder, making a user variable look
/// free-in-env and silently blocking let-generalization — e.g.
/// `fn identity<T>(x) -> T = x` losing its polymorphism depending on which
/// direction a var-var unification happened to bind. [TYPE-GENERICS-FN]
pub const RESERVED_SCHEME_VARS: u32 = 2;

fn mono(env: &mut TypeEnv, name: &str, params: Vec<Type>, ret: Type) {
    env.insert(name, Scheme::mono(Type::fun(params, ret)));
}

fn poly(env: &mut TypeEnv, name: &str, vars: Vec<u32>, params: Vec<Type>, ret: Type) {
    env.insert(name, Scheme::poly(vars, Type::fun(params, ret)));
}

/// Install every built-in into a base environment.
pub fn base_env() -> TypeEnv {
    let mut e = TypeEnv::new();
    core(&mut e);
    strings(&mut e);
    functional(&mut e);
    lists(&mut e);
    files(&mut e);
    http(&mut e);
    json(&mut e);
    concurrency(&mut e);
    websocket(&mut e);
    terminal(&mut e);
    e
}

fn core(e: &mut TypeEnv) {
    mono(e, "print", vec![any()], u());
    mono(e, "input", vec![], s());
    mono(e, "toString", vec![any()], s());
    mono(e, "length", vec![any()], i());
    mono(e, "sleep", vec![i()], u());
    // range(start, end) -> List<int>
    mono(e, "range", vec![i(), i()], Type::list(i()));
    mono(e, "abs", vec![i()], i());
    // Truncating integer division, divide-by-zero-checked → Result<int, MathError>.
    // The `/` operator is float-only (Osprey spec); this is its integer sibling.
    // Implements [BUILTIN-INTDIV].
    mono(e, "intDiv", vec![i(), i()], res(i()));
    // Cryptographically-secure randomness (random_runtime.c). `random` yields a
    // uniform non-negative int; `randomBelow(n)` an unbiased int in [0, n),
    // Error when n <= 0. Implements [BUILTIN-RANDOM], [BUILTIN-RANDOM-BELOW].
    mono(e, "random", vec![], i());
    mono(e, "randomBelow", vec![i()], res(i()));
    mono(e, "not", vec![b()], b());
}

fn strings(e: &mut TypeEnv) {
    mono(e, "contains", vec![s(), s()], b());
    mono(e, "startsWith", vec![s(), s()], b());
    mono(e, "endsWith", vec![s(), s()], b());
    // The fallible string ops return Result<T, Error> (matched on Success/Error).
    mono(e, "indexOf", vec![s(), s()], res(i()));
    mono(e, "split", vec![s(), s()], res(Type::list(s())));
    mono(e, "join", vec![Type::list(s()), s()], s());
    mono(e, "parseInt", vec![s()], res(i()));
    mono(e, "lines", vec![s()], Type::list(s()));
    mono(e, "words", vec![s()], Type::list(s()));
    mono(e, "replace", vec![s(), s(), s()], res(s()));
    mono(e, "repeat", vec![s(), i()], res(s()));
    mono(e, "substring", vec![s(), i(), i()], res(s()));
    mono(e, "take", vec![s(), i()], s());
    mono(e, "drop", vec![s(), i()], s());
    mono(e, "isEmpty", vec![any()], b());
    mono(e, "parseFloat", vec![s()], res(Type::float()));
    mono(e, "padStart", vec![s(), i(), s()], res(s()));
    mono(e, "padEnd", vec![s(), i(), s()], res(s()));
    // O(1) byte / codepoint cursor (BUILTIN-STRING-CURSOR). byteLength is total;
    // the rest are fallible (bad index / invalid UTF-8 / invalid scalar).
    mono(e, "byteLength", vec![s()], i());
    mono(e, "byteAt", vec![s(), i()], res(i()));
    mono(e, "codePointAt", vec![s(), i()], res(i()));
    mono(e, "codePointWidth", vec![i()], res(i()));
    mono(e, "fromCodePoint", vec![i()], res(s()));
    for op in [
        "toUpperCase",
        "toLowerCase",
        "trim",
        "trimStart",
        "trimEnd",
        "reverse",
    ] {
        mono(e, op, vec![s()], s());
    }
}

fn functional(e: &mut TypeEnv) {
    let t = || Type::Var(0);
    let v = || Type::Var(1);
    // forEach : (List<t>, (t) -> Unit) -> Unit
    poly(
        e,
        "forEach",
        vec![0],
        vec![Type::list(t()), Type::fun(vec![t()], u())],
        u(),
    );
    // map : (List<t>, (t) -> v) -> List<v>
    poly(
        e,
        "map",
        vec![0, 1],
        vec![Type::list(t()), Type::fun(vec![t()], v())],
        Type::list(v()),
    );
    // filter : (List<t>, (t) -> bool) -> List<t>
    poly(
        e,
        "filter",
        vec![0],
        vec![Type::list(t()), Type::fun(vec![t()], b())],
        Type::list(t()),
    );
    // fold : (List<t>, v, (v, t) -> v) -> v
    poly(
        e,
        "fold",
        vec![0, 1],
        vec![Type::list(t()), v(), Type::fun(vec![v(), t()], v())],
        v(),
    );
}

fn lists(e: &mut TypeEnv) {
    let t = || Type::Var(0);
    // Persistent List<T> API used by the list examples.
    poly(e, "List", vec![0], vec![], Type::list(t()));
    // `(List<t>, t) -> List<t>`: append/prepend share one signature.
    for name in ["listAppend", "listPrepend"] {
        poly(
            e,
            name,
            vec![0],
            vec![Type::list(t()), t()],
            Type::list(t()),
        );
    }
    poly(
        e,
        "listConcat",
        vec![0],
        vec![Type::list(t()), Type::list(t())],
        Type::list(t()),
    );
    poly(
        e,
        "listReverse",
        vec![0],
        vec![Type::list(t())],
        Type::list(t()),
    );
    poly(e, "listLength", vec![0], vec![Type::list(t())], i());
    poly(e, "listGet", vec![0], vec![Type::list(t()), i()], res(t()));
    poly(e, "listContains", vec![0], vec![Type::list(t()), t()], b());
    poly(
        e,
        "forEachList",
        vec![0],
        vec![Type::list(t()), Type::fun(vec![t()], u())],
        u(),
    );
    maps(e);
}

fn maps(e: &mut TypeEnv) {
    let k = || Type::Var(0);
    let v = || Type::Var(1);
    let m = || Type::map(k(), v());
    poly(e, "Map", vec![0, 1], vec![], m());
    poly(e, "mapSet", vec![0, 1], vec![m(), k(), v()], m());
    poly(e, "mapGet", vec![0, 1], vec![m(), k()], res(v()));
    poly(e, "mapRemove", vec![0, 1], vec![m(), k()], m());
    poly(e, "mapMerge", vec![0, 1], vec![m(), m()], m());
    poly(e, "mapContains", vec![0, 1], vec![m(), k()], b());
    poly(e, "mapLength", vec![0, 1], vec![m()], i());
    poly(e, "mapKeys", vec![0, 1], vec![m()], Type::list(k()));
    poly(e, "mapValues", vec![0, 1], vec![m()], Type::list(v()));
}

fn files(e: &mut TypeEnv) {
    mono(e, "readFile", vec![s()], res(s()));
    mono(e, "writeFile", vec![s(), s()], res(u()));
    mono(e, "deleteFile", vec![s()], res(u()));
}

fn http(e: &mut TypeEnv) {
    mono(e, "httpCreateClient", vec![s(), i()], i());
    mono(e, "httpCloseClient", vec![i()], u());
    mono(e, "httpGet", vec![i(), s(), s()], res(s()));
    mono(e, "httpGetResponse", vec![i(), s(), s()], res(i()));
    mono(e, "httpResponseBody", vec![i()], res(s()));
    mono(e, "httpResponseFree", vec![i()], u());
    mono(e, "httpResponseStatus", vec![i()], i());
    mono(e, "httpResponseHeader", vec![i(), s()], res(s()));
    // (clientId, path, body, headers) for POST/PUT; (clientId, path, headers) for DELETE.
    mono(e, "httpPost", vec![i(), s(), s(), s()], res(s()));
    mono(e, "httpPut", vec![i(), s(), s(), s()], res(s()));
    mono(e, "httpDelete", vec![i(), s(), s()], res(s()));
    mono(e, "httpCreateServer", vec![i(), s()], i());
    // httpListen takes the server id and a request-handler function.
    mono(e, "httpListen", vec![i(), any()], i());
    mono(e, "httpStopServer", vec![i()], u());
}

fn json(e: &mut TypeEnv) {
    // A parsed document is an opaque int handle, matching the runtime.
    mono(e, "jsonParse", vec![s()], res(i()));
    mono(e, "jsonGet", vec![i(), s()], res(s()));
    mono(e, "jsonLength", vec![i(), s()], i());
    mono(e, "jsonFree", vec![i()], u());
}

fn concurrency(e: &mut TypeEnv) {
    let t = || Type::Var(0);
    // await : (Fiber<t>) -> t
    poly(
        e,
        "await",
        vec![0],
        vec![Type::con("Fiber", vec![t()])],
        t(),
    );
    mono(e, "fiberDone", vec![any()], i());
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

fn websocket(e: &mut TypeEnv) {
    mono(e, "websocketCreateServer", vec![i(), s(), s()], i());
    mono(e, "websocketServerListen", vec![i()], i());
    mono(e, "websocketServerBroadcast", vec![i(), s()], i());
    mono(e, "websocketKeepAlive", vec![], u());
    mono(e, "websocketConnect", vec![s()], i());
    mono(e, "websocketSend", vec![i(), s()], i());
    mono(e, "websocketClose", vec![i()], u());
}

/// The rendered signature of a built-in (`name : type`), for editor hover.
/// `None` when `name` is not a built-in.
#[must_use]
pub fn builtin_signature(name: &str) -> Option<String> {
    base_env().get(name).map(|s| format!("{name} : {}", s.ty))
}

fn terminal(e: &mut TypeEnv) {
    mono(e, "termReadKey", vec![], res(s()));
    mono(e, "termRawMode", vec![i()], u());
    mono(e, "termCols", vec![], i());
    mono(e, "termRows", vec![], i());
    mono(e, "termClear", vec![], i());
    mono(e, "termMoveCursor", vec![i(), i()], i());
    mono(e, "termHideCursor", vec![], i());
    mono(e, "termShowCursor", vec![], i());
    // External process control: spawn with an event callback, await exit, clean up.
    mono(e, "spawnProcess", vec![s(), any()], i());
    mono(e, "awaitProcess", vec![i()], i());
    mono(e, "cleanupProcess", vec![i()], u());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_core_and_polymorphic_builtins() {
        let e = base_env();
        assert!(e.get("print").is_some());
        assert_eq!(e.get("map").unwrap().vars.len(), 2);
        assert_eq!(e.get("await").unwrap().vars.len(), 1);
    }
}
