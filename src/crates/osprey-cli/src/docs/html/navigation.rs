//! Compact, keyboard-operable reference navigation. Implements [DOC-EXPORT-HTML].

use super::{items, render, Page, Site};
use std::fmt::Write as _;

/// All links remain available without JavaScript; enhancement folds inactive groups.
pub(super) fn groups(pages: &[Page]) -> String {
    super::groups(pages).iter().fold(String::new(), |mut out, group| {
        let entries = items(pages, group, entry);
        let count = pages.iter().filter(|page| page.group == *group).count();
        let _ = write!(out,
            "<details class=\"nav-group\" open><summary>{}<span class=\"nav-count\">{count}</span></summary><ul class=\"nav\">{entries}</ul></details>",
            render::text(&super::group_label(group)));
        out
    })
}

fn entry(out: &mut String, page: &Page) {
    let _ = write!(
        out,
        "<li><a href=\"{{root}}{}.html\" data-slug=\"{}\" title=\"{}\">{}</a></li>",
        render::attr(&page.slug),
        render::attr(&page.slug),
        render::attr(&page.title),
        render::text(&page.title)
    );
}

/// Brand and search stay in view while the reference tree scrolls independently.
pub(super) fn sidebar(site: &Site, slug: &str, root: &str) -> String {
    format!(
        "<aside class=\"side\">\n\
<a class=\"brand\" href=\"{root}index.html\"><span class=\"brand-mark\" aria-hidden=\"true\">O</span><span><span class=\"brand-name\">Osprey</span><span class=\"brand-caption\">Documentation</span></span></a>\n\
<label class=\"skip\" for=\"q\">Search documentation</label>\n\
<div class=\"search-wrap\"><input id=\"q\" class=\"search\" type=\"search\" placeholder=\"Search documentation…\" autocomplete=\"off\" aria-keyshortcuts=\"/\"><kbd aria-hidden=\"true\">/</kbd></div>\n\
<div id=\"results\" role=\"region\" aria-label=\"Search results\" aria-live=\"polite\"></div>\n\
<details id=\"menu\" open><summary>Browse documentation</summary>\n\
<div id=\"tree\"><a class=\"overview-link\"{current} href=\"{root}index.html\">Overview</a>{nav}</div></details>\n\
<div class=\"side-footer\">Osprey API reference<span>Available offline</span></div>\n\
</aside>",
        nav = mark_current(&site.nav, slug).replace("{root}", root),
        current = if slug == "index" { " aria-current=\"page\"" } else { "" },
    )
}

fn mark_current(nav: &str, slug: &str) -> String {
    let needle = format!("data-slug=\"{}\"", render::attr(slug));
    nav.replace(&needle, &format!("{needle} aria-current=\"page\""))
}
