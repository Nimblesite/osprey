//! The `--docs-format html` writer: a self-contained static site.
//! Implements [DOC-EXPORT-HTML].
//!
//! Everything the reader needs is written into the output directory — theme
//! CSS, any user stylesheets, and the search index — because the export has to
//! work when served from a plain static server with no build pipeline and no
//! network, and when opened straight off the filesystem. Nothing is loaded from
//! a CDN.
//!
//! Validation, writing and manifest-scoped cleanup are the shared publisher's
//! ([`super::output`]), the same one the Markdown writer uses: the exporter
//! removes only files it previously generated, so an unrelated file sharing the
//! output directory survives.

mod anchors;
mod render;
#[cfg(test)]
mod tests;
mod theme;

use super::model::{Page, Stylesheet};
use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io;
use std::path::Path;

const MANIFEST: &str = ".osprey-html-pages.json";
const THEME_CSS: &str = "assets/theme.css";
const SEARCH_INDEX: &str = "assets/search-index.js";
const LANDING: &str = "index.html";

/// Everything a page needs that is identical on every page.
struct Site {
    nav: String,
    links: String,
    symbols: Symbols,
}

/// Where documentation symbol links point ([DOC-LINK]).
struct Symbols {
    /// Fully qualified name to slug.
    exact: BTreeMap<String, String>,
    /// Every shorter spelling to slug, absent where two declarations claim it.
    short: BTreeMap<String, String>,
}

impl Symbols {
    /// The page a symbol link names, resolved from the innermost enclosing
    /// scope outwards before any global match is considered.
    ///
    /// `[helper]` written inside `shop::A::read` means `shop::A::helper` when
    /// that exists, even where an unrelated `shop::B::helper` also does — the
    /// nearest owner wins, the way name resolution works in the language
    /// itself. Only when no enclosing scope owns the name does a global match
    /// apply, and then only if exactly one declaration answers to it.
    fn target(&self, reference: &str, scope: &[&str]) -> Option<&String> {
        let wanted = reference.replace('.', "::");
        (0..=scope.len())
            .rev()
            .filter_map(|depth| scope.get(..depth))
            .find_map(|prefix| self.exact.get(&qualify(prefix, &wanted)))
            .or_else(|| self.short.get(reference))
    }
}

fn qualify(prefix: &[&str], name: &str) -> String {
    if prefix.is_empty() {
        return name.to_owned();
    }
    format!("{}::{name}", prefix.join("::"))
}

/// Write `pages` as an HTML site under `directory`, styled by `theme` and then
/// by each stylesheet in `css` — user CSS is linked AFTER the theme so it wins.
pub(super) fn generate(
    directory: &Path,
    pages: &[Page],
    theme: &str,
    css: &[Stylesheet],
) -> io::Result<()> {
    let site = Site {
        nav: navigation(pages),
        links: stylesheet_links(css),
        symbols: symbol_targets(pages),
    };
    super::output::publish(directory, &files(pages, theme, css, &site), MANIFEST)
}

/// Every file the site is made of, as the shared publisher takes them.
fn files(pages: &[Page], theme: &str, css: &[Stylesheet], site: &Site) -> Vec<(String, Vec<u8>)> {
    // A static server handed this directory serves `index.html`; without one
    // the reader gets a file listing or a 404. It also gives the sidebar's
    // brand a link that always resolves, whatever sections this export has.
    let mut files = vec![
        (THEME_CSS.to_owned(), theme::css(theme).into_bytes()),
        (SEARCH_INDEX.to_owned(), search_index(pages).into_bytes()),
        (LANDING.to_owned(), landing(pages, site).into_bytes()),
    ];
    for sheet in css {
        files.push((
            format!("assets/{}", sheet.name),
            sheet.css.clone().into_bytes(),
        ));
    }
    for page in pages {
        files.push((
            format!("{}.html", page.slug),
            document(page, site).into_bytes(),
        ));
    }
    files
}

/// Where each documentation symbol link points ([DOC-LINK]).
///
/// Guides and index listings are not declarations and never claim a name.
fn symbol_targets(pages: &[Page]) -> Symbols {
    let mut exact: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut short: BTreeMap<String, Option<String>> = BTreeMap::new();
    let declarations = pages.iter().filter(|page| {
        (page.slug.starts_with("api/") || page.slug.starts_with("functions/"))
            && !page.slug.ends_with("/index")
    });
    for page in declarations {
        claim(&mut exact, page.title.clone(), &page.slug);
        for name in names_for(&page.title) {
            claim(&mut short, name, &page.slug);
        }
    }
    Symbols {
        exact: settled(exact),
        short: settled(short),
    }
}

/// Drop every name that stayed contested.
fn settled(claims: BTreeMap<String, Option<String>>) -> BTreeMap<String, String> {
    claims
        .into_iter()
        .filter_map(|(name, slug)| slug.map(|slug| (name, slug)))
        .collect()
}

/// Every spelling a symbol link may use for one declaration.
///
/// A page's title is the FULLY qualified name (`shop::Money::format`), but a
/// doc comment names a sibling the way a reader would — `[Money.format]`, or
/// `[format]` where that is unambiguous — and never repeats the namespace it
/// is already inside. Claiming only the full name resolved almost nothing on a
/// real project. Each suffix is offered in both spellings [DOC-LINK] admits.
fn names_for(qualified: &str) -> Vec<String> {
    let segments: Vec<&str> = qualified.split("::").collect();
    (0..segments.len())
        .filter_map(|start| segments.get(start..))
        .flat_map(|tail| [tail.join("::"), tail.join(".")])
        .collect()
}

/// Record that `slug` answers to `name`.
///
/// A name two different declarations both answer to is poisoned rather than
/// resolved: sending a reader to whichever page happened to be collected first
/// is worse than leaving the link as the text the author wrote.
fn claim(claims: &mut BTreeMap<String, Option<String>>, name: String, slug: &str) {
    match claims.entry(name) {
        Entry::Vacant(slot) => {
            let _ = slot.insert(Some(slug.to_owned()));
        }
        Entry::Occupied(mut slot) => {
            if slot.get().as_deref() != Some(slug) {
                let _ = slot.insert(None);
            }
        }
    }
}

/// `<link>` elements for the user stylesheets, in the order given.
fn stylesheet_links(css: &[Stylesheet]) -> String {
    css.iter().fold(String::new(), |mut out, sheet| {
        let _ = write!(
            out,
            "<link rel=\"stylesheet\" href=\"{{root}}assets/{}\">",
            render::attr(&sheet.name)
        );
        out
    })
}

/// The distinct page groups, in the order they first appear. The sidebar and
/// the landing page must agree on that order, so they read it from here.
fn groups(pages: &[Page]) -> Vec<&str> {
    let mut groups: Vec<&str> = Vec::new();
    for page in pages {
        if !groups.contains(&page.group.as_str()) {
            groups.push(&page.group);
        }
    }
    groups
}

/// Every page in one group, rendered by `item` and concatenated.
fn items(pages: &[Page], group: &str, item: impl Fn(&mut String, &Page)) -> String {
    pages
        .iter()
        .filter(|page| page.group == group)
        .fold(String::new(), |mut out, page| {
            item(&mut out, page);
            out
        })
}

/// The sidebar, grouped in the order groups first appear.
fn navigation(pages: &[Page]) -> String {
    groups(pages).iter().fold(String::new(), |mut out, group| {
        let entries = items(pages, group, |out, page| {
            let _ = write!(
                out,
                "<li><a href=\"{{root}}{}.html\" data-slug=\"{}\">{}</a></li>",
                render::attr(&page.slug),
                render::attr(&page.slug),
                render::text(&page.title)
            );
        });
        let _ = write!(
            out,
            "<p class=\"group\">{}</p><ul class=\"nav\">{entries}</ul>",
            render::text(group)
        );
        out
    })
}

/// The search index: one record per page, with the body flattened to text.
///
/// It is written as a JavaScript assignment rather than as JSON the page
/// fetches. A reader who opens the export straight off disk has the opaque
/// origin `null`, where `fetch()` of a sibling file is a CORS failure with no
/// visible cause — search would simply never match anything, and nothing on
/// the page would say why. A `<script src>` has no such restriction.
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
    format!("window.OSPREY_SEARCH=[{}];\n", records.join(","))
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

fn document(page: &Page, site: &Site) -> String {
    let root = root_prefix(&page.slug);
    let scope = owner_of(&page.title);
    let resolve = |reference: &str| {
        site.symbols
            .target(reference, &scope)
            .map(|slug| format!("{root}{slug}.html"))
    };
    let body = render::markdown(render::without_front_matter(&page.markdown), &resolve);
    document_with_body(page, site, &body)
}

/// The scopes enclosing a declaration, outermost first: `shop::Money::parse`
/// sits in `shop::Money`, which sits in `shop`.
fn owner_of(qualified: &str) -> Vec<&str> {
    let mut segments: Vec<&str> = qualified.split("::").collect();
    let _ = segments.pop();
    segments
}

/// The page shell around an already-rendered `body`. One shell serves the
/// Markdown pages and the landing page, so the two can never drift apart.
fn document_with_body(page: &Page, site: &Site, body: &str) -> String {
    let root = root_prefix(&page.slug);
    format!(
        "<!doctype html>\n<html lang=\"en\">\n{head}\n<body>\n\
<a class=\"skip\" href=\"#content\">Skip to content</a>\n\
<div class=\"shell\">\n{sidebar}\n\
<main id=\"content\">\n<p class=\"crumb\">{group}</p>\n{heading}{summary}\n{body}\n</main>\n\
</div>\n\
<script src=\"{root}{SEARCH_INDEX}\"></script>\n\
<script>{script}</script>\n\
</body>\n</html>\n",
        head = head(page, site, &root),
        sidebar = sidebar(site, &page.slug, &root),
        heading = heading(page, body),
        summary = summary(page),
        group = render::text(&page.group),
        script = search_script(&root),
    )
}

/// The document head: title, description, the theme, then the user stylesheets
/// in the order they were given so the later ones win.
fn head(page: &Page, site: &Site, root: &str) -> String {
    format!(
        "<head>\n\
<meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n\
<title>{title}</title>\n\
<meta name=\"description\" content=\"{description}\">\n\
<link rel=\"stylesheet\" href=\"{root}{THEME_CSS}\">\n{links}\n\
</head>",
        title = render::text(&document_title(page)),
        description = render::attr(&describe(page)),
        links = site.links.replace("{root}", root),
    )
}

/// The sidebar: brand, search, and the page tree.
///
/// The tree is a disclosure. It precedes the article in source order, which is
/// what a screen reader needs and what would otherwise make a phone reader
/// scroll past the entire reference to reach the page they opened. `open` is in
/// the markup, so with no script the links are all still there.
fn sidebar(site: &Site, slug: &str, root: &str) -> String {
    format!(
        "<aside class=\"side\">\n\
<a class=\"brand\" href=\"{root}index.html\">Osprey documentation</a>\n\
<label class=\"skip\" for=\"q\">Search documentation</label>\n\
<input id=\"q\" class=\"search\" type=\"search\" placeholder=\"Search…\" autocomplete=\"off\">\n\
<div id=\"results\" role=\"region\" aria-live=\"polite\"></div>\n\
<details id=\"menu\" open>\n\
<summary>Browse documentation</summary>\n\
<div id=\"tree\">{nav}</div>\n\
</details>\n\
</aside>",
        nav = mark_current(&site.nav, slug).replace("{root}", root),
    )
}

/// The page's summary line, in the plain text the contexts around it can show.
fn summary(page: &Page) -> String {
    if page.summary.is_empty() || page.markdown.contains(&page.summary) {
        return String::new();
    }
    format!(
        "<p class=\"summary\">{}</p>",
        render::text(&render::plain(&page.summary))
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
fn landing(pages: &[Page], site: &Site) -> String {
    let body = groups(pages).iter().fold(String::new(), |mut out, group| {
        let entries = items(pages, group, |out, page| {
            let _ = write!(
                out,
                "<li><a href=\"{}.html\">{}</a> {}</li>",
                render::attr(&page.slug),
                render::text(&page.title),
                render::text(&describe(page))
            );
        });
        let _ = write!(out, "<h2>{}</h2><ul>{entries}</ul>", render::text(group));
        out
    });
    let overview = Page {
        slug: LANDING.trim_end_matches(".html").into(),
        title: "Osprey documentation".into(),
        group: "Overview".into(),
        summary: "Every module, declaration and guide in this export.".into(),
        markdown: String::new(),
    };
    document_with_body(&overview, site, &body)
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
menu=document.getElementById('menu'),narrow=window.matchMedia('(max-width:860px)'),\
data=window.OSPREY_SEARCH;\
function esc(s){{return String(s).replace(/[&<>\"]/g,function(c){{\
return {{'&':'&amp;','<':'&lt;','>':'&gt;','\"':'&quot;'}}[c];}});}}\
function show(items,term){{\
if(!term){{out.innerHTML='';menu.hidden=false;return;}}\
menu.hidden=true;\
if(!items.length){{out.innerHTML='<p class=\"note\">No page matches '+esc(term)+'.</p>';return;}}\
out.innerHTML='<ul class=\"hits\">'+items.slice(0,25).map(function(p){{\
return '<li><a href=\"{root}'+esc(p.slug)+'.html\">'+esc(p.title)+'</a><p>'+esc(p.summary||p.group)+'</p></li>';}}).join('')+'</ul>';}}\
function run(){{var term=q.value.trim(),low=term.toLowerCase();\
show(data.filter(function(p){{return (p.title+' '+p.group+' '+p.summary+' '+p.body).toLowerCase().indexOf(low)>=0;}}),term);}}\
if(!data){{out.innerHTML='<p class=\"note\">Search index unavailable.</p>';return;}}\
function fit(){{menu.open=!narrow.matches;}}\
fit();narrow.addEventListener('change',fit);\
q.addEventListener('input',run);\
q.addEventListener('keydown',function(e){{if(e.key==='Escape'){{q.value='';run();}}}});\
run();\
}})();"
    )
}
