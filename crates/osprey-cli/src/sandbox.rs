//! The capability sandbox: `--sandbox` / `--no-http` / `--no-websocket` /
//! `--no-fs` / `--no-ffi` gate which built-in capabilities a program may use.
//! Enforcement is a pre-codegen pass over the parsed program: a gated builtin
//! referenced anywhere (or an `extern` declaration under `--no-ffi`) is a
//! compile error, so untrusted code is rejected before any IR exists.

use osprey_ast::{Program, Stmt};

/// Which capability groups the invocation allows. Everything defaults to on;
/// `--sandbox` turns every group off at once.
#[expect(
    clippy::struct_excessive_bools,
    reason = "a capability sandbox is by nature a set of independent on/off switches"
)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct Policy {
    pub http: bool,
    pub websocket: bool,
    pub fs: bool,
    pub ffi: bool,
    pub process: bool,
}

impl Policy {
    pub(crate) fn allow_all() -> Policy {
        Policy {
            http: true,
            websocket: true,
            fs: true,
            ffi: true,
            process: true,
        }
    }

    /// `--sandbox`: every risky capability off.
    pub(crate) fn sandbox() -> Policy {
        Policy {
            http: false,
            websocket: false,
            fs: false,
            ffi: false,
            process: false,
        }
    }
}

const HTTP_FNS: &[&str] = &[
    "httpCreateServer",
    "httpListen",
    "httpStopServer",
    "httpCreateClient",
    "httpGet",
    "httpPost",
    "httpPut",
    "httpDelete",
    "httpCloseClient",
    "httpGetResponse",
    "httpResponseStatus",
    "httpResponseBody",
    "httpResponseHeader",
    "httpResponseFree",
];
const WEBSOCKET_FNS: &[&str] = &[
    "websocketConnect",
    "websocketSend",
    "websocketKeepAlive",
    "websocketClose",
    "websocketCreateServer",
    "websocketListen",
    "websocketServerBroadcast",
    "websocketStopServer",
];
const FS_FNS: &[&str] = &["readFile", "writeFile"];
const PROCESS_FNS: &[&str] = &["spawnProcess", "awaitProcess", "cleanupProcess"];

/// Every policy violation in `program`, as ready-to-print messages. Empty means
/// the program is allowed to compile under `policy`.
pub(crate) fn violations(program: &Program, policy: Policy) -> Vec<String> {
    let idents = osprey_codegen::referenced_idents(program);
    let mut out = Vec::new();
    let gated: &[(bool, &[&str], &str)] = &[
        (policy.http, HTTP_FNS, "--no-http"),
        (policy.websocket, WEBSOCKET_FNS, "--no-websocket"),
        (policy.fs, FS_FNS, "--no-fs"),
        (policy.process, PROCESS_FNS, "--sandbox"),
    ];
    for (allowed, fns, flag) in gated {
        if *allowed {
            continue;
        }
        for f in fns.iter().filter(|f| idents.contains(**f)) {
            out.push(format!("security: `{f}` is disabled by {flag}"));
        }
    }
    if !policy.ffi {
        extern_violations(&program.statements, &mut out);
    }
    out
}

/// `--no-ffi`: any `extern` declaration (including inside modules) is rejected.
fn extern_violations(statements: &[Stmt], out: &mut Vec<String>) {
    for s in statements {
        match s {
            Stmt::Extern { name, .. } => out.push(format!(
                "security: extern function `{name}` is disabled by --no-ffi"
            )),
            Stmt::Module { body, .. } => {
                for item in body {
                    extern_violations(std::slice::from_ref(item.declaration.as_ref()), out);
                }
            }
            Stmt::Namespace { body, .. } => extern_violations(body, out),
            _ => {}
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    reason = "test assertions: an out-of-bounds index is a test failure, not a production panic"
)]
mod tests {
    use super::*;

    fn prog(src: &str) -> Program {
        osprey_syntax::parse_program(src).program
    }

    #[test]
    fn allow_all_and_sandbox_set_every_switch() {
        let all = Policy::allow_all();
        assert!(all.http && all.websocket && all.fs && all.ffi && all.process);
        let none = Policy::sandbox();
        assert!(!none.http && !none.websocket && !none.fs && !none.ffi && !none.process);
        // A `Copy` policy survives being passed by value.
        let copy = all;
        assert!(copy.fs && all.fs);
    }

    #[test]
    fn allow_all_permits_gated_builtins() {
        let src = "let f = readFile(\"a.txt\")\nlet p = spawnProcess(\"echo hi\", f)\n";
        assert!(violations(&prog(src), Policy::allow_all()).is_empty());
    }

    #[test]
    fn no_fs_flags_file_builtins_only() {
        let mut policy = Policy::allow_all();
        policy.fs = false;
        let v = violations(
            &prog("let c = readFile(\"a.txt\")\nlet w = writeFile(\"b\", c)\n"),
            policy,
        );
        assert_eq!(v.len(), 2, "both readFile and writeFile are gated: {v:?}");
        assert!(v.iter().all(|m| m.contains("--no-fs")));
        assert!(v.iter().any(|m| m.contains("readFile")));
        assert!(v.iter().any(|m| m.contains("writeFile")));
        // HTTP/process still allowed under --no-fs.
        assert!(violations(&prog("let p = spawnProcess(\"echo\", f)\n"), policy).is_empty());
    }

    #[test]
    fn no_http_and_no_websocket_are_independent() {
        let mut http_off = Policy::allow_all();
        http_off.http = false;
        let v = violations(&prog("let s = httpListen(8080, h)\n"), http_off);
        assert_eq!(v.len(), 1);
        assert!(v[0].contains("httpListen") && v[0].contains("--no-http"));
        // websocket builtin is NOT gated when only http is off.
        assert!(violations(&prog("let c = websocketConnect(\"ws://x\")\n"), http_off).is_empty());

        let mut ws_off = Policy::allow_all();
        ws_off.websocket = false;
        let v = violations(&prog("let c = websocketConnect(\"ws://x\")\n"), ws_off);
        assert_eq!(v.len(), 1);
        assert!(v[0].contains("websocketConnect") && v[0].contains("--no-websocket"));
    }

    #[test]
    fn sandbox_flags_process_builtins() {
        let v = violations(
            &prog("let p = spawnProcess(\"echo hi\", h)\n"),
            Policy::sandbox(),
        );
        assert!(v
            .iter()
            .any(|m| m.contains("spawnProcess") && m.contains("--sandbox")));
    }

    #[test]
    fn no_ffi_rejects_extern_declarations_including_in_modules() {
        let mut policy = Policy::allow_all();
        policy.ffi = false;
        let src = "extern fn sqlite3_open(filename: string, ppDb: Ptr) -> int\n";
        let v = violations(&prog(src), policy);
        assert_eq!(v.len(), 1);
        assert!(v[0].contains("sqlite3_open") && v[0].contains("--no-ffi"));
        // ffi allowed => the same extern compiles.
        assert!(violations(&prog(src), Policy::allow_all()).is_empty());
        // Two externs each get their own message.
        let two = "extern fn a() -> int\nextern fn b() -> int\n";
        assert_eq!(violations(&prog(two), policy).len(), 2);
    }
}
