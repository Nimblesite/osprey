//! A module-first entry to the exported library. Implements [DOC-EXPORT-HTML].

use super::{describe, groups, items, layout, render as escape, Page, Site};
use std::fmt::Write as _;

pub(super) fn render(pages: &[Page], site: &Site) -> String {
    let mut body = hero(pages);
    for group in groups(pages) {
        let _ = write!(body, "{}", section(pages, group));
    }
    let overview = Page {
        slug: "index".into(),
        title: "Osprey documentation".into(),
        group: "Overview".into(),
        summary: String::new(),
        markdown: String::new(),
    };
    layout::document(&overview, site, &body)
}

fn hero(pages: &[Page]) -> String {
    let modules = pages
        .iter()
        .filter(|p| matches!(p.group.as_str(), "Module" | "Modules"))
        .count();
    let guides = pages.iter().filter(|p| p.group == "Guides").count();
    format!("<header class=\"hero\"><p class=\"eyebrow\">Explore the API</p>\
<h1>Osprey documentation</h1><p class=\"hero-description\">Explore the public API. Find a signature, follow an example, and build from here.</p>\
<div class=\"stats\">{}{}{}</div></header>", statistic(modules, "module"), statistic(pages.len(), "reference page"), statistic(guides, "guide"))
}

fn statistic(count: usize, label: &str) -> String {
    format!(
        "<span><strong>{count}</strong> {label}{}</span>",
        if count == 1 { "" } else { "s" }
    )
}

fn section(pages: &[Page], group: &str) -> String {
    let featured = matches!(group, "Module" | "Modules" | "Guides" | "Overview");
    let entries = items(pages, group, |out, page| card(out, page, pages));
    let count = pages.iter().filter(|page| page.group == group).count();
    let heading = format!(
        "<span>{}</span><span class=\"nav-count\">{count}</span>",
        escape::text(&super::group_label(group))
    );
    if featured {
        format!("<section class=\"catalog-section\"><h2 class=\"section-head\">{heading}</h2><div class=\"catalog\">{entries}</div></section>")
    } else {
        format!("<details class=\"reference-group\"><summary class=\"section-head\"><h2>{heading}</h2></summary><div class=\"catalog\">{entries}</div></details>")
    }
}

fn card(out: &mut String, page: &Page, pages: &[Page]) {
    let _ = write!(out,
        "<a class=\"page-card\" href=\"{}.html\"><span class=\"card-kind\">{}</span><span class=\"card-title\">{}</span><span class=\"card-summary\">{}</span><span class=\"card-arrow\" aria-hidden=\"true\">↗</span></a>",
        escape::attr(&page.slug), escape::text(&page.group),
        escape::text(&page.title), escape::text(&card_summary(page, pages)));
}

/// Undocumented modules still give the reader a useful inventory count.
fn card_summary(page: &Page, pages: &[Page]) -> String {
    if matches!(page.group.as_str(), "Module" | "Modules")
        && page.summary.is_empty()
        && escape::first_paragraph(&page.markdown).is_empty()
    {
        let prefix = format!("{}::", page.title);
        let count = pages
            .iter()
            .filter(|member| {
                member
                    .title
                    .strip_prefix(&prefix)
                    .is_some_and(|tail| !tail.contains("::"))
            })
            .count();
        return format!("{count} public member{}", if count == 1 { "" } else { "s" });
    }
    describe(page)
}
