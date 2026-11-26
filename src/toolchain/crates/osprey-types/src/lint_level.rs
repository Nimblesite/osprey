//! Configurable lint severity. Implements [LINT-LEVELS].
//!
//! Every analyzer rule has a level: `allow` hides it, `info` shows it without
//! counting it, `warn` reports it as a warning and `deny` reports it as an
//! error that fails the build. A project sets any rule's level in the `[lints]`
//! table of its `osprey.toml`; a rule it leaves out takes its default, which
//! depends on whether the project is a package (its manifest has `[package]`).
//! An application's exports are its own, so documentation rules are
//! `allow` there and `deny` (`info` for examples) in a package.

use crate::{UNDOCUMENTED_EXPORT, UNEXAMPLED_EXPORT};

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
    /// The level `rule` reports at in this project.
    #[must_use]
    pub fn level(&self, rule: &str) -> LintLevel {
        self.overrides
            .iter()
            .rev()
            .find(|(name, _)| name == rule)
            .map_or_else(|| self.default_level(rule), |(_, level)| *level)
    }

    fn default_level(&self, rule: &str) -> LintLevel {
        match (rule, self.package) {
            (UNDOCUMENTED_EXPORT, true) => LintLevel::Deny,
            (UNEXAMPLED_EXPORT, true) => LintLevel::Info,
            (UNDOCUMENTED_EXPORT | UNEXAMPLED_EXPORT, false) => LintLevel::Allow,
            _ => LintLevel::Warn,
        }
    }
}
