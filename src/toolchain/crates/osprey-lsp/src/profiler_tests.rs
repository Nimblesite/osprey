//! Command recognition [PROF-WIRE-METHODS].
use super::*;

#[test]
fn profiler_commands_are_recognised_with_their_first_argument() {
    let params = json!({ "command": "osprey.lspProfiler.cpu.start", "arguments": [{ "pid": 7 }] });
    let (method, args) = profiler_call(&params).expect("a profiler command");
    assert_eq!(
        (method, args.get("pid")),
        ("lspProfiler/cpu/start", Some(&json!(7)))
    );
    assert_eq!(
        profiler_call(&json!({ "command": "osprey.lspProfiler.capabilities" })).map(|(_, a)| a),
        Some(json!({}))
    );
}

#[test]
fn other_commands_are_left_to_the_server() {
    assert!(profiler_call(&json!({ "command": "osprey.build" })).is_none());
    assert!(profiler_call(&json!({ "command": "sharplsp.lspProfiler.cpu.start" })).is_none());
}

#[test]
fn anything_but_one_params_object_is_malformed() {
    for arguments in [
        json!([]),
        json!([{}, {}]),
        json!([42]),
        json!(["pid=1"]),
        json!([null]),
        json!("pid=1"),
    ] {
        let params =
            json!({ "command": "osprey.lspProfiler.capabilities", "arguments": arguments });
        assert!(
            malformed_arguments(&params).is_some(),
            "{arguments} is not one params object"
        );
    }
    for params in [
        json!({}),
        json!({ "arguments": null }),
        json!({ "arguments": [{}] }),
    ] {
        assert_eq!(malformed_arguments(&params), None, "{params}");
    }
}

/// The service's notifications reach the client as framed JSON-RPC.
#[tokio::test]
async fn service_notifications_are_forwarded_to_the_client() {
    use lspkit_server::jsonrpc::{read_message, MessageWriter};
    let (server_writes, client_reads) = tokio::io::duplex(1 << 12);
    let writer: SharedWriter = Arc::new(MessageWriter::new(Box::new(server_writes)));
    let events = Events { writer };
    assert_eq!(format!("{events:?}"), "Events");
    events
        .notify("lspProfiler/cpu/progress", json!({ "sample_count": 3 }))
        .await;
    let mut reader = tokio::io::BufReader::new(client_reads);
    let message = read_message(&mut reader).await.expect("a notification");
    assert_eq!(message.method.as_deref(), Some("lspProfiler/cpu/progress"));
    assert_eq!(message.params, Some(json!({ "sample_count": 3 })));
}
