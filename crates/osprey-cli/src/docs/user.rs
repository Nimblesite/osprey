//! API pages from validated sources and the editor's actual inferred signatures.
//! Implements [DOC-EXPORT].

use super::{model::Page, options::Options};
use crate::document_entries::{collect, DocEntry};
use osprey_syntax::Flavor;
use serde_json::Value;
use std::collections::HashMap;
use std::io;

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
        serde_json::from_str(&input.symbols_json()).map_err(io::Error::other)?;
    let selected = std::fs::canonicalize(path)?;
    let root = if selected.is_dir() {
        selected.as_path()
    } else {
        selected
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
    };
    let mut documented = Vec::new();
    for source in &sources.sources {
        for mut entry in collect(&source.program) {
            if entry.kind == "File" {
                let canonical = std::fs::canonicalize(&source.path)?;
                let relative = canonical.strip_prefix(root).ok().unwrap_or_else(|| {
                    std::path::Path::new(source.path.file_name().unwrap_or_default())
                });
                entry.qualified_name = format!("File {}", relative.display());
            }
            let (public, opaque) = visibility(&entry, input.public_api());
            if public {
                documented.push((entry, source, opaque));
            }
        }
    }
    let entries: Vec<_> = documented
        .iter()
        .map(|(entry, _, _)| entry.clone())
        .collect();
    let slugs = super::user_slug_map(&entries);
    let mut pages = Vec::new();
    for (entry, source, opaque) in documented {
        let canonical = std::fs::canonicalize(&source.path)?;
        let local_symbols: Vec<_> = symbols
            .iter()
            .filter(|symbol| {
                symbol
                    .get("path")
                    .and_then(Value::as_str)
                    .is_none_or(|path| {
                        std::fs::canonicalize(path).is_ok_and(|path| path == canonical)
                    })
            })
            .cloned()
            .collect();
        merge_page(
            &mut pages,
            page(&entry, &local_symbols, &slugs, source.flavor, opaque)?,
        );
    }
    Ok(pages)
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
            .qualified_name
            .rsplit_once("::")
            .map_or(entry.qualified_name.as_str(), |pair| pair.0)
    } else {
        &entry.qualified_name
    };
    if let Some(opaque) = api.get(name) {
        return (true, *opaque);
    }
    let suffix = format!("::{name}");
    let mut found = api.iter().filter(|(key, _)| key.ends_with(&suffix));
    match (found.next(), found.next()) {
        (Some((_, opaque)), None) => (true, *opaque),
        _ => (false, false),
    }
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
    if let Some(ty) = &entry.operation_type {
        return Ok(format!("{}: {ty}", entry.qualified_name.replace("::", ".")));
    }
    let name = entry.qualified_name.as_str();
    let exact = symbols
        .iter()
        .find(|symbol| symbol.get("name").and_then(Value::as_str) == Some(name));
    let symbol = match exact {
        Some(symbol) => Some(symbol),
        None => unique_suffix(symbols, name)?,
    };
    match symbol {
        Some(symbol) => Ok(render_symbol(symbol, name)),
        None if matches!(entry.kind, "Function" | "Extern" | "Value") => {
            Err(invalid(format!("no inferred signature for {name}")))
        }
        None => Ok(format!("{} {name}", entry.kind.to_lowercase())),
    }
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
