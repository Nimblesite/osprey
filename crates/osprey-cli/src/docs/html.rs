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
mod landing;
mod layout;
mod navigation;
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
        nav: navigation::groups(pages),
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
        (
            LANDING.to_owned(),
            landing::render(pages, site).into_bytes(),
        ),
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
    groups.sort_by_key(|group| group_rank(group));
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
    layout::document(page, site, &body)
}

/// The scopes enclosing a declaration, outermost first: `shop::Money::parse`
/// sits in `shop::Money`, which sits in `shop`.
fn owner_of(qualified: &str) -> Vec<&str> {
    let mut segments: Vec<&str> = qualified.split("::").collect();
    let _ = segments.pop();
    segments
}

/// A page's description: its own summary, else its opening line. An empty
/// `<meta name="description">` is worse than none — it tells a search engine
/// the page has no description rather than letting it read the text.
fn describe(page: &Page) -> String {
    let description = if page.summary.is_empty() {
        render::first_paragraph(&page.markdown)
    } else {
        render::plain(&page.summary)
    };
    if description.is_empty() {
        format!("{} ({})", page.title, page.group.to_lowercase())
    } else {
        description
    }
}

/// Category labels name a collection rather than one declaration.
fn group_label(group: &str) -> String {
    match group {
        "Extern" => "External functions".into(),
        "Overview" => "Reference".into(),
        plural if plural.ends_with('s') => plural.into(),
        singular => format!("{singular}s"),
    }
}

/// Put the reader's own modules and guides ahead of individual declarations.
fn group_rank(group: &str) -> u8 {
    match group {
        "Module" | "Modules" => 0,
        "Guides" => 1,
        "Namespace" => 2,
        "Overview" => 3,
        "Built-in functions" => 5,
        _ => 4,
    }
}
