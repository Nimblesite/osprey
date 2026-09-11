//! Shared semantic shell for every documentation template. Implements [DOC-EXPORT-HTML].

use super::{describe, navigation, render, root_prefix, Page, Site, SEARCH_INDEX, THEME_CSS};

/// The page shell around an already-rendered `body`. One shell serves the
/// Markdown pages and the landing page, so the two can never drift apart.
pub(super) fn document(page: &Page, site: &Site, body: &str) -> String {
    let root = root_prefix(&page.slug);
    format!(
        "<!doctype html>\n<html lang=\"en\">\n{head}\n<body>\n\
<a class=\"skip\" href=\"#content\">Skip to content</a>\n\
<div class=\"shell\">\n{sidebar}\n\
<main id=\"content\" class=\"{class}\">\n<div class=\"article\">\n<p class=\"crumb\"><a href=\"{root}index.html\">Documentation</a><span aria-hidden=\"true\">/</span><span class=\"kind\">{group}</span></p>\n{heading}{summary}\n{body}\n</div>\n<nav class=\"toc\" aria-label=\"On this page\" hidden></nav>\n</main>\n\
</div>\n\
<script src=\"{root}{SEARCH_INDEX}\"></script>\n\
<script>{script}</script>\n\
</body>\n</html>\n",
        head = head(page, site, &root),
        sidebar = navigation::sidebar(site, &page.slug, &root),
        heading = if page.slug == "index" { String::new() } else { heading(page, body) },
        summary = summary(page),
        group = render::text(&page.group),
        script = scripts(&root),
        class = if page.slug == "index" { "overview" } else { "reference" },
    )
}

/// Reuse the website's language grammar; all highlighting still runs offline.
fn scripts(root: &str) -> String {
    let grammar = include_str!("../../../../../website/src/js/osprey-grammar.mjs").replacen(
        "export const",
        "const",
        1,
    );
    format!(
        "{grammar}\n{}\n{}",
        include_str!("highlight.js"),
        include_str!("behavior.js").replace("{root}", root)
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
