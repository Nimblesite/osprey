//! Runtime/system builtins backed by the prebuilt C archives (file I/O, process
//! management, HTTP client/server, JSON) — the camel-case Osprey name maps to
//! its snake-case C symbol with a fixed parameter signature and a return-wrapping
//! discipline. The archive symbols (`libfiber_runtime.a` / `libhttp_runtime.a`)
//! are the contract: each table entry below must match its C signature exactly.
//! A named function passed as a callback (`spawnProcess` / `httpListen` handler)
//! is lowered to a raw code pointer here in `eval_args`. Implements
//! [BUILTIN-FILE], [BUILTIN-PROCESS], [BUILTIN-HTTP], [BUILTIN-WEBSOCKET],
//! [BUILTIN-JSON], [BUILTIN-TERM]. Database APIs deliberately stay ordinary extern calls
//! [FFI-NO-DB-BUILTINS].

use crate::builder::Codegen;
use crate::error::Result;
use crate::expr::gen_expr;
use crate::llty::{LType, Value};
use crate::result::{result_from_i64, result_from_nullable};
use osprey_ast::{Expr, NamedArgument};

/// How a runtime builtin's raw C return becomes an Osprey value.
#[derive(Clone, Copy)]
enum Ret {
    /// Plain `i64` (status, handle, count, exit code).
    Int,
    /// Plain `i8*` string, taken as-is (`input()` — the caller owns it).
    Str,
    /// `void` — yields Unit.
    Unit,
    /// C `i64` status discarded by a language-level Unit function.
    StatusUnit,
    /// `Result<int, _>`: the C `i64` is the success value; `< 0` ⇒ Error.
    ResultInt,
    /// `Result<string, _>`: the C `i8*` is the success value; `null` ⇒ Error.
    /// `Some(msg)` is the FALLBACK reason, used only when the call recorded
    /// none of its own on the failure channel [BUILTIN-FILE-ERRMSG].
    ResultStr(Option<&'static str>),
}

/// One builtin's lowering: its C symbol, parameter LLVM types, and return
/// discipline. A `Ptr` parameter is a string/handle/callback travelling as `i8*`.
struct Sig {
    cname: &'static str,
    params: &'static [LType],
    ret: Ret,
}

/// The runtime-builtin table — `None` if `name` is not one (so the caller falls
/// through to a user call). Returns are read from the C signatures in
/// `runtime/system_runtime.c`, `http_*_runtime.c`, `json_runtime.c`; every
/// `Result<int>` builtin shares the `< 0 ⇒ Error` convention, every
/// `Result<string>` the `null ⇒ Error` one.
fn lookup(name: &str) -> Option<Sig> {
    use LType::{Ptr, Str, I64};
    let sig = |cname, params, ret| Sig { cname, params, ret };
    Some(match name {
        // --- cryptographically-secure random + stdin (random_runtime.c) ---
        // [BUILTIN-RANDOM] uniform non-negative int; [BUILTIN-RANDOM-BELOW]
        // unbiased [0, n) (negative ⇒ Error per ResultInt); [BUILTIN-INPUT]
        // one stdin line as a string.
        "random" => sig("osp_random", &[], Ret::Int),
        "randomBelow" => sig("osp_random_below", &[I64], Ret::ResultInt),
        "input" => sig("osp_input", &[], Ret::Str),
        // --- file I/O [BUILTIN-FILE] (system_runtime.c) ---
        "readFile" => sig("read_file", &[Str], Ret::ResultStr(Some("File read error"))),
        "writeFile" => sig("write_file", &[Str, Str], Ret::ResultInt),
        // --- processes [BUILTIN-PROCESS] (system_runtime.c); arg 2 is callback ---
        "spawnProcess" => sig("spawn_process_with_handler", &[Str, Ptr], Ret::ResultInt),
        "awaitProcess" => sig("fiber_await_process", &[I64], Ret::Int),
        "cleanupProcess" => sig("fiber_cleanup_process", &[I64], Ret::Unit),
        // `sleep(ms)` is milliseconds via the fiber runtime. Its native status
        // is discarded by the Unit language surface [CONCURRENCY-SLEEP].
        "sleep" => sig("fiber_sleep", &[I64], Ret::StatusUnit),
        // --- HTTP server/client (http_*_runtime.c); httpListen arg1 is the handler ---
        "httpCreateServer" => sig("http_create_server", &[I64, Str], Ret::Int),
        "httpListen" => sig("http_listen", &[I64, Ptr], Ret::Int),
        "httpStopServer" => sig("http_stop_server", &[I64], Ret::Int),
        "httpCreateClient" => sig("http_create_client", &[Str, I64], Ret::Int),
        "httpGet" => sig("http_get", &[I64, Str, Str], Ret::Int),
        "httpPost" => sig("http_post", &[I64, Str, Str, Str], Ret::Int),
        "httpPut" => sig("http_put", &[I64, Str, Str, Str], Ret::Int),
        "httpDelete" => sig("http_delete", &[I64, Str, Str], Ret::Int),
        "httpCloseClient" => sig("http_close_client", &[I64], Ret::Int),
        "httpGetResponse" => sig("http_get_response", &[I64, Str, Str], Ret::ResultInt),
        "httpResponseStatus" => sig("http_response_status", &[I64], Ret::Int),
        "httpResponseBody" => sig("http_response_body", &[I64], Ret::ResultStr(None)),
        "httpResponseHeader" => sig("http_response_header", &[I64, Str], Ret::ResultStr(None)),
        "httpResponseFree" => sig("http_response_free", &[I64], Ret::ResultInt),
        // --- WebSocket text transport (websocket_*_runtime.c) [BUILTIN-WEBSOCKET] ---
        "websocketCreateServer" => sig("websocket_create_server", &[I64, Str, Str], Ret::Int),
        "websocketServerListen" => sig("websocket_server_listen", &[I64], Ret::Int),
        "websocketServerBroadcast" => sig("websocket_server_broadcast", &[I64, Str], Ret::Int),
        "websocketKeepAlive" => sig("websocket_keep_alive", &[], Ret::Unit),
        "websocketConnect" => sig("websocket_connect", &[Str], Ret::Int),
        "websocketSend" => sig("websocket_send", &[I64, Str], Ret::Int),
        "websocketClose" => sig("websocket_close", &[I64], Ret::Int),
        // --- JSON document handles (json_runtime.c) ---
        "jsonParse" => sig("json_parse", &[Str], Ret::ResultInt),
        "jsonGet" => sig("json_get", &[I64, Str], Ret::ResultStr(None)),
        "jsonLength" => sig("json_length", &[I64, Str], Ret::Int),
        "jsonFree" => sig("json_free", &[I64], Ret::ResultInt),
        // --- terminal control (term_runtime.c) [BUILTIN-TERM] ---
        "termRawMode" => sig("term_raw_mode", &[I64], Ret::StatusUnit),
        "termCols" => sig("term_cols", &[], Ret::Int),
        "termRows" => sig("term_rows", &[], Ret::Int),
        "termReadKey" => sig("term_read_key", &[], Ret::ResultStr(None)),
        "termClear" => sig("term_clear", &[], Ret::Int),
        "termMoveCursor" => sig("term_move_cursor", &[I64, I64], Ret::Int),
        "termHideCursor" => sig("term_hide_cursor", &[], Ret::Int),
        "termShowCursor" => sig("term_show_cursor", &[], Ret::Int),
        _ => return None,
    })
}

/// Dispatch a runtime builtin by name, or `None` if `name` is not one.
pub(crate) fn gen(
    cg: &mut Codegen,
    name: &str,
    args: &[Expr],
    named: &[NamedArgument],
) -> Result<Option<Value>> {
    let Some(sig) = lookup(name) else {
        return Ok(None);
    };
    let ops = eval_args(cg, name, &sig, args, named)?;
    Ok(Some(emit(cg, &sig, &ops)?))
}

/// Evaluate each argument (positional, or named in written order) and coerce it
/// to the builtin's declared parameter type. A bare user-function name in a
/// `Ptr` slot is a C callback (`spawnProcess`/`httpListen` handler) and takes
/// its RAW code pointer — the C runtime calls it through a plain
/// function-pointer cast, so a closure cell would be jumped into as code.
fn eval_args(
    cg: &mut Codegen,
    name: &str,
    sig: &Sig,
    args: &[Expr],
    named: &[NamedArgument],
) -> Result<Vec<String>> {
    sig.params
        .iter()
        .zip(crate::expr::arg_exprs(args, named))
        .enumerate()
        .map(|(index, (want, e))| {
            let v = match e {
                Expr::Identifier(n)
                    if *want == LType::Ptr
                        && cg.lookup(n).is_none()
                        && cg.fn_params.contains_key(n) =>
                {
                    callback_pointer(cg, n, name, index)?
                }
                _ => gen_expr(cg, e)?,
            };
            Ok(crate::cast::coerce_to(cg, v, *want)?.operand)
        })
        .collect()
}

/// The raw code pointer for a callback argument. A handler whose types are
/// INFERRED is generic and has no `@name` symbol of its own, so pointing at one
/// emitted IR that references a body codegen never defined — reported as
/// success, rejected by clang ([`crate::monofn::specialize_callback`]). The
/// builtin's declared parameter type says exactly which instantiation the C
/// side calls, so that one is emitted and pointed at instead.
fn callback_pointer(cg: &mut Codegen, handler: &str, builtin: &str, index: usize) -> Result<Value> {
    let Some(declared) = osprey_types::builtin_callback_type(builtin, index) else {
        return Ok(crate::expr::fn_pointer(cg, handler));
    };
    match crate::monofn::specialize_callback(cg, handler, &declared)? {
        Some(symbol) => Ok(crate::expr::mono_fn_pointer(cg, &symbol, &declared)),
        None => Ok(crate::expr::fn_pointer(cg, handler)),
    }
}

/// Emit the C call and wrap its return per the builtin's discipline.
fn emit(cg: &mut Codegen, sig: &Sig, ops: &[String]) -> Result<Value> {
    let params = sig
        .params
        .iter()
        .map(|t| t.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let op_refs: Vec<&str> = ops.iter().map(String::as_str).collect();
    match sig.ret {
        Ret::Unit => {
            cg.call_void(sig.cname, &params, &op_refs);
            Ok(Value::unit())
        }
        Ret::StatusUnit => {
            let _status = cg.call("i64", sig.cname, &params, &op_refs);
            Ok(Value::unit())
        }
        Ret::Int => Ok(Value::new(
            cg.call("i64", sig.cname, &params, &op_refs),
            LType::I64,
        )),
        Ret::Str => {
            // Fresh malloc'd C buffer — the caller owns it [GC-ARC-PERCEUS].
            let v = Value::new(cg.call("i8*", sig.cname, &params, &op_refs), LType::Str);
            crate::arc::own(cg, &v);
            Ok(v)
        }
        Ret::ResultInt => {
            clear_io_error(cg);
            let r = cg.call("i64", sig.cname, &params, &op_refs);
            let reason = take_io_error(cg);
            result_from_i64(cg, &r, None, Some(&reason))
        }
        Ret::ResultStr(err) => {
            clear_io_error(cg);
            let r = cg.call("i8*", sig.cname, &params, &op_refs);
            let reason = take_io_error(cg);
            // Own the raw C buffer; the Result payload store dups its own +1,
            // so this one drops at region end (null on the error path — no-op).
            crate::arc::own(cg, &Value::new(&r, LType::Str));
            result_from_nullable(cg, &r, err, Some(&reason))
        }
    }
}

/// Retire any reason the calling thread is holding, so a failure recorded by an
/// EARLIER builtin can never be attributed to this one. Paired with
/// [`take_io_error`] around every call, this is what makes the channel
/// trustworthy rather than merely usually-right. Implements
/// [BUILTIN-FILE-ERRMSG].
fn clear_io_error(cg: &mut Codegen) {
    cg.add_extern("declare void @osp_io_error_clear()");
    cg.emit("call void @osp_io_error_clear()".to_string());
}

/// Take ownership of the reason the call just recorded — `null` when it
/// recorded none, which every consumer treats as "no reason given" and falls
/// back on. Owning is not optional: the channel's own buffer is reused by this
/// thread's next I/O call, but the `Result` built from it can outlive any number
/// of those, so a borrowed pointer would later read as an unrelated failure.
fn take_io_error(cg: &mut Codegen) -> String {
    cg.add_extern("declare i8* @osp_io_error_take()");
    let reason = cg.emit_reg("call i8* @osp_io_error_take()".to_string());
    // The errmsg store dups its own +1, so this one drops at region end
    // (null on the success path — a no-op).
    crate::arc::own(cg, &Value::new(&reason, LType::Str));
    reason
}

#[cfg(test)]
mod tests {
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
            let marker = format!("define i64 @{function}(");
            let start = ir
                .find(&marker)
                .unwrap_or_else(|| panic!("missing {function}"));
            let body = &ir[start..ir[start..].find("\n}").map_or(ir.len(), |n| start + n)];
            assert!(body.contains(&format!("call i64 @{callee}")), "{body}");
            assert!(body.contains("ret i64 0"), "Unit must return zero: {body}");
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
        assert!(ir.contains("declare void @osp_io_error_clear()"), "{ir}");
        assert!(ir.contains("declare i8* @osp_io_error_take()"), "{ir}");
        for (callee, ret) in [("write_file", "i64"), ("read_file", "i8*")] {
            let call = format!("call {ret} @{callee}(");
            let at = ir.find(&call).unwrap_or_else(|| panic!("missing {callee}"));
            let (before, after) = ir.split_at(at);
            assert!(
                before.rfind("call void @osp_io_error_clear()")
                    > before.rfind("call i8* @osp_io_error_take()"),
                "{callee}: the clear must be the last channel op before the call"
            );
            assert!(
                after.contains("call i8* @osp_io_error_take()"),
                "{callee}: no take after the call"
            );
        }
        // The reason outranks the static fallback, so a producer that recorded
        // one is never reported as the placeholder.
        assert!(ir.contains("File read error"), "fallback dropped: {ir}");
        assert!(ir.contains("icmp ne i8* "), "no reason-present test: {ir}");
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
        assert!(!ir.contains("@websocketCreateServer"), "{ir}");
    }
}
