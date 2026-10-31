//! The envelope `@osprey/native` encodes ([NATIVE-HOSTS-TRANSPORT]): the host
//! decodes it whole and refuses a version, patch or request it cannot read.

use serde::Deserialize;
use serde_json::{Map, Value};

/// The only protocol version this host speaks.
pub const VERSION: u32 = 1;

/// One envelope: the opaque session, the regions that moved, slice writes,
/// capability requests, diagnostics and, at start, the theme.
#[derive(Debug, Clone, Deserialize)]
pub struct Envelope {
    pub version: u32,
    pub state: String,
    #[serde(default)]
    pub slices: Vec<Slice>,
    pub patches: Vec<Patch>,
    #[serde(default)]
    pub removed: Vec<String>,
    pub requests: Vec<Request>,
    #[serde(default)]
    pub diagnostics: Vec<String>,
    #[serde(default)]
    pub theme: Option<Theme>,
}

/// Text the host holds under `key` until the runtime writes it again.
#[derive(Debug, Clone, Deserialize)]
pub struct Slice {
    pub key: String,
    pub text: String,
}

/// A region's new node tree; a host keeps its copy while the revision holds.
#[derive(Debug, Clone, Deserialize)]
pub struct Patch {
    pub key: String,
    pub revision: u64,
    pub node: Value,
}

/// A capability the application asks the host to answer under `ticket`.
#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    pub ticket: String,
    pub kind: String,
    /// The capability's own arguments, kept verbatim for the adapter that answers it.
    #[serde(flatten)]
    pub fields: Map<String, Value>,
}

/// One theme colour in both appearances.
// Decoded whole for the renderer; the window that resolves these fields is not
// built yet (plan 0036 phase 1).
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct Token {
    pub name: String,
    pub light: String,
    pub dark: String,
}

/// The application's theme: colour tokens, a spacing scale and motion durations.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct Theme {
    pub tokens: Vec<Token>,
    pub spacing: Vec<i64>,
    pub feedback: i64,
    pub settle: i64,
}

/// Decodes one envelope, refusing any other protocol version ([NATIVE-HOSTS-TRANSPORT]).
pub fn parse(text: &str) -> Result<Envelope, String> {
    let envelope: Envelope = serde_json::from_str(text)
        .map_err(|problem| format!("The Osprey envelope could not be decoded: {problem}"))?;
    match envelope.version == VERSION {
        true => accepted(envelope),
        false => Err(format!(
            "Unsupported Osprey protocol version {}; this host speaks {VERSION}",
            envelope.version
        )),
    }
}

/// Accepts the envelope once every frame it patches asks for nothing this host cannot draw ([UI-PROTOCOL]).
fn accepted(envelope: Envelope) -> Result<Envelope, String> {
    envelope
        .patches
        .iter()
        .try_for_each(|patch| viewports(&patch.key, &patch.node))
        .map(|()| envelope)
}

/// The only inset this host draws: a frame sized to its content.
const CONTENT_SIZED: i64 = -1;

/// Refuses a patch whose frames size themselves to the window or scroll, wherever they nest.
fn viewports(key: &str, node: &Value) -> Result<(), String> {
    let mut pending = vec![node];
    while let Some(item) = pending.pop() {
        match item {
            Value::Object(members) => {
                if members.get("kind").and_then(Value::as_str) == Some("framed") {
                    frame(key, members.get("style"))?;
                }
                pending.extend(members.values());
            }
            Value::Array(items) => pending.extend(items),
            _ => {}
        }
    }
    Ok(())
}

/// Checks one frame's style: its viewport inset and its vertical scrolling.
fn frame(key: &str, style: Option<&Value>) -> Result<(), String> {
    match style {
        None => Ok(()),
        Some(Value::Object(members)) => {
            inset(key, members.get("viewportHeightInset"))?;
            scrolled(key, members.get("scrollY"))
        }
        Some(found) => Err(needs(key, "an object style", found)),
    }
}

/// Refuses any inset but the content-sized default; only absence reads as that default.
fn inset(key: &str, value: Option<&Value>) -> Result<(), String> {
    match value {
        None => Ok(()),
        Some(found @ Value::Number(number)) => match number.as_i64() {
            Some(CONTENT_SIZED) => Ok(()),
            Some(whole) => Err(not_content_sized(key, whole)),
            None => Err(needs(key, "an integer viewportHeightInset", found)),
        },
        Some(found) => Err(needs(key, "an integer viewportHeightInset", found)),
    }
}

/// Refuses vertical scrolling this host cannot perform; only absence reads as unscrolled.
fn scrolled(key: &str, value: Option<&Value>) -> Result<(), String> {
    match value {
        None | Some(Value::Bool(false)) => Ok(()),
        Some(Value::Bool(true)) => Err(format!(
            "Frame scrollY in region {key} asks the frame to scroll, which this host does not draw; \
             the default is false"
        )),
        Some(found) => Err(needs(key, "a boolean scrollY", found)),
    }
}

/// The refusal for an inset other than the content-sized default.
fn not_content_sized(key: &str, whole: i64) -> String {
    format!(
        "Frame viewportHeightInset {whole} in region {key} is not the content-sized default \
         {CONTENT_SIZED}, which this host draws by sizing the frame to its content"
    )
}

/// The refusal for a present field of the wrong kind, named so the application's author can fix it.
fn needs(key: &str, wanted: &str, found: &Value) -> String {
    format!(
        "Frame in region {key} has {} where it needs {wanted}",
        shape(found)
    )
}

/// The JSON shape of a value, as the envelope's own diagnostics name it.
fn shape(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

#[cfg(test)]
mod tests {
    use super::parse;

    /// One envelope carrying a single patch node, as the framework would send it.
    fn envelope(node: &str) -> String {
        format!(
            r#"{{"version":1,"state":"s","requests":[],"patches":[{{"key":"root","revision":1,"node":{node}}}]}}"#
        )
    }

    /// The refusal for one patch node, or a message saying it was wrongly accepted.
    fn refusal(node: &str) -> String {
        match parse(&envelope(node)) {
            Ok(_) => String::from("accepted a frame this host cannot draw"),
            Err(problem) => problem,
        }
    }

    /// A frame the host cannot draw is refused by name, never drawn as if the field were unset.
    #[test]
    fn a_frame_sized_to_the_window_is_refused() {
        for inset in ["0", "400"] {
            let problem = refusal(&format!(
                r#"{{"kind":"framed","style":{{"viewportHeightInset":{inset}}},"body":{{"kind":"empty"}}}}"#
            ));
            assert!(problem.contains("viewportHeightInset"), "{problem}");
            assert!(problem.contains("-1"), "{problem}");
        }
    }

    /// A frame that asks to scroll vertically is refused by name.
    #[test]
    fn a_scrolling_frame_is_refused() {
        let problem =
            refusal(r#"{"kind":"framed","style":{"scrollY":true},"body":{"kind":"empty"}}"#);
        assert!(problem.contains("scrollY"), "{problem}");
        assert!(problem.contains("region root"), "{problem}");
    }

    /// A frame nested in a container, or inside another frame's body, is refused too.
    #[test]
    fn a_nested_frame_is_refused() {
        let child = refusal(
            r#"{"kind":"stack","children":[{"kind":"framed","style":{"viewportHeightInset":12},"body":{"kind":"empty"}}]}"#,
        );
        assert!(child.contains("viewportHeightInset"), "{child}");
        let body = refusal(
            r#"{"kind":"framed","style":{},"body":{"kind":"framed","style":{"scrollY":true},"body":{"kind":"empty"}}}"#,
        );
        assert!(body.contains("scrollY"), "{body}");
    }

    /// A present field of the wrong kind is malformed, not read as its default.
    #[test]
    fn a_malformed_viewport_field_is_refused_not_guessed() {
        for style in [
            r#"{"viewportHeightInset":"4"}"#,
            r#"{"viewportHeightInset":2.5}"#,
            r#"{"scrollY":1}"#,
            r#"{"scrollY":"true"}"#,
        ] {
            let problem = refusal(&format!(
                r#"{{"kind":"framed","style":{style},"body":{{"kind":"empty"}}}}"#
            ));
            assert!(problem.contains("where it needs"), "{problem}");
        }
    }

    const INVALID_FRAME_CASES: [(&str, &str); 5] = [
        (
            r#"{"kind":"framed","style":null,"body":{"kind":"empty"}}"#,
            "an object style",
        ),
        (
            r#"{"kind":"framed","style":{"viewportHeightInset":null}}"#,
            "an integer viewportHeightInset",
        ),
        (
            r#"{"kind":"framed","style":{"scrollY":null}}"#,
            "a boolean scrollY",
        ),
        (
            r#"{"kind":"framed","style":{"viewportHeightInset":-2}}"#,
            "viewportHeightInset",
        ),
        (
            r#"{"kind":"framed","style":{"viewportHeightInset":-9223372036854775808}}"#,
            "viewportHeightInset",
        ),
    ];

    /// Only absent or exactly the content-sized default is legal: null and any other inset are refused.
    #[test]
    fn only_absent_or_the_content_sized_default_is_accepted() {
        for (node, wanted) in INVALID_FRAME_CASES {
            let problem = refusal(node);
            assert!(problem.contains(wanted), "{node}: {problem}");
        }
    }

    /// The content-sized frame the desktop hosts draw stays decodable, fields absent or default.
    #[test]
    fn a_content_sized_frame_keeps_the_defaults() {
        for node in [
            r#"{"kind":"framed","body":{"kind":"empty"}}"#,
            r#"{"kind":"framed","style":{},"body":{"kind":"empty"}}"#,
            r#"{"kind":"framed","style":{"viewportHeightInset":-1,"scrollY":false},"body":{"kind":"empty"}}"#,
        ] {
            assert!(parse(&envelope(node)).is_ok(), "{node}");
        }
    }
}
