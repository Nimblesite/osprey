//! The mounted profiler over real JSON-RPC [PROF-LSP-HOST]: capabilities at
//! `initialize`, `osprey.lspProfiler.*` commands answered from their own task
//! with the protocol's error codes, and cancellation of one in flight.
use super::*;

fn command(name: &str, arguments: &Value) -> Value {
    json!({ "command": format!("osprey.lspProfiler.{name}"), "arguments": arguments })
}

fn error_code(reply: &Message) -> i32 {
    reply.error.as_ref().map_or(0, |error| error.code)
}

#[tokio::test]
async fn initialize_advertises_the_profiler_commands_and_runtime() {
    let mut h = Harness::start();
    let init = h.request(1, "initialize", json!({})).await;
    let result = init.result.expect("initialize result");
    let commands = result
        .pointer("/capabilities/executeCommandProvider/commands")
        .and_then(Value::as_array)
        .expect("commands");
    assert!(
        commands.contains(&json!("osprey.lspProfiler.cpu.start")),
        "{commands:?}"
    );
    let runtime = result
        .pointer("/capabilities/experimental/lspProfiler/runtimes/0")
        .expect("a runtime");
    assert_eq!(runtime.get("id"), Some(&json!("osprey")));
    h.shutdown_and_exit().await;
}

#[tokio::test]
async fn profiler_commands_answer_with_the_protocols_codes() {
    let mut h = Harness::start();
    let caps = h
        .request(
            2,
            "workspace/executeCommand",
            command("capabilities", &json!([{}])),
        )
        .await;
    assert!(
        caps.result
            .as_ref()
            .and_then(|r| r.get("runtimes"))
            .is_some(),
        "{caps:?}"
    );
    let malformed = h
        .request(
            3,
            "workspace/executeCommand",
            command("cpu.start", &json!(["pid=1"])),
        )
        .await;
    assert_eq!(error_code(&malformed), -32602);
    let missing = h
        .request(
            4,
            "workspace/executeCommand",
            command("cpu.start", &json!([{ "pid": 999_999 }])),
        )
        .await;
    assert_eq!(error_code(&missing), -32001);
    let other = h
        .request(
            5,
            "workspace/executeCommand",
            json!({ "command": "osprey.build" }),
        )
        .await;
    assert_eq!(error_code(&other), METHOD_NOT_FOUND);
    h.shutdown_and_exit().await;
}

#[tokio::test]
async fn a_cancelled_profiler_request_is_answered_as_cancelled() {
    let mut h = Harness::start();
    // Listing processes samples CPU usage twice, so it is still running when
    // the cancel, read right after it, arrives.
    let params = command("processes", &json!([{ "workspace_roots": [] }]));
    h.send(&Message::request(
        RequestId::Number(6),
        "workspace/executeCommand",
        params,
    ))
    .await;
    h.notify("$/cancelRequest", json!({ "id": 6 })).await;
    let reply = h.read_response_for(6).await;
    assert_eq!(error_code(&reply), -32800, "{reply:?}");
    h.assert_still_serving(7, "the loop serves on after a cancel")
        .await;
    h.shutdown_and_exit().await;
}
