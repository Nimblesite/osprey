//! The canonical table of built-in function signatures. Each entry is a name
//! bound to a (possibly polymorphic) scheme. Polymorphic schemes use `Var(0)`,
//! `Var(1)` as their quantified variables; `instantiate` renames them per use,
//! so the concrete ids only need to be self-consistent within one scheme.
//!
//! Signatures use `any` only where one Hindley-Milner type cannot express the
//! runtime's supported alternatives (`print`, `toString`, `length`,
//! `isEmpty`); `builtin_constraints` checks their concrete call-site types.
//! Result-returning runtime builtins return `Result<T, Error>` — the shape the C
//! runtime actually returns — while arithmetic operators use `MathError`.

use crate::env::TypeEnv;
use crate::ty::{names, Scheme, Type};

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

/// Built-ins a user function may redefine: the testing names are common
/// identifiers, so a same-named user function shadows the built-in instead of
/// erroring. Implements [TESTING-SHADOWING] (docs/specs/0027-TestingFramework.md).
pub const SHADOWABLE_BUILTINS: &[&str] = &[
    "test",
    "expect",
    "expectAll",
    "expectTrue",
    "expectFalse",
    "check",
    "checkAll",
    "checkTrue",
    "checkFalse",
];

/// Install every built-in into a base environment.
pub fn base_env() -> TypeEnv {
    let mut e = TypeEnv::new();
    core(&mut e);
    testing(&mut e);
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
    // [CONCURRENCY-SLEEP] The native status is not part of the Unit surface.
    mono(e, "sleep", vec![i()], u());
    // A range is a fused iterator handle, not a materialized List [BUILTIN-ITER].
    mono(e, "range", vec![i(), i()], Type::iterator(i()));
    mono(
        e,
        "abs",
        vec![i()],
        Type::result(i(), Type::prim(names::MATH_ERROR)),
    );
    // Truncating integer division, divide-by-zero-checked → Result<int, Error>.
    // The `/` operator is float-only (Osprey spec); this is its integer sibling.
    // Implements [BUILTIN-INTDIV].
    mono(e, "intDiv", vec![i(), i()], res(i()));
    // Named equivalents of the overflow-checked integer operators. These retain
    // the runtime builtins' generic Error channel for compatibility.
    for checked in ["checkedAdd", "checkedSub", "checkedMul"] {
        mono(e, checked, vec![i(), i()], res(i()));
    }
    // Cryptographically-secure randomness (random_runtime.c). `random` yields a
    // uniform non-negative int; `randomBelow(n)` an unbiased int in [0, n),
    // Error when n <= 0. Implements [BUILTIN-RANDOM], [BUILTIN-RANDOM-BELOW].
    mono(e, "random", vec![], i());
    mono(e, "randomBelow", vec![i()], res(i()));
}

/// The testing framework's built-ins. Implements [TESTING-BUILTINS]
/// (docs/specs/0027-TestingFramework.md).
fn testing(e: &mut TypeEnv) {
    // test(name, body): run `body` as one named test case. The body returns any
    // type — Unit for a Default-flavor imperative case, a `Verdict` for the pure
    // ML-flavor value model, which `test` pattern-matches and reports.
    // [TESTING-BUILTIN-TEST], [TESTING-VERDICT]
    poly(
        e,
        "test",
        vec![0],
        vec![s(), Type::fun(vec![], Type::Var(0))],
        u(),
    );
    // expect(actual, expected). [TESTING-BUILTIN-EXPECT]
    mono(e, "expect", vec![any(), any()], u());
    mono(e, "expectAll", vec![Type::list(b())], u());
    mono(e, "expectTrue", vec![b()], u());
    mono(e, "expectFalse", vec![b()], u());
    // check(label, expected, actual). [TESTING-BUILTIN-CHECK]
    mono(e, "check", vec![s(), any(), any()], u());
    mono(e, "checkAll", vec![s(), Type::list(b())], u());
    mono(e, "checkTrue", vec![s(), b()], u());
    mono(e, "checkFalse", vec![s(), b()], u());
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
    let iter_t = || Type::iterator(t());
    let iter_v = || Type::iterator(v());
    // Fused iterator surface [BUILTIN-ITER]. Runtime lists use the explicitly
    // list-named traversal functions below.
    poly(
        e,
        "forEach",
        vec![0],
        vec![iter_t(), Type::fun(vec![t()], u())],
        u(),
    );
    poly(
        e,
        "map",
        vec![0, 1],
        vec![iter_t(), Type::fun(vec![t()], v())],
        iter_v(),
    );
    poly(
        e,
        "filter",
        vec![0],
        vec![iter_t(), Type::fun(vec![t()], b())],
        iter_t(),
    );
    poly(
        e,
        "fold",
        vec![0, 1],
        vec![iter_t(), v(), Type::fun(vec![v(), t()], v())],
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
    // Eager list traversal [BUILTIN-LIST-FOREACH].
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
    // Public map construction uses OSPREY_KEY_STRING in the runtime. Keeping
    // the key concrete prevents an int/bool value from being interpreted as a
    // string pointer by the erased map ABI [TYPE-MAP], [TYPE-MAP-LITERAL].
    let v = || Type::Var(0);
    let m = || Type::map(s(), v());
    poly(e, "Map", vec![0], vec![], m());
    poly(e, "mapSet", vec![0], vec![m(), s(), v()], m());
    poly(e, "mapGet", vec![0], vec![m(), s()], res(v()));
    poly(e, "mapRemove", vec![0], vec![m(), s()], m());
    poly(e, "mapMerge", vec![0], vec![m(), m()], m());
    poly(e, "mapContains", vec![0], vec![m(), s()], b());
    poly(e, "mapLength", vec![0], vec![m()], i());
    poly(e, "mapKeys", vec![0], vec![m()], Type::list(s()));
    poly(e, "mapValues", vec![0], vec![m()], Type::list(v()));
}

fn files(e: &mut TypeEnv) {
    // File surface [BUILTIN-FILE].
    mono(e, "readFile", vec![s()], res(s()));
    mono(e, "writeFile", vec![s(), s()], res(i()));
}

fn http(e: &mut TypeEnv) {
    mono(e, "httpCreateClient", vec![s(), i()], i());
    mono(e, "httpCloseClient", vec![i()], i());
    mono(e, "httpGet", vec![i(), s(), s()], i());
    mono(e, "httpGetResponse", vec![i(), s(), s()], res(i()));
    mono(e, "httpResponseBody", vec![i()], res(s()));
    mono(e, "httpResponseFree", vec![i()], res(i()));
    mono(e, "httpResponseStatus", vec![i()], i());
    mono(e, "httpResponseHeader", vec![i(), s()], res(s()));
    // (clientId, path, body, headers) for POST/PUT; (clientId, path, headers) for DELETE.
    mono(e, "httpPost", vec![i(), s(), s(), s()], i());
    mono(e, "httpPut", vec![i(), s(), s(), s()], i());
    mono(e, "httpDelete", vec![i(), s(), s()], i());
    mono(e, "httpCreateServer", vec![i(), s()], i());
    // The C runtime calls this handler with the four request strings and reads
    // the returned record using the built-in HttpResponse layout.
    mono(
        e,
        "httpListen",
        vec![
            i(),
            Type::fun(vec![s(), s(), s(), s()], Type::prim("HttpResponse")),
        ],
        i(),
    );
    mono(e, "httpStopServer", vec![i()], i());
}

fn json(e: &mut TypeEnv) {
    // A parsed document is an opaque int handle, matching the runtime.
    mono(e, "jsonParse", vec![s()], res(i()));
    mono(e, "jsonGet", vec![i(), s()], res(s()));
    mono(e, "jsonLength", vec![i(), s()], i());
    mono(e, "jsonFree", vec![i()], res(i()));
}

fn concurrency(e: &mut TypeEnv) {
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

fn websocket(e: &mut TypeEnv) {
    mono(e, "websocketCreateServer", vec![i(), s(), s()], i());
    mono(e, "websocketServerListen", vec![i()], i());
    mono(e, "websocketServerBroadcast", vec![i(), s()], i());
    mono(e, "websocketKeepAlive", vec![], u());
    mono(e, "websocketConnect", vec![s()], i());
    mono(e, "websocketSend", vec![i(), s()], i());
    mono(e, "websocketClose", vec![i()], i());
}

/// The rendered signature of a built-in (`name : type`), for editor hover.
/// `None` when `name` is not a built-in.
#[must_use]
pub fn builtin_signature(name: &str) -> Option<String> {
    let scheme = base_env().get(name)?.clone();
    if let (Some(display), Type::Fun { params, ret }) = (
        crate::builtin_constraints::display_param_type(name, 0),
        &scheme.ty,
    ) {
        if params.len() == 1 {
            return Some(format!("{name} : ({display}) -> {ret}"));
        }
    }
    Some(format!("{name} : {}", scheme.ty))
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
    // External process control [BUILTIN-PROCESS]: spawning can fail before a
    // handle exists; await and cleanup operate on a successful handle.
    mono(
        e,
        "spawnProcess",
        vec![s(), Type::fun(vec![i(), i(), s()], u())],
        res(i()),
    );
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
        assert_eq!(
            builtin_signature("fiberDone").as_deref(),
            Some("fiberDone : (Fiber<t0>) -> int")
        );
        assert_eq!(
            builtin_signature("abs").as_deref(),
            Some("abs : (int) -> Result<int, MathError>")
        );
    }

    #[test]
    fn process_builtins_match_the_result_returning_runtime() {
        assert_eq!(
            builtin_signature("spawnProcess").as_deref(),
            Some("spawnProcess : (string, (int, int, string) -> Unit) -> Result<int, Error>")
        );
        assert_eq!(
            builtin_signature("awaitProcess").as_deref(),
            Some("awaitProcess : (int) -> int")
        );
        assert_eq!(
            builtin_signature("cleanupProcess").as_deref(),
            Some("cleanupProcess : (int) -> Unit")
        );
    }

    #[test]
    fn network_builtins_match_the_runtime_status_abi() {
        let expected = [
            (
                "writeFile",
                "writeFile : (string, string) -> Result<int, Error>",
            ),
            ("httpCloseClient", "httpCloseClient : (int) -> int"),
            ("httpGet", "httpGet : (int, string, string) -> int"),
            (
                "httpResponseFree",
                "httpResponseFree : (int) -> Result<int, Error>",
            ),
            (
                "httpPost",
                "httpPost : (int, string, string, string) -> int",
            ),
            ("httpPut", "httpPut : (int, string, string, string) -> int"),
            ("httpDelete", "httpDelete : (int, string, string) -> int"),
            (
                "httpListen",
                "httpListen : (int, (string, string, string, string) -> HttpResponse) -> int",
            ),
            ("httpStopServer", "httpStopServer : (int) -> int"),
            ("websocketClose", "websocketClose : (int) -> int"),
            ("jsonFree", "jsonFree : (int) -> Result<int, Error>"),
        ];
        for (name, signature) in expected {
            assert_eq!(
                builtin_signature(name).as_deref(),
                Some(signature),
                "{name}"
            );
        }
    }

    #[test]
    fn public_maps_use_the_runtime_string_key_abi() {
        let expected = [
            ("Map", "Map : () -> Map<string, t0>"),
            (
                "mapSet",
                "mapSet : (Map<string, t0>, string, t0) -> Map<string, t0>",
            ),
            (
                "mapGet",
                "mapGet : (Map<string, t0>, string) -> Result<t0, Error>",
            ),
            ("mapKeys", "mapKeys : (Map<string, t0>) -> List<string>"),
        ];
        for (name, signature) in expected {
            assert_eq!(
                builtin_signature(name).as_deref(),
                Some(signature),
                "{name}"
            );
        }
    }

    #[test]
    fn iterator_builtins_do_not_advertise_runtime_lists() {
        let expected = [
            ("range", "range : (int, int) -> Iterator<int>"),
            ("map", "map : (Iterator<t0>, (t0) -> t1) -> Iterator<t1>"),
            (
                "filter",
                "filter : (Iterator<t0>, (t0) -> bool) -> Iterator<t0>",
            ),
            ("forEach", "forEach : (Iterator<t0>, (t0) -> Unit) -> Unit"),
            ("fold", "fold : (Iterator<t0>, t1, (t1, t0) -> t1) -> t1"),
        ];
        for (name, signature) in expected {
            assert_eq!(
                builtin_signature(name).as_deref(),
                Some(signature),
                "{name}"
            );
        }
    }
}
