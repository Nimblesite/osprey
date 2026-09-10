//! API pages from validated sources and the editor's actual inferred signatures.
//! Implements [DOC-EXPORT].

use super::{model::Page, options::Options};
use crate::document_entries::{collect, DocEntry};
use osprey_syntax::Flavor;
use serde_json::Value;
use std::collections::HashMap;
use std::io;

type Documented<'a> = (DocEntry, &'a osprey_project::SourceFile, bool);

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
    render_pages(documented, &symbols)
}

fn source_root(path: &str) -> io::Result<std::path::PathBuf> {
    let selected = std::fs::canonicalize(path)?;
    if selected.is_dir() {
        return Ok(selected);
    }
    Ok(selected
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .to_path_buf())
}

fn public_entries<'a>(
    sources: &'a crate::document_source::SourceSet,
    input: &crate::project::CompilationInput,
    root: &std::path::Path,
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

fn file_name(source: &osprey_project::SourceFile, root: &std::path::Path) -> io::Result<String> {
    let canonical = std::fs::canonicalize(&source.path)?;
    let relative = canonical
        .strip_prefix(root)
        .ok()
        .unwrap_or_else(|| std::path::Path::new(source.path.file_name().unwrap_or_default()));
    Ok(format!("File {}", relative.display()))
}

fn render_pages(documented: Vec<Documented<'_>>, symbols: &[Value]) -> io::Result<Vec<Page>> {
    let entries: Vec<_> = documented
        .iter()
        .map(|(entry, _, _)| entry.clone())
        .collect();
    let slugs = super::user_slug_map(&entries);
    let mut pages = Vec::new();
    for (entry, source, opaque) in documented {
        let local = local_symbols(symbols, &source.path)?;
        merge_page(
            &mut pages,
            page(&entry, &local, &slugs, source.flavor, opaque)?,
        );
    }
    add_members(&mut pages);
    Ok(pages)
}

fn local_symbols(symbols: &[Value], source: &std::path::Path) -> io::Result<Vec<Value>> {
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
            let summary = page.summary.replace('|', "\\|").replace('\n', " ");
            Some(format!(
                "| [{name}]({slug}.md) | {} | {summary} |",
                page.group
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
    let root = source
        .path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
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
    flavor: Flavor,
    opaque: bool,
) -> io::Result<Page> {
    let slug = slugs
        .get(&entry.qualified_name)
        .ok_or_else(|| invalid("missing API page slug"))?;
    let signature = signature(entry, symbols)?;
    let signature = osprey_lsp::source_signature(flavor, &signature);
    let docs = entry.markdown().replace(
        "```osprey\n",
        &format!("```{}\n", osprey_lsp::source_fence(flavor)),
    );
    let markdown = format!(
        "# {}\n\n```{}\n{}\n```\n\n{}\n\n{}\n",
        entry.qualified_name,
        osprey_lsp::source_fence(flavor),
        signature,
        docs,
        super::declarations::details(entry.declaration.as_ref(), opaque)
    );
    Ok(Page {
        slug: format!("api/{slug}"),
        title: entry.qualified_name.clone(),
        group: entry.kind.into(),
        summary: entry.doc.summary.clone(),
        markdown,
    })
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
