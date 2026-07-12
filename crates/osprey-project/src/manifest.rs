//! Minimal, dependency-free reader for the project and module sections of
//! `osprey.toml`. Unknown keys are ignored so packaging/tooling sections can
//! evolve independently of the compiler.

use crate::ProjectError;
use osprey_syntax::Flavor;
use std::path::{Path, PathBuf};

/// Project assembly settings read from `osprey.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectConfig {
    /// Human-readable project name.
    pub name: String,
    /// Directories scanned for production sources, relative to the manifest.
    pub source_roots: Vec<PathBuf>,
    /// Optional default namespace for files without an explicit declaration.
    pub default_namespace: Option<String>,
    /// Optional entry source, relative to the manifest directory.
    pub entry: Option<PathBuf>,
    /// Optional project-wide fallback flavor.
    pub flavor: Option<Flavor>,
    /// Whether `::*` imports are permitted outside scripts and tests.
    pub allow_wildcard_imports: bool,
}

impl ProjectConfig {
    /// Defaults for a manifest-free directory project.
    #[must_use]
    pub fn for_root(root: &Path) -> Self {
        let name = root
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("app")
            .to_string();
        let source = if root.join("src").is_dir() { "src" } else { "." };
        Self {
            default_namespace: Some(name.clone()),
            name,
            source_roots: vec![PathBuf::from(source)],
            entry: None,
            flavor: None,
            allow_wildcard_imports: false,
        }
    }

    /// Parse the supported `osprey.toml` keys.
    pub fn parse(text: &str, path: &Path) -> Result<Self, Vec<ProjectError>> {
        let root = path.parent().unwrap_or_else(|| Path::new("."));
        let mut config = Self::for_root(root);
        let mut errors = Vec::new();
        let mut section = String::new();
        for (index, raw) in text.lines().enumerate() {
            parse_line(raw, index + 1, &mut section, &mut config, &mut errors);
        }
        if errors.is_empty() {
            Ok(config)
        } else {
            Err(errors)
        }
    }
}

fn parse_line(
    raw: &str,
    line: usize,
    section: &mut String,
    config: &mut ProjectConfig,
    errors: &mut Vec<ProjectError>,
) {
    let content = raw.split('#').next().unwrap_or_default().trim();
    if content.is_empty() {
        return;
    }
    if let Some(name) = content.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        section.clear();
        section.push_str(name.trim());
        return;
    }
    let Some((key, value)) = content.split_once('=') else {
        errors.push(ProjectError::manifest(line, "expected `key = value`"));
        return;
    };
    apply_key(section, key.trim(), value.trim(), line, config, errors);
}

fn apply_key(
    section: &str,
    key: &str,
    value: &str,
    line: usize,
    config: &mut ProjectConfig,
    errors: &mut Vec<ProjectError>,
) {
    let result = match (section, key) {
        ("project", "name") => parse_string(value).map(|v| config.name = v),
        ("project", "source_roots") => parse_list(value).map(|roots| {
            config.source_roots = roots.into_iter().map(PathBuf::from).collect();
        }),
        ("project", "default_namespace") => {
            parse_string(value).map(|v| config.default_namespace = Some(v))
        }
        ("project", "entry") => parse_string(value).map(|v| config.entry = Some(v.into())),
        ("project", "flavor") => parse_flavor(value).map(|v| config.flavor = Some(v)),
        ("modules", "allow_wildcard_imports") => {
            parse_bool(value).map(|v| config.allow_wildcard_imports = v)
        }
        _ => return,
    };
    if let Err(message) = result {
        errors.push(ProjectError::manifest(line, message));
    }
}

fn parse_string(value: &str) -> Result<String, String> {
    value
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .map(str::to_string)
        .ok_or_else(|| "expected a quoted string".to_string())
}

fn parse_list(value: &str) -> Result<Vec<String>, String> {
    let inner = value
        .strip_prefix('[')
        .and_then(|items| items.strip_suffix(']'))
        .ok_or_else(|| "expected a string list".to_string())?;
    if inner.trim().is_empty() {
        return Ok(Vec::new());
    }
    inner.split(',').map(|item| parse_string(item.trim())).collect()
}

fn parse_bool(value: &str) -> Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err("expected `true` or `false`".to_string()),
    }
}

fn parse_flavor(value: &str) -> Result<Flavor, String> {
    parse_string(value)?.parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_project_and_module_policy() {
        // Implements [MODULES-PROJECT].
        let text = "[project]\nname = \"billing\"\nsource_roots = [\"src\", \"generated\"]\n\
                    default_namespace = \"billing\"\nentry = \"src/main.ospml\"\nflavor = \"ml\"\n\
                    [modules]\nallow_wildcard_imports = true\n";
        let config = ProjectConfig::parse(text, Path::new("/tmp/app/osprey.toml"));
        let config = match config {
            Ok(value) => value,
            Err(errors) => panic!("unexpected manifest errors: {errors:?}"),
        };
        assert_eq!(config.name, "billing");
        assert_eq!(config.source_roots.len(), 2);
        assert_eq!(config.entry, Some(PathBuf::from("src/main.ospml")));
        assert_eq!(config.flavor, Some(Flavor::Ml));
        assert!(config.allow_wildcard_imports);
    }

    #[test]
    fn reports_bad_supported_values_and_ignores_other_sections() {
        let text = "[project]\nname = bare\n[package]\nversion = nope\n\
                    [modules]\nallow_wildcard_imports = maybe\n";
        let errors = ProjectConfig::parse(text, Path::new("osprey.toml"));
        let errors = match errors {
            Ok(value) => panic!("expected errors, got {value:?}"),
            Err(errors) => errors,
        };
        assert_eq!(errors.len(), 2);
    }
}
