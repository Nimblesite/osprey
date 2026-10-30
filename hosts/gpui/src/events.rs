//! The host's JSON half of the protocol ([NATIVE-HOSTS-TRANSPORT]): the events
//! `ui::Events` decodes and the launch context `ui::Runtime` reads. Building
//! them is all this seam does; what a control means is the application's.

use serde_json::Value;

/// A JSON string literal, escaped by the same writer that escapes values.
fn quoted(text: &str) -> String {
    Value::String(text.to_owned()).to_string()
}

/// A control event: the binding ordinal the framework issued, in its region
/// revision, with the value the control delivered.
pub fn control(now: i64, region: &str, revision: u64, ordinal: u64, value: Value) -> String {
    format!(
        "{{\"kind\":\"ui\",\"now\":{now},\"region\":{},\"revision\":{revision},\"ordinal\":{ordinal},\"value\":{value}}}",
        quoted(region)
    )
}

/// A capability completion that succeeded, delivered under its ticket.
pub fn completed_ok(now: i64, ticket: &str, result: Value) -> String {
    format!(
        "{{\"kind\":\"completed\",\"now\":{now},\"ticket\":{},\"ok\":true,\"result\":{result}}}",
        quoted(ticket)
    )
}

/// A capability completion that failed, delivered under its ticket.
pub fn completed_err(now: i64, ticket: &str, error: &str) -> String {
    format!(
        "{{\"kind\":\"completed\",\"now\":{now},\"ticket\":{},\"ok\":false,\"error\":{}}}",
        quoted(ticket),
        quoted(error)
    )
}

/// One display frame with its clamped elapsed seconds, always rendered as a float.
pub fn frame(now: i64, elapsed: f64) -> String {
    format!("{{\"kind\":\"frame\",\"now\":{now},\"elapsed\":{elapsed:?}}}")
}

/// A lifecycle phase, such as the window becoming visible.
pub fn lifecycle(now: i64, phase: &str) -> String {
    format!(
        "{{\"kind\":\"lifecycle\",\"now\":{now},\"phase\":{}}}",
        quoted(phase)
    )
}

/// The launch context: Unix seconds, the local UTC offset, the platform name,
/// the home folder and the arguments the application was opened with.
pub fn start_context(
    now: i64,
    offset: i64,
    platform: &str,
    home: &str,
    arguments: &[String],
) -> String {
    let listed = Value::Array(
        arguments
            .iter()
            .map(|argument| Value::String(argument.clone()))
            .collect(),
    );
    format!(
        "{{\"now\":{now},\"offset\":{offset},\"platform\":{},\"home\":{},\"arguments\":{listed}}}",
        quoted(platform),
        quoted(home)
    )
}
