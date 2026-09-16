//! Coverage for the HTML exporter: what a generated site contains, where it is
//! allowed to write, what it prunes, and how it navigates. Markdown rendering
//! has its own module.

mod markdown;
mod page_semantics;

use super::{generate, render, theme, Page, Stylesheet};
use std::path::{Path, PathBuf};

/// Render a Markdown fragment in which no symbol link resolves.
fn md(source: &str) -> String {
    render::markdown(source, &|_| None)
}

/// The rendered article, without the sidebar — which links every page in the
/// export and so would answer "is this linked?" with yes whatever the body says.
fn article(html: &str) -> String {
    html.split_once("<main")
        .and_then(|(_, rest)| rest.split_once("</main>"))
        .map_or_else(String::new, |(body, _)| body.to_owned())
}

/// The search index as JSON, read back out of the script a page loads.
fn search_index(dir: &Path) -> serde_json::Value {
    let script = read(dir, "assets/search-index.js");
    let json = script
        .strip_prefix("window.OSPREY_SEARCH=")
        .and_then(|rest| rest.trim_end().strip_suffix(';'))
        .unwrap_or_default();
    serde_json::from_str(json).expect("search index is valid JSON")
}

fn page(slug: &str, title: &str, group: &str, markdown: &str) -> Page {
    Page {
        slug: slug.into(),
        title: title.into(),
        group: group.into(),
        summary: String::new(),
        signature: String::new(),
        markdown: markdown.into(),
    }
}

fn fresh_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("osprey_html_test_{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn read(dir: &Path, relative: &str) -> String {
    std::fs::read_to_string(dir.join(relative)).unwrap_or_default()
}

#[test]
fn generated_paths_stay_inside_the_trees_the_exporter_owns() {
    // A slug reaches the exporter from user input. Traversal, absolute paths
    // and roots the exporter does not own are refused for the whole set before
    // a single byte is written.
    let dir = fresh_dir("paths");
    for slug in ["../escape", "api/../../escape", "/etc/passwd", "src/main"] {
        let hostile = vec![page(slug, "Escape", "Modules", "# Escape\n")];
        assert!(
            generate(&dir, &hostile, "osprey", &[]).is_err(),
            "slug {slug} was accepted"
        );
        assert!(!dir.join("index.html").exists(), "{slug} wrote anyway");
    }
    // Two pages whose slugs differ only in case would silently overwrite each
    // other on a case-insensitive filesystem.
    let clash = vec![
        page("api/Math", "Math", "Modules", "# Math\n"),
        page("api/math", "math", "Modules", "# math\n"),
    ];
    assert!(
        generate(&dir, &clash, "osprey", &[]).is_err(),
        "a case-only slug collision was accepted"
    );
    // The trees it does own are written.
    let owned = vec![page("api/math", "Math", "Modules", "# Math\n")];
    generate(&dir, &owned, "osprey", &[]).expect("owned paths are written");
    assert!(dir.join("api/math.html").is_file());
}

#[test]
fn every_theme_defines_the_same_variables_and_they_differ() {
    let names = ["osprey", "midnight", "paper"];
    let sheets: Vec<String> = names.iter().map(|n| theme::css(n)).collect();
    for (name, css) in names.iter().zip(&sheets) {
        for var in ["--bg", "--text", "--accent", "--border", "--code-bg"] {
            assert!(css.contains(var), "{name} theme is missing {var}");
        }
        assert!(css.contains(".shell"), "{name} theme lost the layout");
        // Self-contained: no CDN, no remote font.
        assert!(!css.contains("http"), "{name} theme reaches the network");
    }
    // Every theme must be distinct from every other, not merely from its
    // neighbour: a set collapses any pair that renders identically.
    let distinct: std::collections::BTreeSet<&String> = sheets.iter().collect();
    assert_eq!(distinct.len(), names.len(), "two themes render identically");
    assert!(theme::css("midnight").contains("color-scheme:dark"));
    // An unknown name is never a panic; parsing rejects those already.
    assert_eq!(theme::css("nonexistent"), theme::css("osprey"));
}

#[test]
fn a_generated_site_is_self_contained_and_navigable() {
    let dir = fresh_dir("site");
    let pages = vec![
        page(
            "api/index",
            "API Reference",
            "Overview",
            "# API\n\n[Math](math.md)\n",
        ),
        page("api/math", "Math", "Modules", "# Math\n\nAdds numbers.\n"),
    ];
    let css = vec![Stylesheet {
        name: "custom-0-brand.css".into(),
        css: "body{--accent:#f0f}".into(),
    }];
    generate(&dir, &pages, "paper", &css).expect("generates");

    // Every declared file exists, including the landing page a static server
    // needs and the search index.
    for file in [
        "index.html",
        "api/index.html",
        "api/math.html",
        "assets/theme.css",
        "assets/custom-0-brand.css",
        "assets/search-index.js",
    ] {
        assert!(dir.join(file).is_file(), "missing {file}");
    }

    let math = read(&dir, "api/math.html");
    // Relative depth is computed per page, so a nested page still finds assets.
    assert!(math.contains("href=\"../assets/theme.css\""), "{math}");
    // User CSS is linked AFTER the theme so it can override it.
    let theme_at = math.find("assets/theme.css").unwrap_or(0);
    let custom_at = math.find("custom-0-brand.css").unwrap_or(0);
    assert!(theme_at < custom_at, "user css must load after the theme");
    // Accessibility scaffolding a keyboard user depends on.
    assert!(math.contains("class=\"skip\""), "no skip link");
    assert!(
        math.contains("aria-current=\"page\""),
        "current page unmarked"
    );
    assert!(
        math.contains("aria-live"),
        "search results are not announced"
    );
    assert!(math.contains("width=device-width"), "not mobile-ready");
    // Nothing is fetched from the network.
    assert!(
        !math.contains("https://"),
        "page reaches the network: {math}"
    );

    assert_eq!(search_index(&dir).as_array().map(Vec::len), Some(2));
}

#[test]
fn regeneration_prunes_only_what_the_exporter_wrote() {
    let dir = fresh_dir("prune");
    let first = vec![
        page("api/keep", "Keep", "Modules", "# Keep\n"),
        page("api/drop", "Drop", "Modules", "# Drop\n"),
    ];
    generate(&dir, &first, "osprey", &[]).expect("first pass");
    // A file the exporter did not write shares the directory.
    std::fs::write(dir.join("api/handwritten.html"), "mine").expect("write");

    let second = vec![page("api/keep", "Keep", "Modules", "# Keep\n")];
    generate(&dir, &second, "osprey", &[]).expect("second pass");

    assert!(dir.join("api/keep.html").is_file(), "kept page was removed");
    assert!(
        !dir.join("api/drop.html").exists(),
        "obsolete page survived"
    );
    assert_eq!(
        read(&dir, "api/handwritten.html"),
        "mine",
        "an unrelated file must never be pruned"
    );
}

#[test]
fn hostile_documentation_text_cannot_break_out_of_any_context() {
    let dir = fresh_dir("hostile");
    let hostile = Page {
        slug: "api/evil".into(),
        title: "Title \"><script>alert(1)</script>".into(),
        group: "Group & <b>bold</b>".into(),
        summary: "Summary \"quoted\" & </script>".into(),
        signature: String::new(),
        markdown: "# Heading\n\n<script>alert(2)</script>\n".into(),
    };
    generate(&dir, std::slice::from_ref(&hostile), "osprey", &[]).expect("generates");
    let html = read(&dir, "api/evil.html");
    assert!(
        !html.contains("<script>alert(1)"),
        "title escaped out: {html}"
    );
    assert!(
        !html.contains("<script>alert(2)"),
        "body escaped out: {html}"
    );
    // The index stays parseable even with `</script>` in the text, and cannot
    // close a `<script>` element if it is ever inlined into one.
    let raw = read(&dir, "assets/search-index.js");
    assert!(!raw.contains("</script>"), "index can close its element");
    assert_eq!(search_index(&dir).as_array().map(Vec::len), Some(1));
}

#[test]
fn an_empty_export_still_produces_a_usable_landing_page() {
    let dir = fresh_dir("empty");
    generate(&dir, &[], "osprey", &[]).expect("generates");
    let index = read(&dir, "index.html");
    assert!(index.contains("Osprey documentation"), "no landing page");
    assert!(dir.join("assets/search-index.js").is_file());
    assert_eq!(
        read(&dir, "assets/search-index.js"),
        "window.OSPREY_SEARCH=[];\n"
    );
}

#[test]
fn the_exporter_refuses_to_write_through_a_symlink() {
    // A slug with no traversal in it can still escape if the output directory
    // already contains a symlink: the write follows it. Path validation alone
    // does not catch that, so the write itself checks.
    let dir = fresh_dir("symlink");
    let outside = fresh_dir("symlink_target");
    std::fs::create_dir_all(dir.join("api")).expect("dir");
    std::fs::create_dir_all(&outside).expect("outside dir");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outside.join("stolen.html"), dir.join("api/evil.html"))
            .expect("symlink");
        let pages = vec![page("api/evil", "Evil", "Modules", "# Evil\n")];
        let result = generate(&dir, &pages, "osprey", &[]);
        assert!(result.is_err(), "a symlinked target must be refused");
        assert!(
            !outside.join("stolen.html").exists(),
            "the write escaped the output directory"
        );
    }
}

#[test]
fn summaries_reach_plain_text_contexts_without_markdown_markers() {
    // The summary is reused where nothing renders Markdown: search results,
    // the landing page and <meta name="description">. Asterisks and backticks
    // there are noise, not formatting.
    assert_eq!(
        render::plain("**Signature:** `map(x: int) -> int`"),
        "Signature: map(x: int) -> int"
    );
    assert_eq!(
        render::plain("# Heading   with\n  gaps"),
        "Heading with gaps"
    );
    assert_eq!(render::plain("_em_ and __strong__"), "em and strong");

    let dir = fresh_dir("summaries");
    let mut with_summary = page("api/m", "M", "Modules", "# M\n");
    with_summary.summary = "**Bold** summary".into();
    generate(&dir, std::slice::from_ref(&with_summary), "osprey", &[]).expect("generates");
    let html = read(&dir, "api/m.html");
    assert!(html.contains("Bold summary"), "markers survived: {html}");
    assert!(!html.contains("**Bold**"), "raw markdown in page: {html}");
    let index = read(&dir, "assets/search-index.js");
    assert!(
        !index.contains("**Bold**"),
        "raw markdown in index: {index}"
    );
}

#[test]
fn search_works_from_the_filesystem_without_fetching_anything() {
    // A page opened straight off disk has the opaque origin `null`, and a
    // `fetch()` of a sibling file from there is a CORS failure with no visible
    // cause: the search box would silently never match anything. The index has
    // to arrive as a script the document loads directly.
    let dir = fresh_dir("file_url");
    let pages = vec![page("api/math", "Math", "Modules", "# Math\n")];
    generate(&dir, &pages, "osprey", &[]).expect("generates");

    let html = read(&dir, "api/math.html");
    assert!(!html.contains("fetch("), "search still fetches: {html}");
    assert!(
        html.contains("src=\"../assets/search-index.js\""),
        "index is not loaded as a script: {html}"
    );
    let index = read(&dir, "assets/search-index.js");
    assert!(
        index.starts_with("window.OSPREY_SEARCH="),
        "index is not a script: {index}"
    );
    assert!(index.contains("\"api/math\""), "page missing from index");
}

#[test]
fn documentation_symbol_links_point_at_the_declaration_they_name() {
    let dir = fresh_dir("symbols");
    let pages = vec![
        // Titles are the fully qualified names the compiler produces. A doc
        // comment names a sibling the way a reader would — by owner and
        // member — not by repeating the namespace it is already inside.
        page(
            "api/ledger",
            "bank::Ledger",
            "Module",
            "# Ledger\n\nSee [Ledger.entries], [Ledger] and [post].\n",
        ),
        page(
            "api/ledger-entries",
            "bank::Ledger::entries",
            "Value",
            "# e\n",
        ),
        page("api/ledger-post", "bank::Ledger::post", "Function", "# p\n"),
        page("api/audit-post", "bank::Audit::post", "Function", "# p\n"),
    ];
    generate(&dir, &pages, "osprey", &[]).expect("generates");
    let body = article(&read(&dir, "api/ledger.html"));

    // A dotted link names its owner, so it resolves even where the leaf alone
    // would not ([DOC-LINK]).
    assert!(
        body.contains("href=\"../api/ledger-entries.html\""),
        "dotted symbol link unresolved: {body}"
    );
    // An exact qualified name resolves too.
    assert!(
        body.contains("href=\"../api/ledger.html\""),
        "own name unresolved: {body}"
    );
    // `post` is owned by two modules. Picking one would send the reader to a
    // declaration the author did not mean, so the link stays plain text.
    assert!(
        !body.contains("audit-post.html") && !body.contains("ledger-post.html"),
        "an ambiguous leaf was linked anyway: {body}"
    );
    assert!(
        body.contains("[post]"),
        "an unresolved link lost its text: {body}"
    );
}

#[test]
fn guide_links_resolve_to_the_slugs_the_exporter_actually_wrote() {
    // Guide pages are slugged component by component, so a link an author
    // wrote against their own filenames — spaces and all, or percent-encoded
    // by their editor — has to go through the SAME transformation. Left alone
    // it points at a file that was never written.
    let dir = fresh_dir("guide_links");
    let pages = vec![
        page(
            "guides/getting-started",
            "Getting Started",
            "Guides",
            "# Getting Started\n\n[deep](<Deep Dive/Advanced Topics.md>)\n",
        ),
        page(
            "guides/deep-dive/advanced-topics",
            "Advanced Topics",
            "Guides",
            "# Advanced Topics\n\n[back](../Getting%20Started.md#intro)\n\
             ![logo](../logo.png)\n",
        ),
    ];
    generate(&dir, &pages, "osprey", &[]).expect("generates");

    let start = read(&dir, "guides/getting-started.html");
    assert!(
        start.contains("href=\"deep-dive/advanced-topics.html\""),
        "nested guide link unresolved: {start}"
    );
    let deep = read(&dir, "guides/deep-dive/advanced-topics.html");
    assert!(
        deep.contains("href=\"../getting-started.html#intro\""),
        "encoded-space link unresolved: {deep}"
    );
    // An image is not a page: retargeting it would break the reference.
    assert!(
        deep.contains("src=\"../logo.png\""),
        "asset link was rewritten: {deep}"
    );
}

#[test]
fn a_symbol_link_resolves_in_its_own_scope_before_anywhere_else() {
    // Two modules each own a `helper`. A comment inside one of them means ITS
    // helper — that is what the name means where it was written — so a global
    // uniqueness rule would refuse a link the author had every right to expect.
    let dir = fresh_dir("scoped");
    let pages = vec![
        page(
            "api/a-read",
            "shop::A::read",
            "Function",
            "# read\n\nDelegates to [helper]. Compare [B.helper] and [missing].\n",
        ),
        page("api/a-helper", "shop::A::helper", "Function", "# h\n"),
        page(
            "api/b-read",
            "shop::B::read",
            "Function",
            "# read\n\nUses [helper].\n",
        ),
        page("api/b-helper", "shop::B::helper", "Function", "# h\n"),
    ];
    generate(&dir, &pages, "osprey", &[]).expect("generates");

    let a = article(&read(&dir, "api/a-read.html"));
    assert!(
        a.contains("href=\"../api/a-helper.html\""),
        "[helper] did not resolve to its own module: {a}"
    );
    assert!(
        !a.contains("b-helper.html\">helper"),
        "[helper] resolved to the other module: {a}"
    );
    // The same spelling in the sibling module resolves to the sibling's own.
    let b = article(&read(&dir, "api/b-read.html"));
    assert!(
        b.contains("href=\"../api/b-helper.html\""),
        "[helper] did not resolve in module B: {b}"
    );
    // An explicit owner still reaches across.
    assert!(
        a.contains("href=\"../api/b-helper.html\""),
        "[B.helper] did not resolve across modules: {a}"
    );
    // A name this export documents nowhere stays text rather than becoming a
    // link to a page that was never written.
    assert!(a.contains("[missing]"), "unknown symbol was linked: {a}");
}

#[test]
fn navigation_collapses_on_a_phone_and_opens_from_the_keyboard() {
    // The sidebar precedes the article in source order — right for a screen
    // reader, wrong for a phone, where 130 links push the article off screen.
    // A disclosure fixes that without a script and without losing the links.
    let dir = fresh_dir("menu");
    let pages = vec![page("api/math", "Math", "Modules", "# Math\n")];
    generate(&dir, &pages, "osprey", &[]).expect("generates");
    let html = read(&dir, "api/math.html");

    // `<details>`/`<summary>` is operable by keyboard with no script of ours,
    // and `open` in the markup means the links are all still there when
    // scripting is off.
    assert!(html.contains("<details id=\"menu\" open>"), "{html}");
    assert!(
        html.contains("<summary>Browse documentation</summary>"),
        "no disclosure control: {html}"
    );
    let css = theme::css("osprey");
    assert!(
        css.contains("#menu>summary{display:none}"),
        "the control is not hidden on a wide screen: {css}"
    );
    assert!(
        css.contains("#menu>summary{display:block"),
        "the control is not shown on a phone: {css}"
    );
    // The script closes it where the layout is narrow.
    assert!(html.contains("menu.open=!narrow.matches"), "{html}");
}
