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

/// The decimal magnitude of `i64::MIN`. One larger than `i64::MAX`, so it is a
/// valid literal ONLY under a unary minus — both flavors accept exactly that
/// spelling and reject every other use, so a twin pair agrees on the full int
/// domain ([ARITH-NEG-LITERAL], [FLAVOR-EQUIVALENCE]).
pub(crate) const I64_MIN_MAGNITUDE: &str = "9223372036854775808";

mod default;
mod desugar;
mod docparse;
mod kernel;
mod ml;
mod positional;
mod strings;
#[cfg(test)]
mod test_support;

pub use default::parse_tree;
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
    let parsed = match flavor {
        Flavor::Default => default::parse(source),
        Flavor::Ml => ml::parse_ml(source),
    };
    discharge_static_handlers(parsed)
}

/// Every function's dependency set — the static-effect operations it requires,
/// transitively, minus what it answers itself — paired with the syntax errors
/// found deriving them. Implements [STAGE-SIGNALS-DIRTY]
/// (docs/specs/0035-StagedEffects.md).
///
/// Computed on the program **before** static discharge, because discharge is
/// what makes those reads free and this is what makes them visible. Every
/// other entry point returns a discharged program, in which the dependency set
/// no longer exists to be read.
///
/// Parsing is best-effort, so a source that did not parse still yields a tree —
/// and the dependency sets read off it are silently short. "This view reads no
/// signals" and "this file did not parse" print identically, and a dirty set
/// that is wrongly empty is a subtree that never rebuilds: the exact class of
/// bug [STAGE-SIGNALS-DIRTY] claims to remove. So any caller answering a human
/// or a build MUST surface these errors rather than the sets alone, which is
/// the nonzero exit [STAGE-SIGNALS-EXACT] requires of `--deps`.
#[must_use]
pub fn dependency_report(
    source: &str,
    flavor: Flavor,
) -> (
    std::collections::BTreeMap<String, Vec<String>>,
    Vec<SyntaxError>,
) {
    let parsed = match flavor {
        Flavor::Default => default::parse(source),
        Flavor::Ml => ml::parse_ml(source),
    };
    (
        osprey_ast::stage::dependencies(&parsed.program),
        parsed.errors,
    )
}

/// Answer every `static` effect before the canonical program leaves the flavor
/// boundary, so no later phase — the checker, the GPU purity gate, codegen, the
/// language server — ever sees a compile-time effect. A static handler is a
/// lowering pass, and this is where lowering belongs. Implements [STAGE-LOWER],
/// [STAGE-LOWER-ORDER-PHASE] (docs/specs/0035-StagedEffects.md).
///
/// A program with no `static` effect is returned untouched ([STAGE-COMPAT]);
/// when a staging rule is violated the *undischarged* program is kept so
/// tolerant consumers (outline, test discovery) still see declarations, and the
/// violations join the syntax errors.
fn discharge_static_handlers(parsed: Parsed) -> Parsed {
    match osprey_ast::stage::discharge(&parsed.program) {
        Ok(program) => Parsed { program, ..parsed },
        Err(violations) => {
            let mut errors = parsed.errors;
            errors.extend(violations.into_iter().map(|violation| SyntaxError {
                message: violation.message,
                position: violation.position.unwrap_or_default(),
            }));
            Parsed { errors, ..parsed }
        }
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
pub(crate) fn flavor_from_extension(path: &str) -> Option<Flavor> {
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
    use osprey_ast::{Expr, Stmt};

    const NUMERIC_PREFIXES: [(Flavor, &str); 2] =
        [(Flavor::Default, "let value = "), (Flavor::Ml, "value = ")];

    /// [FLOAT-LITERAL-RANGE] Both flavors reject overflow at the source token.
    #[test]
    fn both_flavors_reject_overflowing_float_literals_with_positions() {
        let literal = format!("{}.0", "9".repeat(400));
        for (flavor, prefix) in NUMERIC_PREFIXES {
            for sign in ["", "-"] {
                let parsed =
                    parse_program_with_flavor(&format!("{prefix}{sign}{literal}\n"), flavor);
                assert_float_range_error(&parsed, &literal, prefix.len() + sign.len());
            }
        }
    }

    fn assert_float_range_error(parsed: &Parsed, literal: &str, column: usize) {
        assert_eq!(parsed.errors.len(), 1, "{:?}", parsed.errors);
        let error = parsed.errors.first().expect("one range diagnostic");
        assert_eq!(
            error.message,
            format!("float literal `{literal}` is outside the finite 64-bit range")
        );
        assert_eq!(error.position.line, 1);
        assert_eq!(
            usize::try_from(error.position.column).expect("source column"),
            column
        );
    }

    /// [FLOAT-LITERAL-RANGE] Rejecting infinity must retain finite boundaries.
    #[test]
    fn both_flavors_accept_finite_float_boundaries_and_signed_zero() {
        for literal in [
            "0.0".to_owned(),
            "2.5".to_owned(),
            format!("{}.0", f64::MAX),
        ] {
            for (flavor, prefix) in NUMERIC_PREFIXES {
                for sign in ["", "-"] {
                    let parsed =
                        parse_program_with_flavor(&format!("{prefix}{sign}{literal}\n"), flavor);
                    assert_finite_float(&parsed, sign == "-");
                }
            }
        }
    }

    fn assert_finite_float(parsed: &Parsed, negative: bool) {
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let Some(Stmt::Let {
            value: Expr::Float(value),
            ..
        }) = parsed.program.statements.first()
        else {
            panic!("expected a float binding: {:?}", parsed.program);
        };
        assert!(value.is_finite());
        assert_eq!(value.is_sign_negative(), negative);
    }

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

    /// The Default and ML spellings of one staged program, for the two surfaces
    /// added with the stage axis. Both must reach the SAME canonical AST, which
    /// is the whole content of [FLAVOR-BOUNDARY].
    const KERNEL_DEFAULT: &str = "static effect Tile { size: fn() -> int }\n\
fn shade(px) = (px * perform Tile.size()) ?: 0\n\
fn frame() = kernel\n    Tile size => 8\nin shade(2)\n";
    const KERNEL_ML: &str =
        "static effect Tile\n    size : Unit => int\n\nshade px = px * perform Tile.size () ?: 0\n\nframe =\n    kernel\n        Tile size => 8\n    in shade 2\n";

    /// [STAGE-GPU-KERNEL]: a `kernel` is a handler region, so it discharges by
    /// the same rewrite and leaves the same nothing behind — from either
    /// surface. Implements [STAGE-RESIDUE], [FLAVOR-BOUNDARY].
    #[test]
    fn both_flavors_discharge_a_kernel_region_to_zero_residue() {
        for (flavor, source) in [(Flavor::Default, KERNEL_DEFAULT), (Flavor::Ml, KERNEL_ML)] {
            let parsed = parse_program_with_flavor(source, flavor);
            assert!(parsed.errors.is_empty(), "{flavor:?}: {:?}", parsed.errors);
            let rendered = format!("{:?}", parsed.program);
            assert!(
                !rendered.contains("Handler"),
                "{flavor:?}: a discharged kernel must leave no handler"
            );
            assert!(
                !rendered.contains("Perform"),
                "{flavor:?}: a discharged kernel must leave no request"
            );
        }
    }

    /// [STAGE-SIGNALS-EXACT]: identity is the instantiation, and both surfaces
    /// spell the mention the same way, so a dependency set is the same set.
    #[test]
    fn both_flavors_name_a_signal_by_its_instantiation() {
        let default = "static effect Signal<T> { read: fn() -> T }\n\
fn counter(n) = (perform Signal<Count>.read()) ?: n\n";
        let ml = "static effect Signal T\n    read : Unit => T\n\ncounter n = perform Signal<Count>.read () ?: n\n";
        for (flavor, source) in [(Flavor::Default, default), (Flavor::Ml, ml)] {
            let deps = dependency_report(source, flavor).0;
            assert_eq!(
                deps.get("counter").cloned().unwrap_or_default(),
                vec!["Signal<Count>.read"],
                "{flavor:?}: the dependency is the instantiation, not the bare effect"
            );
        }
    }

    /// [STAGE-GPU-LEGAL]: a kernel body that still needs a runtime handler is
    /// rejected at the boundary, naming what forced it — device code cannot
    /// leave the device to reach one.
    #[test]
    fn a_kernel_body_with_a_residual_dynamic_row_is_rejected() {
        let source = "static effect Tile { size: fn() -> int }\n\
effect Log { write: fn(string) -> Unit }\n\
fn shade(px) = {\n    perform Log.write(\"px\")\n    (px * perform Tile.size()) ?: 0\n}\n\
fn frame() = kernel\n    Tile size => 8\nin shade(2)\n";
        let errors = parse_program_with_flavor(source, Flavor::Default).errors;
        assert!(
            errors.iter().any(|e| e.message.contains(
                "kernel body is not stage-legal; it requires dynamic effects: Log.write"
            )),
            "expected the stage-legality rejection, got: {errors:?}"
        );
    }

    /// [STAGE-SIGNALS-EXACT]: only the static stage can represent instantiation
    /// identity — a dynamic handler is keyed by effect name at runtime — so an
    /// instantiated mention of a dynamic effect is rejected rather than
    /// compiled to a key it shares with every other instantiation.
    #[test]
    fn an_instantiated_dynamic_effect_is_rejected_rather_than_shared() {
        let source = "effect Signal<T> { read: fn() -> T }\n\
fn counter(n) = (perform Signal<Count>.read()) ?: n\n";
        let errors = parse_program_with_flavor(source, Flavor::Default).errors;
        assert!(
            errors.iter().any(|e| e
                .message
                .contains("names an instantiation of dynamic effect `Signal`")),
            "expected the dynamic-instantiation rejection, got: {errors:?}"
        );
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
