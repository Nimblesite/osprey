//! Flavor-agnostic frontend entry: source text in, canonical [`Program`] out.
//!
//! This crate hosts two **flavor** folders that each parse a source surface and
//! lower it to the one shared [`osprey_ast::Program`]: [`default`] (C-style
//! braces, tree-sitter) and [`ml`] (layout, curry-by-default, hand-written).
//! Everything in this module is flavor-neutral — the [`Flavor`] selector, the
//! [`Parsed`] result, and the dispatch that routes source to a flavor's
//! frontend. Past lowering, nothing may tell the flavors apart
//! ([FLAVOR-BOUNDARY], docs/specs/0023-LanguageFlavors.md). `parse_program` is
//! the public entry; errors are collected, never fatal, so the frontend never
//! panics on bad input and always produces a best-effort AST.

use osprey_ast::{Position, Program};

mod default;
mod docparse;
mod ml;
mod positional;
mod strings;

pub use default::{parse_tree, Lowerer};
pub use docparse::doc_links;

/// A syntax error located in the source (an ERROR/MISSING node from tree-sitter).
#[derive(Debug, Clone, PartialEq)]
pub struct SyntaxError {
    /// Human-readable description of what went wrong at this location.
    pub message: String,
    /// Source location (line/column) where the error was detected.
    pub position: Position,
}

/// A source **flavor**: a parser-and-lowering profile over the one shared
/// language core. Every flavor converges on the same canonical [`Program`]
/// before any semantic analysis runs, so nothing past lowering may inspect
/// which flavor produced a program. Implements [FLAVOR-BOUNDARY],
/// [FLAVOR-FRONTEND] (docs/specs/0023-LanguageFlavors.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Flavor {
    /// C-style braces, parens-and-named-argument calls, explicit currying. The
    /// language defined by specs 0001–0022; today's fully-implemented frontend.
    #[default]
    Default,
    /// Layout (offside-rule) blocks, whitespace application, curry-by-default.
    /// Surface specified in spec 0024; built by plan 0013 phases 2–3.
    Ml,
}

impl std::fmt::Display for Flavor {
    /// The canonical lowercase name used by the `--flavor` flag, the
    /// `// osprey: flavor=` marker, and diagnostics.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Flavor::Default => "default",
            Flavor::Ml => "ml",
        })
    }
}

impl std::str::FromStr for Flavor {
    type Err = String;

    /// Parse a flavor name (`default` | `ml`). Unknown names are an error so a
    /// typo fails loudly instead of silently selecting the Default frontend.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "default" => Ok(Flavor::Default),
            "ml" => Ok(Flavor::Ml),
            other => Err(format!("unknown flavor '{other}' (available: default, ml)")),
        }
    }
}

/// The result of lowering: the program plus any syntax errors. Errors being
/// non-empty does not prevent producing a best-effort tree.
#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    /// The lowered program (best-effort even when errors are present).
    pub program: Program,
    /// Syntax errors discovered while parsing; empty on a clean parse.
    pub errors: Vec<SyntaxError>,
    /// The flavor this source was parsed under. Carried for diagnostic
    /// rendering only — no semantic phase may branch on it ([FLAVOR-BOUNDARY]).
    pub flavor: Flavor,
}

/// Parse Osprey source into a typed [`Program`] using the **Default** flavor.
///
/// The signature is unchanged so every existing caller is unaffected: Default
/// stays the default API. Implements [FLAVOR-FRONTEND].
#[must_use]
pub fn parse_program(source: &str) -> Parsed {
    parse_program_with_flavor(source, Flavor::Default)
}

/// Parse Osprey source under an explicit [`Flavor`], dispatching to that
/// flavor's frontend. Both frontends produce the same canonical [`Program`];
/// they meet at the AST and are indistinguishable from there on.
/// Implements [FLAVOR-FRONTEND], [FLAVOR-BOUNDARY].
#[must_use]
pub fn parse_program_with_flavor(source: &str, flavor: Flavor) -> Parsed {
    match flavor {
        Flavor::Default => default::parse(source),
        Flavor::Ml => ml::parse_ml(source),
    }
}

/// The value of a leading `// osprey: flavor=<name>` marker, if the source has
/// one (the space-less `//osprey: flavor=` spelling is accepted too). The marker
/// must appear before any code so flavor selection never depends on a deep scan.
fn flavor_marker(source: &str) -> Option<&str> {
    source.lines().find_map(|line| {
        let t = line.trim();
        t.strip_prefix("// osprey: flavor=")
            .or_else(|| t.strip_prefix("//osprey: flavor="))
            .map(str::trim)
    })
}

/// The flavor implied by a path's extension: `.ospml` ⇒ ML, `.osp` ⇒ Default.
/// Any other extension yields `None` (no opinion). [FLAVOR-SELECT]
#[must_use]
pub fn flavor_from_extension(path: &str) -> Option<Flavor> {
    match std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
    {
        Some("ospml") => Some(Flavor::Ml),
        Some("osp") => Some(Flavor::Default),
        _ => None,
    }
}

/// Resolve a compilation unit's flavor by precedence: explicit `flag` >
/// file marker > extension > Default. A marker and extension that disagree are a
/// hard error rather than a silent guess, so the CLI and the editor agree on the
/// same frontend for the same file. Implements [FLAVOR-SELECT]
/// (docs/specs/0023-LanguageFlavors.md).
///
/// # Errors
/// Returns the disagreement message when a marker and the extension select
/// different flavors, or the parse error when the marker names an unknown flavor.
pub fn resolve_flavor(flag: Option<Flavor>, path: &str, source: &str) -> Result<Flavor, String> {
    if let Some(f) = flag {
        return Ok(f);
    }
    let marker = match flavor_marker(source) {
        Some(value) => Some(value.parse::<Flavor>()?),
        None => None,
    };
    match (marker, flavor_from_extension(path)) {
        (Some(m), Some(e)) if m != e => Err(format!(
            "{path}: flavor marker (flavor={m}) and file extension (flavor={e}) disagree; \
             make them agree or pass --flavor to override"
        )),
        (Some(m), _) => Ok(m),
        (None, Some(e)) => Ok(e),
        (None, None) => Ok(Flavor::Default),
    }
}

/// Parse `source` under the flavor resolved from `path` and any file marker,
/// falling back to Default when the two disagree or name an unknown flavor (an
/// editor surfaces such conflicts as ordinary diagnostics, never a hard stop).
/// This is the entry the LSP uses so `.ospml` is read with the ML frontend.
#[must_use]
pub fn parse_program_for_path(path: &str, source: &str) -> Parsed {
    let flavor = resolve_flavor(None, path, source).unwrap_or(Flavor::Default);
    parse_program_with_flavor(source, flavor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_flavor_follows_flag_marker_extension_precedence() {
        // Flag wins outright, overriding a disagreeing extension silently.
        assert_eq!(
            resolve_flavor(Some(Flavor::Default), "a.ospml", "").expect("ok"),
            Flavor::Default
        );
        // No flag: with a neutral extension, the marker decides.
        assert_eq!(
            resolve_flavor(None, "a.txt", "// osprey: flavor=ml\nx = 1\n").expect("ok"),
            Flavor::Ml
        );
        // No flag, no marker: extension decides for both Osprey extensions.
        assert_eq!(resolve_flavor(None, "a.ospml", "").expect("ok"), Flavor::Ml);
        assert_eq!(
            resolve_flavor(None, "a.osp", "").expect("ok"),
            Flavor::Default
        );
        // Nothing at all ⇒ Default.
        assert_eq!(
            resolve_flavor(None, "a.txt", "").expect("ok"),
            Flavor::Default
        );
        // Marker and extension that disagree are a hard error, not a guess.
        assert!(resolve_flavor(None, "a.osp", "// osprey: flavor=ml\n").is_err());
        // An unknown marker name fails loudly too.
        assert!(resolve_flavor(None, "a.txt", "// osprey: flavor=fsharp\n").is_err());
    }

    #[test]
    fn parse_program_for_path_selects_ml_for_ospml() {
        // The `.ospml` extension routes through the ML frontend, so a layout,
        // curry-by-default source parses cleanly where the Default frontend would
        // reject the bare `:` signature and `\` lambda.
        let ml = parse_program_for_path("tour.ospml", "inc : int -> int\ninc x = x + 1\n");
        assert!(ml.errors.is_empty(), "ml errors: {:?}", ml.errors);
        assert_eq!(ml.flavor, Flavor::Ml);
        // A `.osp` path stays on the Default frontend.
        let def = parse_program_for_path("m.osp", "fn inc(x: int) -> int = x + 1\n");
        assert!(def.errors.is_empty(), "default errors: {:?}", def.errors);
        assert_eq!(def.flavor, Flavor::Default);
    }
}
