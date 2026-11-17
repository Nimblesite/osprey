use super::{parse_script, Send};

/// Blank lines and comments do not become replay steps.
#[test]
fn skips_blank_and_comment_lines() {
    let steps = parse_script("\n// comment\n  // indented\n {\"send\":null,\"expect\":{}}\n");
    assert_eq!(steps.as_ref().map(Vec::len), Ok(1));
    assert!(steps.is_ok_and(|steps| steps[0].send.is_none()));
}

/// A malformed JSON line reports its original one-based line number.
#[test]
fn malformed_line_names_its_number() {
    let problem = parse_script("\n// comment\n{not json}\n");
    assert!(problem.is_err_and(|problem| problem.starts_with("step 3:")));
}

/// Every recorded send is the typed step it names, not a verbatim event.
#[test]
fn typed_steps_name_what_they_send() {
    let steps = parse_script(concat!(
        r#"{"send":{"control":{"label":"Dashed","words":""}},"expect":{}}"#,
        "\n",
        r#"{"send":{"pointer":{"x":1.0,"y":2.0,"down":true}},"expect":{}}"#,
        "\n",
        r#"{"send":{"key":{"key":"left","down":true}},"expect":{}}"#,
        "\n",
        r#"{"send":{"frame":0.016},"expect":{}}"#,
        "\n",
        r#"{"send":{"completed":{"ticket":"t","reason":"no"}},"expect":{}}"#,
        "\n",
        "// a comment\n",
    ))
    .expect("typed steps");
    assert!(matches!(&steps[0].send, Some(Send::Control(_))));
    assert!(matches!(&steps[1].send, Some(Send::Pointer(_))));
    assert!(matches!(&steps[2].send, Some(Send::Key(_))));
    assert!(matches!(&steps[3].send, Some(Send::Frame(_))));
    assert!(matches!(&steps[4].send, Some(Send::Completed(_))));
}

/// [SYNTH-DEVICE-QUEUE] Deferred completions preserve FIFO order and enforce bounded work.
#[test]
fn deferred_completion_queue_retains_order_and_rejects_overflow() -> Result<(), String> {
    use crate::rows::Ask;
    let ask = |ticket: &str| Ask {
        code: 1,
        ticket: ticket.to_owned(),
        fields: Vec::new(),
    };
    assert!(super::queued(&[ask("first"), ask("second")], false)?.is_none());
    assert_eq!(
        super::queued(&[], true)?.map(|a| a.ticket).as_deref(),
        Some("first")
    );
    assert_eq!(
        super::queued(&[ask("third")], true)?
            .map(|a| a.ticket)
            .as_deref(),
        Some("second")
    );
    assert_eq!(
        super::queued(&[], true)?.map(|a| a.ticket).as_deref(),
        Some("third")
    );
    assert!(super::queued(&vec![ask("held"); 1024], false)?.is_none());
    assert!(super::queued(&[ask("overflow")], true).is_err());
    for _ in 0..1024 {
        assert_eq!(
            super::queued(&[], true)?.map(|a| a.ticket).as_deref(),
            Some("held")
        );
    }
    assert!(super::queued(&[], true)?.is_none());
    Ok(())
}
