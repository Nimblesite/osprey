//! Page-level documentation semantics. Implements [DOC-EXPORT-HTML].

use super::{article, fresh_dir, generate, page, read};

#[test]
fn a_summary_already_in_markdown_is_shown_once_inside_the_article() {
    let dir = fresh_dir("summary_once");
    let mut documented = page(
        "api/example",
        "example",
        "Function",
        "# example\n\nThe summary.\n",
    );
    documented.summary = "The summary.".into();
    generate(&dir, &[documented], "osprey", &[]).expect("site");
    let body = article(&read(&dir, "api/example.html"));
    assert_eq!(body.matches("The summary.").count(), 1, "{body}");
}

#[test]
fn the_landing_page_promotes_modules_and_guides_before_the_full_reference() {
    let dir = fresh_dir("landing_hierarchy");
    let pages = vec![
        page(
            "functions/byteat",
            "byteAt",
            "Built-in functions",
            "Read a byte.",
        ),
        page("api/money", "bank::Money", "Module", "Money operations."),
        page("guides/start", "Getting started", "Guides", "Start here."),
        page("api/pay", "bank::Money::pay", "Function", "Transfer money."),
    ];
    generate(&dir, &pages, "osprey", &[]).expect("site");
    let body = article(&read(&dir, "index.html"));
    assert!(read(&dir, "index.html").contains("class=\"overview-link\" aria-current=\"page\""));
    assert!(
        body.contains("class=\"hero\""),
        "no landing hierarchy: {body}"
    );
    assert!(
        body.contains("class=\"page-card\""),
        "no module cards: {body}"
    );
    assert!(body.contains("<h2><span>Built-in functions</span>"));
    assert!(body.contains("<strong>1</strong> guide</span>"));
    let module = body.find("bank::Money</").expect("module card");
    let builtin = body.find("byteAt</").expect("builtin is still reachable");
    assert!(module < builtin, "project modules must precede builtins");
    for path in [
        "api/money.html",
        "api/pay.html",
        "guides/start.html",
        "functions/byteat.html",
    ] {
        assert!(body.contains(path), "landing lost {path}");
    }
}

#[test]
fn navigation_groups_are_disclosures_and_every_page_remains_reachable() {
    let dir = fresh_dir("nav_disclosures");
    let pages = vec![
        page("api/m", "M", "Module", "# M\n\n## Examples\n"),
        page("functions/read", "read", "Built-in functions", "Read text."),
    ];
    generate(&dir, &pages, "paper", &[]).expect("site");
    let html = read(&dir, "api/m.html");
    assert_eq!(html.matches("class=\"nav-group\" open").count(), 2);
    assert!(html.contains("data-slug=\"api/m\" aria-current=\"page\""));
    assert!(html.contains("href=\"../functions/read.html\""));
    assert!(html.contains("class=\"article\""));
    assert!(html.contains("aria-label=\"On this page\""));
    assert!(html.contains("aria-label=\"Search results\""));
    assert!(html.contains("aria-keyshortcuts=\"/\""));
}
