//! API pages from validated sources and the editor's actual inferred signatures.
//! Implements [DOC-EXPORT].

use super::{model::Page, options::Options};
use crate::document_entries::{collect, DocEntry};
use osprey_syntax::Flavor;
use serde_json::Value;
use std::collections::HashMap;
use std::io;
use std::path::Path;

type Documented<'a> = (DocEntry, &'a osprey_project::SourceFile, bool);

/// Where a page's declaration came from, as the page needs to present it: the
/// flavor it was authored in, whether its representation stays hidden, and the
/// project-relative file it is written in.
struct Origin {
    flavor: Flavor,
    opaque: bool,
    location: String,
}

pub(super) fn pages(options: &Options) -> io::Result<Vec<Page>> {
    let Some(path) = &options.source else {
        return Ok(Vec::new());
    };
    let cli = source_cli(path, options.flavor)?;
    let sources = crate::document_source::load(&cli).map_err(invalid)?;
    let input = sources.input(&cli).map_err(invalid)?;
    if crate::report_type_errors(&input) != 0 {
        return Err(invalid("documentation source has type errors"));
    }
    let symbols: Vec<Value> =
        serde_json::from_str(&input.documentation_symbols_json()).map_err(io::Error::other)?;
    let root = source_root(path)?;
    let documented = public_entries(&sources, &input, &root)?;
    render_pages(documented, &symbols, &root)
}

fn source_root(path: &str) -> io::Result<std::path::PathBuf> {
    let selected = std::fs::canonicalize(path)?;
    if selected.is_dir() {
        return Ok(selected);
    }
    Ok(selected
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf())
}

fn public_entries<'a>(
    sources: &'a crate::document_source::SourceSet,
    input: &crate::project::CompilationInput,
    root: &Path,
) -> io::Result<Vec<Documented<'a>>> {
    let mut documented = Vec::new();
    for source in &sources.sources {
        for mut entry in collect(&source.program) {
            entry.symbol_name = resolved_name(&entry, source, sources.config.as_ref());
            if entry.kind == "File" {
                entry.qualified_name = file_name(source, root)?;
            }
            let (public, opaque) = visibility(&entry, input.public_api());
            if public {
                documented.push((entry, source, opaque));
            }
        }
    }
    Ok(documented)
}

fn file_name(source: &osprey_project::SourceFile, root: &Path) -> io::Result<String> {
    Ok(format!("File {}", relative_path(source, root)?))
}

/// A source file as the reader would name it in their own checkout: relative to
/// the documented root, or bare where it sits outside one.
fn relative_path(source: &osprey_project::SourceFile, root: &Path) -> io::Result<String> {
    let canonical = std::fs::canonicalize(&source.path)?;
    let relative = canonical
        .strip_prefix(root)
        .ok()
        .unwrap_or_else(|| Path::new(source.path.file_name().unwrap_or_default()));
    Ok(relative.display().to_string())
}

fn render_pages(
    documented: Vec<Documented<'_>>,
    symbols: &[Value],
    root: &Path,
) -> io::Result<Vec<Page>> {
    let entries: Vec<_> = documented
        .iter()
        .map(|(entry, _, _)| entry.clone())
        .collect();
    let slugs = super::user_slug_map(&entries);
    let mut pages = Vec::new();
    for (entry, source, opaque) in documented {
        let local = local_symbols(symbols, &source.path)?;
        let origin = origin(source, opaque, root)?;
        merge_page(&mut pages, page(&entry, &local, &slugs, &origin)?);
    }
    add_members(&mut pages);
    Ok(pages)
}

fn origin(source: &osprey_project::SourceFile, opaque: bool, root: &Path) -> io::Result<Origin> {
    Ok(Origin {
        flavor: source.flavor,
        opaque,
        location: relative_path(source, root)?,
    })
}

fn local_symbols(symbols: &[Value], source: &Path) -> io::Result<Vec<Value>> {
    let canonical = std::fs::canonicalize(source)?;
    Ok(symbols
        .iter()
        .filter(|symbol| {
            symbol
                .get("path")
                .and_then(Value::as_str)
                .is_none_or(|path| std::fs::canonicalize(path).is_ok_and(|path| path == canonical))
        })
        .cloned()
        .collect())
}

fn add_members(pages: &mut [Page]) {
    let indexes: Vec<_> = pages.iter().map(|page| member_table(page, pages)).collect();
    for (page, members) in pages.iter_mut().zip(indexes) {
        page.markdown.push_str(&members);
    }
}

fn member_table(owner: &Page, pages: &[Page]) -> String {
    if !matches!(owner.group.as_str(), "Module" | "Namespace" | "Effect") {
        return String::new();
    }
    let prefix = format!("{}::", owner.title);
    let rows = pages
        .iter()
        .filter_map(|page| {
            let name = page.title.strip_prefix(&prefix)?;
            if name.contains("::") {
                return None;
            }
            let slug = page.slug.strip_prefix("api/")?;
            Some(format!(
                "| [{name}]({slug}.md) | {} | {} |",
                page.group,
                cell(page, name)
            ))
        })
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return String::new();
    }
    format!(
        "\n## Members\n\n| Name | Kind | Description |\n| --- | --- | --- |\n{}\n",
        rows.join("\n")
    )
}

/// What a member listing says about one member: its summary where the author
/// wrote one, and otherwise its signature. A column of blank cells tells a
/// reader nothing, while the signature is the fact they came for.
fn cell(page: &Page, name: &str) -> String {
    let description = if page.summary.is_empty() {
        format!("`{}`", unqualified(&page.signature, name))
    } else {
        page.summary.clone()
    };
    description.replace('|', "\\|").replace('\n', " ")
}

/// A member's signature with the qualification taken off its head.
///
/// The Name column already carries it, and a table repeating it in every row
/// pushes the part a reader came for off the edge of a phone screen. The
/// qualification a signature carries is the compiler's finalized one — an
/// assembled project prefixes a namespace nobody wrote — so the head is found
/// by the member's own name rather than by the name of the page. An effect
/// operation is qualified with dots rather than `::`, so both are tried.
fn unqualified(signature: &str, name: &str) -> String {
    ["::", "."]
        .into_iter()
        .find_map(|separator| strip_head(signature, &format!("{separator}{name}"), name))
        .unwrap_or_else(|| signature.to_owned())
}

/// `signature` with the qualified name ending in `tail` reduced to `name`, or
/// `None` where it does not carry one.
fn strip_head(signature: &str, tail: &str, name: &str) -> Option<String> {
    let at = signature.find(tail)?;
    let head = signature.get(..at)?;
    let start = head
        .char_indices()
        .rev()
        .find(|(_, character)| !qualifier(*character))
        .map_or(0, |(at, character)| at.saturating_add(character.len_utf8()));
    Some(format!(
        "{}{name}{}",
        head.get(..start)?,
        signature.get(at.saturating_add(tail.len())..)?
    ))
}

/// A character that can appear inside a qualified name, and so belongs to the
/// head being removed rather than to the keyword or spacing in front of it.
fn qualifier(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '_' | ':' | '.')
}

fn visibility(
    entry: &DocEntry,
    public_api: Option<&std::collections::BTreeMap<String, bool>>,
) -> (bool, bool) {
    let Some(api) = public_api else {
        return (entry.public, false);
    };
    if matches!(entry.kind, "File" | "Namespace" | "Signature" | "Test") {
        return (entry.public, false);
    }
    let name = if entry.kind == "Operation" {
        entry
            .symbol_name
            .rsplit_once("::")
            .map_or(entry.symbol_name.as_str(), |pair| pair.0)
    } else {
        &entry.symbol_name
    };
    if let Some(opaque) = api.get(name) {
        return (true, *opaque);
    }
    (false, false)
}

fn resolved_name(
    entry: &DocEntry,
    source: &osprey_project::SourceFile,
    config: Option<&osprey_project::ProjectConfig>,
) -> String {
    let explicit = entry
        .scope
        .first()
        .and_then(|index| source.program.statements.get(*index))
        .is_some_and(|statement| matches!(statement, osprey_ast::Stmt::Namespace { .. }));
    if explicit || (config.is_none() && !osprey_project::needs_assembly(&source.program)) {
        return entry.qualified_name.clone();
    }
    let root = source.path.parent().unwrap_or_else(|| Path::new("."));
    let default = osprey_project::ProjectConfig::for_root(root);
    let config = config.unwrap_or(&default);
    let namespace = config.default_namespace.as_ref().unwrap_or(&config.name);
    format!("{namespace}::{}", entry.qualified_name)
}

fn merge_page(pages: &mut Vec<Page>, page: Page) {
    if let Some(existing) = pages.iter_mut().find(|existing| existing.slug == page.slug) {
        if existing.markdown != page.markdown {
            existing.markdown.push_str("\n\n");
            existing.markdown.push_str(&page.markdown);
        }
    } else {
        pages.push(page);
    }
}

fn source_cli(path: &str, flavor: Option<Flavor>) -> io::Result<crate::Cli> {
    let mut args = vec![path.into()];
    if let Some(flavor) = flavor {
        args.push("--flavor".into());
        args.push(
            match flavor {
                Flavor::Default => "default",
                Flavor::Ml => "ml",
            }
            .into(),
        );
    }
    crate::parse_args(&args).map_err(invalid)
}

fn page(
    entry: &DocEntry,
    symbols: &[Value],
    slugs: &HashMap<String, String>,
    origin: &Origin,
) -> io::Result<Page> {
    let slug = slugs
        .get(&entry.qualified_name)
        .ok_or_else(|| invalid("missing API page slug"))?;
    let signature = declared_signature(entry, symbols, origin)?;
    let markdown = article(entry, &signature, origin);
    Ok(Page {
        slug: format!("api/{slug}"),
        title: entry.qualified_name.clone(),
        group: entry.kind.into(),
        summary: entry.doc.summary.clone(),
        signature,
        markdown,
    })
}

/// Everything the page says, in reading order: the name, the signature, what
/// the author wrote, what the declaration's own shape adds.
fn article(entry: &DocEntry, signature: &str, origin: &Origin) -> String {
    let fence = osprey_lsp::source_fence(origin.flavor);
    body(&[
        format!("# {}", entry.qualified_name),
        format!("```{fence}\n{signature}\n```"),
        entry
            .markdown()
            .replace("```osprey\n", &format!("```{fence}\n")),
        super::declarations::details(entry.declaration.as_ref(), origin.opaque),
        super::facts::sections(entry, origin.flavor, &origin.location),
    ])
}

/// The page's Markdown, with the parts a declaration has nothing to say about
/// left out. A page is read, not just rendered: empty sections separated by
/// blank runs are noise in the `.md` output and in the search index alike.
fn body(parts: &[String]) -> String {
    let written: Vec<&str> = parts
        .iter()
        .map(String::as_str)
        .filter(|part| !part.trim().is_empty())
        .collect();
    format!("{}\n", written.join("\n\n"))
}

/// The signature line a page leads with: the editor's inferred type, respelled
/// in the flavor the declaration was authored in, carrying the effect row the
/// type model leaves off ([DOC-EXPORT]).
fn declared_signature(entry: &DocEntry, symbols: &[Value], origin: &Origin) -> io::Result<String> {
    let inferred = signature(entry, symbols)?;
    Ok(format!(
        "{}{}",
        osprey_lsp::source_signature(origin.flavor, &inferred),
        super::facts::effect_row(entry, origin.flavor)
    ))
}

fn signature(entry: &DocEntry, symbols: &[Value]) -> io::Result<String> {
    if entry.module_kind == Some(osprey_ast::ModuleKind::State) {
        return Ok(format!("state module {}", entry.qualified_name));
    }
    if let Some(osprey_ast::Stmt::Effect {
        stage, type_params, ..
    }) = &entry.declaration
    {
        let stage = if stage.is_compile_time() {
            "static "
        } else {
            ""
        };
        let binder = osprey_lsp::analysis::render_type_params(type_params);
        return Ok(format!("{stage}effect {}{binder}", entry.qualified_name));
    }
    if let Some(ty) = &entry.operation_type {
        return Ok(format!("{}: {ty}", entry.qualified_name.replace("::", ".")));
    }
    let name = entry.symbol_name.as_str();
    let exact = symbols
        .iter()
        .find(|symbol| symbol.get("name").and_then(Value::as_str) == Some(name));
    let symbol = match exact {
        Some(symbol) => Some(symbol),
        None => match unique_suffix(symbols, name)? {
            Some(symbol) => Some(symbol),
            None => local_binding(entry, symbols),
        },
    };
    match symbol {
        Some(symbol) => Ok(render_symbol(symbol, &entry.qualified_name)),
        None if matches!(entry.kind, "Function" | "Extern" | "Value") => {
            Err(invalid(format!("no inferred signature for {name}")))
        }
        None => Ok(format!(
            "{} {}",
            entry.kind.to_lowercase(),
            entry.qualified_name
        )),
    }
}

fn local_binding<'a>(entry: &DocEntry, symbols: &'a [Value]) -> Option<&'a Value> {
    let osprey_ast::Stmt::Let {
        name,
        position: Some(position),
        ..
    } = entry.declaration.as_ref()?
    else {
        return None;
    };
    symbols.iter().find(|symbol| {
        symbol.get("name").and_then(Value::as_str) == Some(name)
            && symbol.get("line").and_then(Value::as_u64) == Some(u64::from(position.line))
    })
}

fn unique_suffix<'a>(symbols: &'a [Value], name: &str) -> io::Result<Option<&'a Value>> {
    let suffix = format!("::{name}");
    let mut matches = symbols.iter().filter(|symbol| {
        symbol
            .get("name")
            .and_then(Value::as_str)
            .is_some_and(|candidate| candidate.ends_with(&suffix))
    });
    let first = matches.next();
    if matches.next().is_some() {
        return Err(invalid(format!("ambiguous API signature for {name}")));
    }
    Ok(first)
}

fn render_symbol(symbol: &Value, name: &str) -> String {
    if let Some(signature) = symbol.get("signature").and_then(Value::as_str) {
        return signature.into();
    }
    match symbol.get("type").and_then(Value::as_str) {
        Some(kind @ ("type" | "effect" | "module" | "namespace" | "signature")) => {
            format!("{kind} {name}")
        }
        Some(ty) => format!("{name}: {ty}"),
        None => name.into(),
    }
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
