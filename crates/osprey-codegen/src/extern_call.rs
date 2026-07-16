//! Runtime/system builtins backed by the prebuilt C archives (file I/O, process
//! management, HTTP client/server, JSON) — the camel-case Osprey name maps to
//! its snake-case C symbol with a fixed parameter signature and a return-wrapping
//! discipline. The archive symbols (`libfiber_runtime.a` / `libhttp_runtime.a`)
//! are the contract: each table entry below must match its C signature exactly.
//! A named function passed as a callback (`spawnProcess` / `httpListen` handler)
//! is lowered to a raw code pointer here in `eval_args`. Implements
//! [BUILTIN-FILE], [BUILTIN-PROCESS], [BUILTIN-HTTP], [BUILTIN-JSON],
//! [BUILTIN-TERM].

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
    /// `Result<int, _>`: the C `i64` is the success value; `< 0` ⇒ Error.
    ResultInt,
    /// `Result<string, _>`: the C `i8*` is the success value; `null` ⇒ Error.
    /// `Some(msg)` stores that constant on the error path (`readFile`).
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
        // --- file I/O (system_runtime.c) ---
        "readFile" => sig("read_file", &[Str], Ret::ResultStr(Some("File read error"))),
        "writeFile" => sig("write_file", &[Str, Str], Ret::ResultInt),
        // --- processes (system_runtime.c); 2nd arg of spawn is the callback ---
        "spawnProcess" => sig("spawn_process_with_handler", &[Str, Ptr], Ret::ResultInt),
        "awaitProcess" => sig("fiber_await_process", &[I64], Ret::Int),
        "cleanupProcess" => sig("fiber_cleanup_process", &[I64], Ret::Unit),
        // `sleep(ms)` is MILLISECONDS via the fiber runtime — NOT libc
        // `sleep(seconds)`, which an unmapped fall-through would link.
        "sleep" => sig("fiber_sleep", &[I64], Ret::Int),
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
        // --- JSON document handles (json_runtime.c) ---
        "jsonParse" => sig("json_parse", &[Str], Ret::ResultInt),
        "jsonGet" => sig("json_get", &[I64, Str], Ret::ResultStr(None)),
        "jsonLength" => sig("json_length", &[I64, Str], Ret::Int),
        "jsonFree" => sig("json_free", &[I64], Ret::ResultInt),
        // --- terminal control (term_runtime.c) [BUILTIN-TERM] ---
        "termRawMode" => sig("term_raw_mode", &[I64], Ret::Int),
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
    let ops = eval_args(cg, &sig, args, named)?;
    Ok(Some(emit(cg, &sig, &ops)?))
}

/// Evaluate each argument (positional, or named in written order) and coerce it
/// to the builtin's declared parameter type. A bare user-function name in a
/// `Ptr` slot is a C callback (`spawnProcess`/`httpListen` handler) and takes
/// its RAW code pointer — the C runtime calls it through a plain
/// function-pointer cast, so a closure cell would be jumped into as code.
fn eval_args(
    cg: &mut Codegen,
    sig: &Sig,
    args: &[Expr],
    named: &[NamedArgument],
) -> Result<Vec<String>> {
    sig.params
        .iter()
        .zip(crate::expr::arg_exprs(args, named))
        .map(|(want, e)| {
            let v = match e {
                Expr::Identifier(n)
                    if *want == LType::Ptr
                        && cg.lookup(n).is_none()
                        && cg.fn_params.contains_key(n) =>
                {
                    crate::expr::fn_pointer(cg, n)
                }
                _ => gen_expr(cg, e)?,
            };
            Ok(crate::cast::coerce_to(cg, v, *want)?.operand)
        })
        .collect()
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
            let r = cg.call("i64", sig.cname, &params, &op_refs);
            // The negative-i64 runtime convention carries no message string;
            // the Error arm falls back to the bare "Error" reason.
            result_from_i64(cg, &r, None)
        }
        Ret::ResultStr(err) => {
            let r = cg.call("i8*", sig.cname, &params, &op_refs);
            // Own the raw C buffer; the Result payload store dups its own +1,
            // so this one drops at region end (null on the error path — no-op).
            crate::arc::own(cg, &Value::new(&r, LType::Str));
            result_from_nullable(cg, &r, err)
        }
    }
}
