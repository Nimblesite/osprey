use super::{i, mono, res, s, u, Type, TypeEnv};

pub(super) fn files(e: &mut TypeEnv) {
    // File surface [BUILTIN-FILE].
    mono(e, "readFile", vec![s()], res(s()));
    mono(e, "writeFile", vec![s(), s()], res(i()));
}

pub(super) fn http(e: &mut TypeEnv) {
    http_client(e);
    http_server(e);
}

fn http_client(e: &mut TypeEnv) {
    mono(e, "httpCreateClient", vec![s(), i()], res(i()));
    mono(e, "httpCloseClient", vec![i()], res(i()));
    mono(e, "httpGet", vec![i(), s(), s()], res(i()));
    mono(e, "httpGetResponse", vec![i(), s(), s()], res(i()));
    mono(e, "httpPostResponse", vec![i(), s(), s(), s()], res(i()));
    mono(e, "httpResponseBody", vec![i()], res(s()));
    mono(e, "httpResponseFree", vec![i()], res(i()));
    mono(e, "httpResponseStatus", vec![i()], i());
    mono(e, "httpResponseHeader", vec![i(), s()], res(s()));
    // (clientId, path, body, headers) for POST/PUT; (clientId, path, headers) for DELETE.
    mono(e, "httpPost", vec![i(), s(), s(), s()], res(i()));
    mono(e, "httpPut", vec![i(), s(), s(), s()], res(i()));
    mono(e, "httpDelete", vec![i(), s(), s()], res(i()));
}

fn http_server(e: &mut TypeEnv) {
    mono(e, "httpCreateServer", vec![i(), s()], res(i()));
    // The C runtime calls this handler with the four request strings and reads
    // the returned record using the built-in HttpResponse layout.
    mono(
        e,
        "httpListen",
        vec![
            i(),
            Type::fun(vec![s(), s(), s(), s()], Type::prim("HttpResponse")),
        ],
        res(i()),
    );
    mono(e, "httpStopServer", vec![i()], res(i()));
    // [HTTP-SERVER-CLIENT-GONE] A long handler asks whether anyone still waits.
    mono(e, "httpClientGone", vec![], i());
}

pub(super) fn websocket(e: &mut TypeEnv) {
    mono(e, "websocketCreateServer", vec![i(), s(), s()], i());
    mono(e, "websocketServerListen", vec![i()], i());
    mono(e, "websocketServerBroadcast", vec![i(), s()], i());
    mono(e, "websocketKeepAlive", vec![], u());
    mono(e, "websocketConnect", vec![s()], i());
    mono(e, "websocketSend", vec![i(), s()], i());
    mono(e, "websocketClose", vec![i()], i());
}

/// The shared-frame writers of `native_scene.c` ([NATIVE-SHM-VIEW],
/// [NATIVE-SHM-SCENE], [NATIVE-SHM-ASK]). Typed builtins whose C signatures the
/// compiler fixes, so `@osprey/native` declares no raw extern and the registry
/// publishes it [PACKAGE-CAPABILITY-CONFINEMENT]. Each returns the C status.
pub(super) fn native_frame(e: &mut TypeEnv) {
    let f = Type::float;
    let shape = vec![i(), i(), f(), f(), f(), f(), f(), i(), i(), s()];
    mono(e, "nativeSceneShape", shape, i());
    let row = vec![i(), i(), i(), s(), i(), i(), f(), f(), f(), f(), i()];
    mono(e, "nativeViewRow", row, i());
    let extend = vec![i(), f(), f(), f(), i(), i(), s()];
    mono(e, "nativeViewExtend", extend, i());
    mono(e, "nativeAsk", vec![i(), i(), s()], i());
}

pub(super) fn terminal(e: &mut TypeEnv) {
    mono(e, "termReadKey", vec![], res(s()));
    mono(e, "termRawMode", vec![i()], u());
    mono(e, "termCols", vec![], i());
    mono(e, "termRows", vec![], i());
    mono(e, "termClear", vec![], i());
    mono(e, "termMoveCursor", vec![i(), i()], i());
    mono(e, "termHideCursor", vec![], i());
    mono(e, "termShowCursor", vec![], i());
    processes(e);
}

fn processes(e: &mut TypeEnv) {
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
