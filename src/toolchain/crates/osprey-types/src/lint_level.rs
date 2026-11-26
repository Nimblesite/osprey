//! Configurable lint severity. Implements [LINT-LEVELS].
//!
//! Every analyzer rule has a level: `allow` hides it, `info` shows it without
//! counting it, `warn` reports it as a warning and `deny` reports it as an
//! error that fails the build. A project sets any rule's level in the `[lints]`
//! table of its `osprey.toml`; a rule it leaves out takes its default, which
//! depends on whether the project is a package (its manifest has `[package]`).
//! An application's exports are its own, so documentation rules are
//! `allow` there and `deny` (`info` for examples) in a package.

use crate::{
    UnusedKind, CONSECUTIVE_PRINT, REDUNDANT_ANNOTATION, REDUNDANT_CALLBACK, REPEATED_LITERAL,
    UNDOCUMENTED_EXPORT, UNEXAMPLED_EXPORT, UNUSED_PRIVATE,
};

/// A module nested more than three levels deep ([MODULES-STYLE]).
pub const MODULE_DEEP_HIERARCHY: &str = "module-deep-hierarchy";
/// A module that owns state ([MODULES-STATE]).
pub const STATE_BOUNDARY: &str = "state-boundary";
/// One namespace spread over several folders ([MODULES-STYLE]).
pub const NAMESPACE_FOLDER_DRIFT: &str = "namespace-folder-drift";
/// A reverse-domain namespace label ([MODULES-STYLE]).
pub const NAMESPACE_REVERSE_DOMAIN: &str = "namespace-reverse-domain";
/// An import nothing reads ([MODULES-IMPORT]).
pub const UNUSED_IMPORT: &str = "unused-import";

/// Every analyzer rule a `[lints]` table may name. Any other diagnostic, such
/// as a type error or a skipped test, has no level a project can change.
pub const LINT_RULES: [&str; 17] = [
    UNDOCUMENTED_EXPORT,
    UNEXAMPLED_EXPORT,
    REDUNDANT_ANNOTATION,
    REDUNDANT_CALLBACK,
    REPEATED_LITERAL,
    CONSECUTIVE_PRINT,
    crate::source_warnings::CLOSING_RUN,
    UNUSED_PRIVATE,
    UnusedKind::Variable.rule(),
    UnusedKind::Parameter.rule(),
    UnusedKind::PatternBinding.rule(),
    UnusedKind::HandlerParameter.rule(),
    MODULE_DEEP_HIERARCHY,
    STATE_BOUNDARY,
    NAMESPACE_FOLDER_DRIFT,
    NAMESPACE_REVERSE_DOMAIN,
    UNUSED_IMPORT,
];

/// How loudly a rule reports its findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintLevel {
    /// Not reported at all.
    Allow,
    /// Shown, never counted, so `--warnings-as-errors` does not fail on it.
    Info,
    /// A warning: fails only under `--warnings-as-errors`.
    Warn,
    /// An error: always fails the build ([COMPILER-RED-FAILS]).
    Deny,
}

impl LintLevel {
    /// The level a `[lints]` value names.
    ///
    /// # Errors
    ///
    /// Returns a message naming the accepted levels for any other text.
    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "allow" => Ok(Self::Allow),
            "info" => Ok(Self::Info),
            "warn" => Ok(Self::Warn),
            "deny" => Ok(Self::Deny),
            other => Err(format!(
                "unknown lint level `{other}`; expected allow, info, warn or deny"
            )),
        }
    }
}

/// The lint levels of one project: its `[lints]` table over the defaults.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LintLevels {
    /// Whether the project is a package, which sets the documentation defaults.
    pub package: bool,
    /// `[lints]` entries in manifest order; a later entry wins.
    pub overrides: Vec<(String, LintLevel)>,
}

impl LintLevels {
    /// Set `rule` to `level`, as one `[lints]` line does.
    ///
    /// # Errors
    ///
    /// Returns a message naming the rule when it is not in [`LINT_RULES`].
    pub fn set(&mut self, rule: &str, level: LintLevel) -> Result<(), String> {
        if !LINT_RULES.contains(&rule) {
            return Err(format!("unknown lint `{rule}`; `[lints]` names an analyzer rule such as `{UNDOCUMENTED_EXPORT}`"));
        }
        self.overrides.push((rule.to_owned(), level));
        Ok(())
    }

    /// The level `rule` reports at in this project.
    #[must_use]
    pub fn level(&self, rule: &str) -> LintLevel {
        self.overrides
            .iter()
            .rev()
            .find(|(name, _)| name == rule)
            .map_or_else(|| self.default_level(rule), |(_, level)| *level)
    }

    /// A package documents its API and may nest and name its modules as a
    /// library does; an application keeps its exports to itself.
    fn default_level(&self, rule: &str) -> LintLevel {
        match (rule, self.package) {
            (UNDOCUMENTED_EXPORT, true) => LintLevel::Deny,
            (UNEXAMPLED_EXPORT, true) => LintLevel::Info,
            (UNDOCUMENTED_EXPORT | UNEXAMPLED_EXPORT, false)
            | (MODULE_DEEP_HIERARCHY | NAMESPACE_REVERSE_DOMAIN, true) => LintLevel::Allow,
            _ => LintLevel::Warn,
        }
    }
}
