//! The checker protocol of [TYPES-DISCIPLINE-PROTOCOL] (spec 0050): one JSON
//! request line on the checker's stdin, one JSON response line on its stdout.

use crate::{ProjectConfig, SourceFile};
use serde::{Deserialize, Serialize};

/// The protocol version this compiler speaks; a checker answering another is
/// a `discipline-error`.
pub const VERSION: u64 = 1;

/// The module handed to a checker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Module {
    /// Path as the user spelled it, for diagnostics only.
    pub path: String,
    /// `default`, `ml` or `python`.
    pub flavor: String,
    /// Full source text, marker included.
    pub source: String,
}

/// The export signature of one imported module, in ML signature syntax.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Import {
    /// Qualified module name.
    pub name: String,
    /// Discipline that produced the signature.
    pub discipline: String,
    /// `signature Name ...` text.
    pub signature: String,
}

/// Paths a checker may use for scratch and cache; it never reads sources itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Options {
    /// Project directory.
    pub project_root: String,
    /// Directory the checker may cache into.
    pub cache_dir: String,
}

/// One request: the module, the signatures of what it imports, and options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    /// Protocol version.
    pub version: u64,
    /// The module to check.
    pub module: Module,
    /// Imported signatures in dependency order.
    pub imports: Vec<Import>,
    /// Scratch locations.
    pub options: Options,
}

/// One diagnostic from a checker; the compiler prefixes `code` with `<name>/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Checker-chosen code.
    pub code: String,
    /// `error`, `warning` or `info`.
    pub severity: String,
    /// One-based line, zero-based column, when known.
    #[serde(default)]
    pub line: Option<usize>,
    /// Column of the diagnostic, when known.
    #[serde(default)]
    pub column: Option<usize>,
    /// Message text.
    pub message: String,
}

/// The erased module a checker wrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Erased {
    /// Flavor the erased source is written in.
    pub flavor: String,
    /// Erased source text, accepted by the HM pipeline as it is.
    pub source: String,
}

/// One response: `accepted` with a signature and an erased module, or
/// `rejected` with diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Response {
    /// Protocol version.
    pub version: u64,
    /// `accepted` or `rejected`.
    pub result: String,
    /// ML `signature` blocks, one per exported module.
    #[serde(default)]
    pub signature: String,
    /// The erased module, present when accepted.
    #[serde(default)]
    pub erased: Option<Erased>,
    /// Diagnostics, present when rejected and allowed as warnings when accepted.
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
}

impl Request {
    /// The request for `source`. Imported signatures are not yet supplied:
    /// the assembler does not expose per-module inferred signatures, so the
    /// list is empty until it does, and a checker must not depend on it.
    #[must_use]
    pub fn new(source: &SourceFile, config: &ProjectConfig) -> Self {
        Self {
            version: VERSION,
            module: Module {
                path: source.path.to_string_lossy().into_owned(),
                flavor: source.flavor.to_string(),
                source: source.source.clone(),
            },
            imports: Vec::new(),
            options: Options {
                project_root: config.directory.to_string_lossy().into_owned(),
                cache_dir: config
                    .directory
                    .join(".osprey/cache/disciplines")
                    .to_string_lossy()
                    .into_owned(),
            },
        }
    }

    /// The single request line.
    ///
    /// # Errors
    /// Serialization of plain strings cannot fail; the error is kept typed.
    pub fn to_line(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|error| error.to_string())
    }
}

impl Response {
    /// Parse the response line, checking the version and result words.
    ///
    /// # Errors
    /// Malformed JSON, a missing field, an unsupported version or an unknown
    /// result word are each reported with the text that caused them.
    pub fn parse(line: &str) -> Result<Self, String> {
        let response: Self = serde_json::from_str(line.trim())
            .map_err(|error| format!("answered with malformed JSON ({error})"))?;
        if response.version != VERSION {
            return Err(format!(
                "answered protocol version {} but this compiler speaks {VERSION}",
                response.version
            ));
        }
        match response.result.as_str() {
            "accepted" | "rejected" => Ok(response),
            other => Err(format!("answered with unknown result `{other}`")),
        }
    }

    /// Whether the checker accepted the module.
    #[must_use]
    pub fn accepted(&self) -> bool {
        self.result == "accepted"
    }
}
