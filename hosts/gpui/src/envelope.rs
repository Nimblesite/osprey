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
        true => Ok(envelope),
        false => Err(format!(
            "Unsupported Osprey protocol version {}; this host speaks {VERSION}",
            envelope.version
        )),
    }
}
