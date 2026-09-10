//! The `--docs-format html` writer: a self-contained static site.
//! Implements [DOC-EXPORT-HTML].
//!
//! Everything the reader needs is written into the output directory — theme
//! CSS, any user stylesheets, and a JSON search index — because the export has
//! to work when served from a plain static server with no build pipeline and
//! no network. Nothing is loaded from a CDN.
//!
//! Cleanup is manifest-scoped exactly as the Markdown writer's is: the exporter
//! removes only files it previously generated, so an unrelated file sharing the
//! output directory survives.

mod render;
#[cfg(test)]
mod tests;
mod theme;

use super::model::{Page, Stylesheet};
use std::collections::BTreeSet;
use std::io;
use std::path::{Component, Path};

const MANIFEST: &str = ".osprey-html-pages.json";
const THEME_CSS: &str = "assets/theme.css";

/// Write `pages` as an HTML site under `directory`, styled by `theme` and then
/// by each stylesheet in `css` — user CSS is linked AFTER the theme so it wins.
pub(super) fn generate(
    directory: &Path,
    pages: &[Page],
    theme: &str,
    css: &[Stylesheet],
) -> io::Result<()> {
    let mut written: BTreeSet<String> = BTreeSet::new();
    let links = stylesheet_links(css);
    let nav = navigation(pages);

    let _ = written.insert(THEME_CSS.to_owned());
    write_file(directory, THEME_CSS, &theme::css(theme))?;
    for sheet in css {
        let path = format!("assets/{}", sheet.name);
        validate_path(&path)?;
        let _ = written.insert(path.clone());
        write_file(directory, &path, &sheet.css)?;
    }
    let index = "assets/search-index.json".to_owned();
    let _ = written.insert(index.clone());
    write_file(directory, &index, &search_index(pages))?;

    for page in pages {
        let file = format!("{}.html", page.slug);
        validate_path(&file)?;
        let _ = written.insert(file.clone());
        write_file(directory, &file, &document(page, &nav, &links))?;
    }
    // A static server handed this directory serves `index.html`; without one the
    // reader gets a file listing or a 404. It also gives the sidebar's brand a
    // link that always resolves, whatever sections this export happens to have.
    let _ = written.insert("index.html".to_owned());
    write_file(directory, "index.html", &landing(pages, &nav, &links))?;
    prune(directory, &written)?;
    let manifest = serde_json::to_string_pretty(&written).map_err(io::Error::other)?;
    std::fs::write(directory.join(MANIFEST), manifest)
}

/// Every generated path is relative, traversal-free and lands in a tree the
/// exporter owns. A page slug reaches this from user input, so it is checked
/// rather than trusted.
fn validate_path(path: &str) -> io::Result<()> {
    let normal = Path::new(path)
        .components()
        .all(|part| matches!(part, Component::Normal(_)));
    let owned = path == "index.html"
        || path
            .split('/')
            .next()
            .is_some_and(|root| matches!(root, "functions" | "api" | "guides" | "assets"));
    if normal && owned && !path.contains("..") {
        return Ok(());
    }
    Err(io::Error::other(format!(
        "invalid generated documentation path: {path}"
    )))
}

fn write_file(directory: &Path, relative: &str, body: &str) -> io::Result<()> {
    let path = directory.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    contained(directory, &path)?;
    std::fs::write(path, body)
}

/// Refuse to write through a symlink. `validate_path` proves the *slug* has no
/// traversal in it, but a symlink already sitting in the output directory — as
/// the target file or as one of its parents — would still send the write
/// somewhere else entirely. The exporter overwrites what it finds, so it must
/// know it is writing inside the directory it was given.
fn contained(directory: &Path, path: &Path) -> io::Result<()> {
    if std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err(io::Error::other(format!(
            "refusing to write through a symlink: {}",
            path.display()
        )));
    }
    let (Ok(root), Some(parent)) = (directory.canonicalize(), path.parent()) else {
        return Ok(());
    };
    match parent.canonicalize() {
        Ok(resolved) if !resolved.starts_with(&root) => Err(io::Error::other(format!(
            "documentation path escapes the output directory: {}",
            path.display()
        ))),
        _ => Ok(()),
    }
}

/// Delete files this exporter generated last time and did not generate now.
fn prune(directory: &Path, written: &BTreeSet<String>) -> io::Result<()> {
    let previous: BTreeSet<String> = match std::fs::read_to_string(directory.join(MANIFEST)) {
        Ok(text) => serde_json::from_str(&text).map_err(io::Error::other)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => BTreeSet::new(),
        Err(error) => return Err(error),
    };
    for obsolete in previous.difference(written) {
        validate_path(obsolete)?;
        let path = directory.join(obsolete);
        if path.is_file() {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}

/// `<link>` elements for the user stylesheets, in the order given.
fn stylesheet_links(css: &[Stylesheet]) -> String {
    css.iter()
        .map(|sheet| {
            format!(
                "<link rel=\"stylesheet\" href=\"{{root}}assets/{}\">",
                render::attr(&sheet.name)
            )
        })
        .collect()
}

/// The sidebar, grouped in the order groups first appear.
fn navigation(pages: &[Page]) -> String {
    let mut groups: Vec<&str> = Vec::new();
    for page in pages {
        if !groups.contains(&page.group.as_str()) {
            groups.push(&page.group);
        }
    }
    groups
        .iter()
        .map(|group| {
            let items: String = pages
                .iter()
                .filter(|page| page.group == *group)
                .map(|page| {
                    format!(
                        "<li><a href=\"{{root}}{}.html\" data-slug=\"{}\">{}</a></li>",
                        render::attr(&page.slug),
                        render::attr(&page.slug),
                        render::text(&page.title)
                    )
                })
                .collect();
            format!(
                "<p class=\"group\">{}</p><ul class=\"nav\">{items}</ul>",
                render::text(group)
            )
        })
        .collect()
}

/// The search index: one record per page, with the body flattened to text.
fn search_index(pages: &[Page]) -> String {
    let records: Vec<String> = pages
        .iter()
        .map(|page| {
            let summary = describe(page);
            format!(
                "{{\"slug\":\"{}\",\"title\":\"{}\",\"group\":\"{}\",\"summary\":\"{}\",\"body\":\"{}\"}}",
                render::json(&page.slug),
                render::json(&page.title),
                render::json(&page.group),
                render::json(&summary),
                render::json(&haystack(&page.markdown))
            )
        })
        .collect();
    format!("[{}]", records.join(","))
}

/// A page body reduced to searchable words, capped so the index stays small
/// enough to fetch on a phone.
fn haystack(markdown: &str) -> String {
    const LIMIT: usize = 2000;
    let text: String = render::without_front_matter(markdown)
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    let mut words = String::new();
    for word in text.split_whitespace() {
        if words.len().saturating_add(word.len()) >= LIMIT {
            break;
        }
        if !words.is_empty() {
            words.push(' ');
        }
        words.push_str(&word.to_lowercase());
    }
    words
}

/// How many `../` segments reach the output root from a page's own directory.
fn root_prefix(slug: &str) -> String {
    "../".repeat(slug.matches('/').count())
}

fn document(page: &Page, nav: &str, links: &str) -> String {
    let body = render::markdown(render::without_front_matter(&page.markdown));
    document_with_body(page, nav, links, &body)
}

/// The page shell around an already-rendered `body`. One shell serves the
/// Markdown pages and the landing page, so the two can never drift apart.
fn document_with_body(page: &Page, nav: &str, links: &str, body: &str) -> String {
    let root = root_prefix(&page.slug);
    let summary = if page.summary.is_empty() {
        String::new()
    } else {
        format!(
            "<p class=\"summary\">{}</p>",
            render::text(&render::plain(&page.summary))
        )
    };
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n\
<meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n\
<title>{document_title}</title>\n\
<meta name=\"description\" content=\"{description}\">\n\
<link rel=\"stylesheet\" href=\"{root}{THEME_CSS}\">\n{links}\n\
</head>\n<body>\n\
<a class=\"skip\" href=\"#content\">Skip to content</a>\n\
<div class=\"shell\">\n\
<aside class=\"side\">\n\
<a class=\"brand\" href=\"{root}index.html\">Osprey documentation</a>\n\
<label class=\"skip\" for=\"q\">Search documentation</label>\n\
<input id=\"q\" class=\"search\" type=\"search\" placeholder=\"Search…\" autocomplete=\"off\">\n\
<div id=\"results\" role=\"region\" aria-live=\"polite\"></div>\n\
<div id=\"tree\">{nav}</div>\n\
</aside>\n\
<main id=\"content\">\n<p class=\"crumb\">{group}</p>\n{heading}{summary}\n{body}\n</main>\n\
</div>\n\
<script>{script}</script>\n\
</body>\n</html>\n",
        document_title = render::text(&document_title(page)),
        heading = heading(page, &body),
        description = render::attr(&describe(page)),
        group = render::text(&page.group),
        links = links.replace("{root}", &root),
        nav = mark_current(nav, &page.slug).replace("{root}", &root),
        script = search_script(&root),
    )
}

/// The browser-tab title. The site name is appended only when the page is not
/// already named after it, so the landing page does not read "Osprey
/// documentation — Osprey documentation".
fn document_title(page: &Page) -> String {
    const SITE: &str = "Osprey documentation";
    if page.title == SITE {
        SITE.to_owned()
    } else {
        format!("{} — {SITE}", page.title)
    }
}

/// The page's `<h1>`, supplied only when the rendered body has none.
///
/// The built-in reference pages carry their name in front matter and open
/// straight into `**Signature:**`, so rendering them alone produced a page with
/// no top-level heading at all: bad document structure, and a screen-reader
/// user landing in `main` with nothing telling them where they are.
fn heading(page: &Page, body: &str) -> String {
    if body.contains("<h1") {
        return String::new();
    }
    format!("<h1>{}</h1>\n", render::text(&page.title))
}

/// A page's description: its own summary, else its opening line. An empty
/// `<meta name="description">` is worse than none — it tells a search engine
/// the page has no description rather than letting it read the text.
fn describe(page: &Page) -> String {
    if page.summary.is_empty() {
        render::first_paragraph(&page.markdown)
    } else {
        render::plain(&page.summary)
    }
}

/// The landing page: every group, and every page within it.
fn landing(pages: &[Page], nav: &str, links: &str) -> String {
    let mut groups: Vec<&str> = Vec::new();
    for page in pages {
        if !groups.contains(&page.group.as_str()) {
            groups.push(&page.group);
        }
    }
    let body: String = groups
        .iter()
        .map(|group| {
            let items: String = pages
                .iter()
                .filter(|page| page.group == *group)
                .map(|page| {
                    format!(
                        "<li><a href=\"{}.html\">{}</a> {}</li>",
                        render::attr(&page.slug),
                        render::text(&page.title),
                        render::text(&describe(page))
                    )
                })
                .collect();
            format!("<h2>{}</h2><ul>{items}</ul>", render::text(group))
        })
        .collect();
    let overview = Page {
        slug: "index".into(),
        title: "Osprey documentation".into(),
        group: "Overview".into(),
        summary: "Every module, declaration and guide in this export.".into(),
        markdown: String::new(),
    };
    document_with_body(&overview, nav, links, &body)
}

/// Flag the sidebar entry for the page being rendered.
fn mark_current(nav: &str, slug: &str) -> String {
    let needle = format!("data-slug=\"{}\"", render::attr(slug));
    let marked = format!("{needle} aria-current=\"page\"");
    nav.replace(&needle, &marked)
}

/// The search behaviour. Kept small and dependency-free: it fetches the index
/// once, filters on every keystroke, and says so plainly when nothing matches
/// rather than leaving an empty box.
fn search_script(root: &str) -> String {
    format!(
        "(function(){{\
var q=document.getElementById('q'),out=document.getElementById('results'),\
tree=document.getElementById('tree'),data=null;\
function esc(s){{return s.replace(/[&<>\"]/g,function(c){{\
return {{'&':'&amp;','<':'&lt;','>':'&gt;','\"':'&quot;'}}[c];}});}}\
function show(items,term){{\
if(!term){{out.innerHTML='';tree.hidden=false;return;}}\
tree.hidden=true;\
if(!items.length){{out.innerHTML='<p class=\"note\">No page matches '+esc(term)+'.</p>';return;}}\
out.innerHTML='<ul class=\"hits\">'+items.slice(0,25).map(function(p){{\
return '<li><a href=\"{root}'+esc(p.slug)+'.html\">'+esc(p.title)+'</a><p>'+esc(p.summary||p.group)+'</p></li>';}}).join('')+'</ul>';}}\
function run(){{var term=q.value.trim();if(!data){{return;}}var low=term.toLowerCase();\
show(data.filter(function(p){{return (p.title+' '+p.group+' '+p.summary+' '+p.body).toLowerCase().indexOf(low)>=0;}}),term);}}\
q.addEventListener('input',run);\
q.addEventListener('keydown',function(e){{if(e.key==='Escape'){{q.value='';run();}}}});\
fetch('{root}assets/search-index.json').then(function(r){{return r.json();}})\
.then(function(j){{data=j;run();}}).catch(function(){{\
out.innerHTML='<p class=\"note\">Search index unavailable.</p>';}});\
}})();"
    )
}
