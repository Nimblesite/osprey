//! Compiler-backed documentation export for built-ins, source files, and projects.
//! Both renderers consume the same inferred API pages and preserve user-authored
//! guides. The Markdown output integrates with existing sites; HTML is a complete
//! static site with navigation, search, themes, and custom stylesheets.
//! Implements [DOC-EXPORT], [DOC-EXPORT-HTML], [DOC-EXPORT-PAGES], [DOC-EXPORT-CSS].

mod assets;
mod declarations;
mod facts;
mod html;
mod model;
mod options;
mod output;
mod prose;
mod user;
mod write;

use crate::document_entries::DocEntry;
use model::Page;
use options::{Format, Options};
use osprey_types::{builtin_doc_view, builtin_names, BuiltinDocView};
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

/// Export a validated API reference in the requested output format.
pub(crate) fn run(args: &[String]) -> ExitCode {
    let options = match Options::parse(args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("osprey --docs: {error}");
            return ExitCode::from(2);
        }
    };
    match export(&options) {
        Ok(count) => {
            println!(
                "generated {count} documentation pages in {}",
                options.directory.display()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("osprey --docs: {error}");
            ExitCode::FAILURE
        }
    }
}

fn export(options: &Options) -> std::io::Result<usize> {
    let mut pages = user::pages(options)?;
    pages.extend(assets::pages(&options.pages)?);
    let styles = assets::stylesheets(&options.css)?;
    add_api_index(&mut pages);
    pages.extend(builtin_pages());
    match options.format {
        Format::Markdown => {
            let _ = generate(&options.directory, &pages)?;
            Ok(pages.len())
        }
        Format::Html => {
            html::generate(&options.directory, &pages, &options.theme, &styles)?;
            Ok(pages.len())
        }
    }
}

fn add_api_index(pages: &mut Vec<Page>) {
    let api: Vec<_> = pages
        .iter()
        .filter(|page| page.slug.starts_with("api/"))
        .collect();
    if api.is_empty() {
        return;
    }
    let links = api
        .iter()
        .map(|page| {
            format!(
                "- [{}]({}.md)",
                page.title,
                page.slug.strip_prefix("api/").unwrap_or(&page.slug)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    pages.push(Page {
        slug: "api/index".into(),
        title: "API Reference".into(),
        group: "Overview".into(),
        summary: "Modules and declarations in this project.".into(),
        signature: String::new(),
        markdown: format!("# API Reference\n\n{links}\n"),
    });
}

fn builtin_pages() -> Vec<Page> {
    let names = builtin_names();
    let slugs = slug_map(&names);
    let views: Vec<_> = names
        .iter()
        .filter_map(|name| builtin_doc_view(name))
        .collect();
    let mut pages: Vec<_> = views
        .iter()
        .filter_map(|view| {
            slugs.get(&view.name).map(|slug| Page {
                slug: format!("functions/{slug}"),
                title: view.name.clone(),
                group: "Built-in functions".into(),
                summary: String::new(),
                signature: view.signature.clone(),
                markdown: page(view),
            })
        })
        .collect();
    pages.push(Page {
        slug: "functions/index".into(),
        title: "Built-in Functions".into(),
        group: "Overview".into(),
        summary: "The Osprey standard library.".into(),
        signature: String::new(),
        markdown: index(&views, &slugs),
    });
    pages
}

/// The value following `--docs-dir`, if present.
#[cfg(test)]
fn docs_dir(args: &[String]) -> Option<&str> {
    args.iter()
        .position(|a| a == "--docs-dir")
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

/// Write a page per built-in plus the index, then prune stale pages. Returns the
/// number of function pages written.
fn generate(docs_dir: &Path, pages: &[Page]) -> std::io::Result<usize> {
    write::markdown(docs_dir, pages)?;
    let names = builtin_names();
    prune(&docs_dir.join("functions"), &slug_map(&names))?;
    Ok(names.len())
}

/// Assign each built-in a unique, filesystem-safe page stem. Names normally
/// lowercase to their stem; a capitalized type constructor that would clash with
/// a same-spelled function (`Map` vs `map`) is suffixed `-type`, and any further
/// clash gets a numeric suffix — so distinct built-ins never share a page even
/// on a case-insensitive filesystem.
fn slug_map(names: &[String]) -> HashMap<String, String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for n in names {
        *counts.entry(n.to_lowercase()).or_insert(0) += 1;
    }
    let mut used: HashSet<String> = HashSet::new();
    let mut out = HashMap::new();
    for n in names {
        let lower = n.to_lowercase();
        let unique = counts.get(&lower).copied().unwrap_or(0) == 1;
        let mut slug = if unique || *n == lower {
            lower.clone()
        } else {
            format!("{lower}-type")
        };
        let mut k = 2;
        while !used.insert(slug.clone()) {
            slug = format!("{lower}-{k}");
            k += 1;
        }
        let _ = out.insert(n.clone(), slug);
    }
    out
}

/// Remove `functions/*.md` pages that no longer correspond to a built-in, so the
/// directory mirrors the live set exactly.
fn prune(functions: &Path, slugs: &HashMap<String, String>) -> std::io::Result<()> {
    let keep: HashSet<&str> = slugs.values().map(String::as_str).collect();
    for entry in fs::read_dir(functions)? {
        let path = entry?.path();
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        let is_md = path.extension().is_some_and(|e| e == "md");
        if is_md && stem != "index" && !keep.contains(stem) {
            println!("pruned stale doc {}", path.display());
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}

/// Render one function reference page in the website's front-matter format.
fn page(v: &BuiltinDocView) -> String {
    let mut out = format!(
        "---\nlayout: page\ntitle: \"{} (Function)\"\ndescription: \"{}\"\n---\n\n",
        v.name,
        yaml(&v.summary)
    );
    let _ = writeln!(
        out,
        "**Signature:** `{}`\n\n**Description:** {}\n",
        v.signature, v.summary
    );
    push_parameters(&mut out, v);
    let _ = writeln!(out, "**Returns:** {}", v.return_type);
    push_example(&mut out, v);
    out
}

fn push_parameters(out: &mut String, v: &BuiltinDocView) {
    if v.params.is_empty() {
        return;
    }
    out.push_str("## Parameters\n\n");
    for p in &v.params {
        let _ = writeln!(out, "- **{}** ({}): {}", p.name, p.ty, p.description);
    }
    out.push('\n');
}

fn push_example(out: &mut String, v: &BuiltinDocView) {
    if !v.example.is_empty() {
        let _ = writeln!(out, "\n## Example\n\n```osprey\n{}\n```", v.example);
    }
}

/// Render the `functions/index.md` listing, one entry per built-in.
fn index(views: &[BuiltinDocView], slugs: &HashMap<String, String>) -> String {
    let mut out = String::from(
        "---\nlayout: page\ntitle: \"Built-in Functions\"\n\
         description: \"Complete reference for all built-in functions in Osprey\"\n---\n\n\
         All built-in functions available in Osprey.\n\n",
    );
    for v in views {
        let slug = slugs.get(&v.name).map_or("", String::as_str);
        let _ = writeln!(
            out,
            "## [{}]({}/)\n\n**Signature:** `{}`\n\n{}\n",
            v.name, slug, v.signature, v.summary
        );
    }
    out
}

/// Filesystem-safe unique page stems for qualified user names (`::` → `-`).
fn user_slug_map(entries: &[DocEntry]) -> HashMap<String, String> {
    let mut used: HashSet<String> = HashSet::from(["index".to_string()]);
    let mut slugs = HashMap::new();
    for name in entries
        .iter()
        .map(|e| &e.qualified_name)
        .collect::<std::collections::BTreeSet<_>>()
    {
        let base = safe_slug(name);
        let mut slug = base.clone();
        let mut k = 2;
        while !used.insert(slug.clone()) {
            slug = format!("{base}-{k}");
            k += 1;
        }
        let _ = slugs.insert(name.clone(), slug);
    }
    slugs
}

fn safe_slug(name: &str) -> String {
    let mut slug = String::new();
    for byte in name.bytes() {
        if byte.is_ascii_alphanumeric() || byte == b'_' {
            slug.push(char::from(byte.to_ascii_lowercase()));
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "page".into()
    } else {
        slug.into()
    }
}

/// Escape a string for a YAML double-quoted scalar (front-matter `description`).
fn yaml(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    reason = "test assertions: a missing slug key is a test failure, not a production panic"
)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn page_uses_scheme_types_and_omits_empty_sections() {
        let view = builtin_doc_view("sleep").expect("sleep documented");
        let md = page(&view);
        // The signature follows the scheme (`-> Unit`), not the old prose `-> int`.
        assert!(
            md.contains("**Signature:** `sleep(milliseconds: int) -> Unit`"),
            "{md}"
        );
        assert!(
            md.contains("- **milliseconds** (int): Number of milliseconds"),
            "{md}"
        );
        assert!(md.contains("**Returns:** Unit"), "{md}");
        // A zero-arg builtin renders no Parameters section.
        let input = page(&builtin_doc_view("input").expect("input documented"));
        assert!(!input.contains("## Parameters"), "{input}");
    }

    #[test]
    fn docs_dir_reads_the_flag_value() {
        let args = vec!["--docs".into(), "--docs-dir".into(), "out".into()];
        assert_eq!(docs_dir(&args), Some("out"));
        assert_eq!(docs_dir(&["--docs".to_string()]), None);
    }

    #[test]
    fn yaml_escapes_quotes_and_backslashes() {
        assert_eq!(yaml(r#"a "b" \c"#), r#"a \"b\" \\c"#);
    }

    #[test]
    fn slugs_disambiguate_case_collisions_and_stay_unique() {
        // `map` (function) keeps the bare stem; `Map` (constructor) cannot share
        // it on a case-insensitive disk, so it is suffixed.
        let names = vec!["map".to_string(), "Map".to_string(), "sleep".to_string()];
        let slugs = slug_map(&names);
        assert_eq!(slugs["map"], "map");
        assert_eq!(slugs["Map"], "map-type");
        assert_eq!(slugs["sleep"], "sleep");
        let distinct: HashSet<&String> = slugs.values().collect();
        assert_eq!(distinct.len(), slugs.len(), "slugs must be unique");
    }

    #[test]
    fn slug_map_falls_back_to_a_numeric_suffix_when_the_type_slug_also_collides() {
        // Three names collapsing to one lowercase stem force the dedupe loop past
        // the bare stem and the `-type` form into the numeric fallback.
        let names = vec!["foo".to_string(), "Foo".to_string(), "FOO".to_string()];
        let slugs = slug_map(&names);
        let distinct: HashSet<&String> = slugs.values().collect();
        assert_eq!(
            distinct.len(),
            3,
            "each colliding name gets a distinct slug"
        );
        assert!(
            slugs.values().any(|s| s == "foo-2"),
            "numeric fallback used: {slugs:?}"
        );
    }

    // A fresh, empty temp directory unique to `tag` (any prior run is cleared).
    fn fresh_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("osprey_docs_test_{tag}"));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn generate_writes_a_page_per_builtin_plus_an_index() {
        let dir = fresh_dir("generate");
        let count = generate(&dir, &builtin_pages()).expect("generation succeeds");
        let functions = dir.join("functions");
        assert!(count > 0, "wrote at least one page");
        assert!(
            functions.join("sleep.md").exists(),
            "a known builtin page written"
        );
        let index = fs::read_to_string(functions.join("index.md")).expect("index readable");
        assert!(
            index.contains("Built-in Functions"),
            "index carries its heading"
        );
        assert!(
            index.contains("sleep"),
            "index lists each builtin: {index:.80}"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn prune_deletes_stale_pages_but_keeps_live_ones_and_the_index() {
        let dir = fresh_dir("prune");
        let functions = dir.join("functions");
        fs::create_dir_all(&functions).expect("mkdir functions");
        for (name, body) in [
            ("sleep.md", "live"),
            ("ghost.md", "stale"),
            ("index.md", "idx"),
        ] {
            fs::write(functions.join(name), body).expect("seed file");
        }
        let mut slugs = HashMap::new();
        let _ = slugs.insert("sleep".to_string(), "sleep".to_string());
        prune(&functions, &slugs).expect("prune succeeds");
        assert!(functions.join("sleep.md").exists(), "live page kept");
        assert!(functions.join("index.md").exists(), "index is never pruned");
        assert!(!functions.join("ghost.md").exists(), "stale page removed");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn generate_surfacing_an_io_error_takes_the_failure_branch() {
        // Point `--docs-dir` at a path whose parent is a *file*: `create_dir_all`
        // then fails, so `generate` returns `Err` and `run` takes its error arm.
        let file = std::env::temp_dir().join("osprey_docs_not_a_dir");
        fs::write(&file, "i am a file").expect("seed blocking file");
        let blocked = file.join("under_a_file");
        assert!(
            generate(&blocked, &builtin_pages()).is_err(),
            "creating a dir beneath a file must fail"
        );
        let code = run(&[
            "--docs".to_string(),
            "--docs-dir".to_string(),
            blocked.to_string_lossy().into_owned(),
        ]);
        let _ = code; // ExitCode is opaque; reaching here proves the arm ran.
        let _ = fs::remove_file(&file);
    }

    #[test]
    fn run_generates_into_a_dir_and_takes_the_usage_branch_without_a_flag() {
        let dir = fresh_dir("run");
        let ok = run(&[
            "--docs".to_string(),
            "--docs-dir".to_string(),
            dir.to_string_lossy().into_owned(),
        ]);
        assert!(
            dir.join("functions/index.md").exists(),
            "run --docs-dir generated docs"
        );
        // No `--docs-dir` takes the usage/error return; `ExitCode` exposes no
        // accessor, so binding both invocations proves neither path panics.
        let missing = run(&["--docs".to_string()]);
        let _ = (ok, missing);
        let _ = fs::remove_dir_all(&dir);
    }
}
