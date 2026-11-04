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

/// The signature is the one code block the exporter writes itself, and it is
/// named rather than left as a bare fence. It opens expanded: a reference that
/// answers no question on arrival is not one.
#[test]
fn a_declaration_page_names_its_signature_and_lets_the_reader_fold_it_away() {
    let dir = fresh_dir("signature_panel");
    let pages = vec![
        page(
            "api/pay",
            "bank::Money::pay",
            "Function",
            "# bank::Money::pay\n\n```osprey\nfn pay(cents: int) -> int\n```\n\n\
             ## Examples\n\n```osprey\npay(100)\n```\n",
        ),
        page(
            "guides/start",
            "Getting started",
            "Guides",
            "```osprey\nx\n```\n",
        ),
    ];
    generate(&dir, &pages, "osprey", &[]).expect("site");
    let body = article(&read(&dir, "api/pay.html"));
    assert!(
        body.contains("<details class=\"signature-panel\" open><summary>Signature</summary><pre>"),
        "{body}"
    );
    // The example below the first heading is an author's code, not a signature.
    assert_eq!(body.matches("signature-panel").count(), 1, "{body}");
    let guide = article(&read(&dir, "guides/start.html"));
    assert!(
        !guide.contains("signature-panel"),
        "a guide's first fence is not a signature: {guide}"
    );
}

/// A reader who arrives at a declaration from search has no way back to the
/// module that owns it unless the page says so.
#[test]
fn a_declaration_page_links_every_scope_that_encloses_it() {
    let dir = fresh_dir("crumb_ancestors");
    let pages = vec![
        page("api/money", "bank::Money", "Module", "# bank::Money\n"),
        page(
            "api/pay",
            "bank::Money::pay",
            "Function",
            "# bank::Money::pay\n",
        ),
    ];
    generate(&dir, &pages, "osprey", &[]).expect("site");
    let body = article(&read(&dir, "api/pay.html"));
    assert!(
        body.contains("<a href=\"../api/money.html\">Money</a>"),
        "the owning module is not linked: {body}"
    );
    // `bank` has no page in this export, so it is skipped rather than linked
    // into a 404.
    assert!(!body.contains(">bank</a>"), "{body}");
    assert!(
        body.contains("<span class=\"kind\">Function</span>"),
        "{body}"
    );
}
