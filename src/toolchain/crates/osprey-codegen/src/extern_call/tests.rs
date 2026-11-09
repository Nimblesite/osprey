#[test]
fn unit_builtins_discard_native_status_values() {
    // The C functions report status, but their language contracts are Unit:
    // [BUILTIN-TERM] and [CONCURRENCY-SLEEP].
    let parsed = osprey_syntax::parse_program(
        "fn configure() -> Unit = termRawMode(0)\n\
         fn nap() -> Unit = sleep(0)\n\
         let configured = configure()\n\
         let slept = nap()\n",
    );
    assert!(
        parsed.errors.is_empty(),
        "syntax errors: {:?}",
        parsed.errors
    );
    let ir = crate::compile_program(&parsed.program).expect("terminal codegen");
    for (function, callee) in [("configure", "term_raw_mode"), ("nap", "fiber_sleep")] {
        let marker = format!("define internal i64 @{function}(");
        let start = ir
            .find(&marker)
            .unwrap_or_else(|| panic!("missing {function}"));
        let body = &ir[start..ir[start..].find("\n}").map_or(ir.len(), |n| start + n)];
        assert!(body.contains(&format!("call i64 @{callee}")), "{body}");
        crate::testkit::shows(body, &["ret i64 0"]);
    }
}

#[test]
fn a_fallible_builtin_carries_the_runtime_failure_reason() {
    // [BUILTIN-FILE-ERRMSG] The reason must be CLEARED before the call and
    // TAKEN after it. Without the clear, a reason left by an earlier failed
    // op is reported as this call's cause; without the take, the Error holds
    // a borrowed pointer into a thread-local the next I/O call overwrites.
    let parsed = osprey_syntax::parse_program(
        "let written = writeFile(\"out.txt\", \"body\")\n\
         let loaded = readFile(\"out.txt\")\n",
    );
    assert!(
        parsed.errors.is_empty(),
        "syntax errors: {:?}",
        parsed.errors
    );
    let ir = crate::compile_program(&parsed.program).expect("file builtin codegen");
    crate::testkit::shows(
        &ir,
        &[
            "declare void @osp_io_error_clear()",
            "declare i8* @osp_io_error_take()",
        ],
    );
    for (callee, ret) in [("write_file", "i64"), ("read_file", "i8*")] {
        let call = format!("call {ret} @{callee}(");
        let at = ir.find(&call).unwrap_or_else(|| panic!("missing {callee}"));
        let (before, after) = ir.split_at(at);
        assert!(
            before.rfind("call void @osp_io_error_clear()")
                > before.rfind("call i8* @osp_io_error_take()"),
            "{callee}: the clear must be the last channel op before the call"
        );
        crate::testkit::shows(after, &["call i8* @osp_io_error_take()"]);
    }
    // The reason outranks the static fallback, so a producer that recorded
    // one is never reported as the placeholder.
    crate::testkit::shows(&ir, &["File read error", "icmp ne i8* "]);
}

#[test]
fn load_tensor_reads_a_host_list_without_the_gpu_surface() {
    // [BUILTIN-FILE-TENSOR] One call into the backend-neutral safetensors
    // reader, wrapped like readFile: no GpuBuffer is built and copied out.
    let parsed = osprey_syntax::parse_program(
        "let w = loadTensor(\"m.safetensors\", \"w\")\n\
         let first = match w { Success { value } => listGet(value, 0) ?: 0.0  Error { message } => 0.0 }\n",
    );
    assert!(
        parsed.errors.is_empty(),
        "syntax errors: {:?}",
        parsed.errors
    );
    let ir = crate::compile_program(&parsed.program).expect("loadTensor codegen");
    let call = ir.find("call i8* @osprey_load_tensor(");
    assert!(call.is_some(), "{ir}");
    let (before, after) = ir.split_at(call.unwrap_or(0));
    assert!(
        before.rfind("call void @osp_io_error_clear()").is_some(),
        "{ir}"
    );
    crate::testkit::shows(after, &["call i8* @osp_io_error_take()"]);
    crate::testkit::contents(&ir, &[], &["osprey_gpu_"]);
}

#[test]
fn websocket_builtins_lower_to_the_c_runtime_abi() {
    // [BUILTIN-WEBSOCKET] Camel-case language names must not escape into
    // LLVM: the archive exports snake-case C symbols.
    let parsed = osprey_syntax::parse_program(
        "let s = websocketCreateServer(8080, \"127.0.0.1\", \"/chat\")\n\
         let listening = websocketServerListen(s)\n\
         let sent = websocketServerBroadcast(s, \"hello\")\n\
         let c = websocketConnect(\"ws://127.0.0.1:8080/chat\")\n\
         let wrote = websocketSend(c, \"hello\")\n\
         let closed = websocketClose(c)\n\
         websocketKeepAlive()\n",
    );
    assert!(
        parsed.errors.is_empty(),
        "syntax errors: {:?}",
        parsed.errors
    );
    let ir = crate::compile_program(&parsed.program).expect("WebSocket codegen");
    for symbol in [
        "websocket_create_server",
        "websocket_server_listen",
        "websocket_server_broadcast",
        "websocket_connect",
        "websocket_send",
        "websocket_close",
        "websocket_keep_alive",
    ] {
        assert!(ir.contains(&format!("@{symbol}")), "missing {symbol}: {ir}");
    }
    crate::testkit::contents(&ir, &[], &["@websocketCreateServer"]);
}
