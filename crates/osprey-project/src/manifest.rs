//! Minimal, dependency-free reader for the project and module sections of
//! `osprey.toml`. Unknown keys are ignored so packaging/tooling sections can
//! evolve independently of the compiler.

use crate::ProjectError;
use osprey_syntax::Flavor;
use std::path::{Path, PathBuf};

/// Project assembly settings read from `osprey.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectConfig {
    /// Directory containing the manifest, used to resolve entry identity.
    pub directory: PathBuf,
    /// Human-readable project name.
    pub name: String,
    /// Directories scanned for production sources, relative to the manifest.
    pub source_roots: Vec<PathBuf>,
    /// Literal Python-flavor files selected without sweeping up Python tools.
    pub python_sources: Vec<PathBuf>,
    /// Optional default namespace for files without an explicit declaration.
    pub default_namespace: Option<String>,
    /// Optional entry source, relative to the manifest directory.
    pub entry: Option<PathBuf>,
    /// Optional project-wide flavor **override**, not a fallback: `parse_source`
    /// passes it as `resolve_flavor`'s `flag`, which is precedence level 1 in
    /// [FLAVOR-SELECT] (spec 0023) — the same level as `--flavor`. So it outranks
    /// both the `// osprey: flavor=` marker and the file extension, and a `.osp`
    /// file in a `flavor = "ml"` project is parsed as ML. Absent, selection falls
    /// through to marker > extension > Default per file.
    pub flavor: Option<Flavor>,
    /// Whether `::*` imports are permitted outside scripts and tests.
    pub allow_wildcard_imports: bool,
    /// Suppress application-only hierarchy and reverse-domain style advice.
    pub published_library: bool,
    /// `[project].types`: the type discipline of every source without a
    /// `// osprey: types=` marker ([TYPES-DISCIPLINE-SELECT], spec 0050).
    pub types: Option<String>,
    /// `[dependencies]` entries by package name; only the name and an optional
    /// local `path` are read here, the solver owns the rest (spec 0029).
    pub dependencies: Vec<Dependency>,
    /// The `[discipline]` table a package declares when it is a type discipline
    /// ([TYPES-DISCIPLINE-REGISTRY], spec 0050).
    pub discipline: Option<DisciplineDecl>,
}

/// One `[dependencies]` entry as the compiler reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    /// The `@org/name` package identity.
    pub name: String,
    /// A workspace-local package directory, relative to the manifest, used
    /// when the package is not installed under `.osprey/packages`.
    pub path: Option<PathBuf>,
}

/// The `[discipline]` table of a type-discipline package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisciplineDecl {
    /// The word after `types=`.
    pub name: String,
    /// The checker program, relative to the manifest: an Osprey program with `main`.
    pub checker: PathBuf,
    /// Per-module time limit for the checker process.
    pub timeout_seconds: u64,
}

/// The default `[discipline].timeout_seconds`.
pub(crate) const DEFAULT_TIMEOUT_SECONDS: u64 = 600;

impl ProjectConfig {
    /// Whether a saved or unsaved path belongs to the configured source roots.
    #[must_use]
    pub fn contains_source(&self, root: &Path, path: &Path) -> bool {
        crate::source::contains(root, self, path)
    }

    /// Defaults for a manifest-free directory project.
    #[must_use]
    pub fn for_root(root: &Path) -> Self {
        let name = root
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("app")
            .to_string();
        let source = if root.join("src").is_dir() {
            "src"
        } else {
            "."
        };
        Self {
            directory: root.to_path_buf(),
            default_namespace: Some(name.clone()),
            name,
            source_roots: vec![PathBuf::from(source)],
            python_sources: Vec::new(),
            entry: None,
            flavor: None,
            allow_wildcard_imports: false,
            published_library: false,
            types: None,
            dependencies: Vec::new(),
            discipline: None,
        }
    }

    /// Parse the supported `osprey.toml` keys.
    ///
    /// # Errors
    ///
    /// Returns every malformed supported manifest value with its source line.
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
        ("project", "python_sources") => parse_list(value).map(|paths| {
            config.python_sources = paths.into_iter().map(PathBuf::from).collect();
        }),
        ("project", "default_namespace") => {
            parse_string(value).map(|v| config.default_namespace = Some(v))
        }
        ("project", "entry") => parse_string(value).map(|v| config.entry = Some(v.into())),
        ("project", "flavor") => parse_flavor(value).map(|v| config.flavor = Some(v)),
        ("modules", "allow_wildcard_imports") => {
            parse_bool(value).map(|v| config.allow_wildcard_imports = v)
        }
        ("modules", "published_library") => parse_bool(value).map(|v| config.published_library = v),
        ("project", "types") => parse_string(value).map(|v| config.types = Some(v)),
        ("dependencies", name) => {
            parse_dependency(name, value).map(|d| config.dependencies.push(d))
        }
        ("discipline", key) => apply_discipline_key(key, value, config),
        _ => return,
    };
    if let Err(message) = result {
        errors.push(ProjectError::manifest(line, message));
    }
}

/// `"@org/name" = { epoch = 1, min_release = 1 }` or `{ path = "../pkg" }`.
fn parse_dependency(name: &str, value: &str) -> Result<Dependency, String> {
    let name = parse_string(name)?;
    let inner = value
        .strip_prefix('{')
        .and_then(|rest| rest.strip_suffix('}'))
        .ok_or_else(|| "expected an inline table".to_string())?;
    let path = inner
        .split(',')
        .filter_map(|field| field.split_once('='))
        .find(|(key, _)| key.trim() == "path")
        .map(|(_, path)| parse_string(path.trim()).map(PathBuf::from))
        .transpose()?;
    Ok(Dependency { name, path })
}

fn apply_discipline_key(key: &str, value: &str, config: &mut ProjectConfig) -> Result<(), String> {
    let decl = config.discipline.get_or_insert_with(|| DisciplineDecl {
        name: String::new(),
        checker: PathBuf::new(),
        timeout_seconds: DEFAULT_TIMEOUT_SECONDS,
    });
    match key {
        "name" => parse_string(value).map(|v| decl.name = v),
        "checker" => parse_string(value).map(|v| decl.checker = PathBuf::from(v)),
        "timeout_seconds" => value
            .parse::<u64>()
            .map(|v| decl.timeout_seconds = v)
            .map_err(|error| format!("expected a whole number of seconds ({error})")),
        _ => Ok(()),
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
    inner
        .split(',')
        .map(|item| parse_string(item.trim()))
        .collect()
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

    #[test]
    fn parses_discipline_selection_registry_and_dependencies() {
        // Implements [TYPES-DISCIPLINE-SELECT] and [TYPES-DISCIPLINE-REGISTRY].
        let text = "[project]\nname = \"proofs\"\ntypes = \"bounded\"\n\
                    [dependencies]\n\"@osprey/bounded\" = { path = \"../bounded\" }\n\
                    \"@osprey/testing\" = { epoch = 1, min_release = 1 }\n\
                    [discipline]\nname = \"bounded\"\nchecker = \"src/checker.ospml\"\n\
                    timeout_seconds = 30\n";
        let config = match ProjectConfig::parse(text, Path::new("/tmp/app/osprey.toml")) {
            Ok(value) => value,
            Err(errors) => panic!("unexpected manifest errors: {errors:?}"),
        };
        assert_eq!(config.types.as_deref(), Some("bounded"));
        assert_eq!(
            config.dependencies,
            vec![
                Dependency {
                    name: "@osprey/bounded".into(),
                    path: Some("../bounded".into())
                },
                Dependency {
                    name: "@osprey/testing".into(),
                    path: None
                },
            ]
        );
        assert_eq!(
            config.discipline,
            Some(DisciplineDecl {
                name: "bounded".into(),
                checker: "src/checker.ospml".into(),
                timeout_seconds: 30,
            })
        );
    }

    #[test]
    fn discipline_timeout_defaults_and_rejects_non_numbers() {
        let text = "[discipline]\nname = \"x\"\nchecker = \"c.ospml\"\n";
        let config = ProjectConfig::parse(text, Path::new("osprey.toml"));
        assert_eq!(
            config
                .ok()
                .and_then(|c| c.discipline)
                .map(|d| d.timeout_seconds),
            Some(DEFAULT_TIMEOUT_SECONDS)
        );
        let bad = "[discipline]\ntimeout_seconds = soon\n[dependencies]\n\"@o/p\" = 3\n";
        assert_eq!(
            ProjectConfig::parse(bad, Path::new("osprey.toml"))
                .err()
                .map(|e| e.len()),
            Some(2)
        );
    }
}
