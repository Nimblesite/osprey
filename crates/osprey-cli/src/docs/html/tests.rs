//! Coverage for the HTML exporter: escaping in every context it writes, the
//! raw-HTML policy, link retargeting, path validation, pruning and themes.

use super::{generate, render, theme, validate_path, Page, Stylesheet};
use std::path::{Path, PathBuf};

fn page(slug: &str, title: &str, group: &str, markdown: &str) -> Page {
    Page {
        slug: slug.into(),
        title: title.into(),
        group: group.into(),
        summary: String::new(),
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
fn each_escaping_context_neutralises_its_own_terminator() {
    // Element text, attribute values and JSON each end at a different
    // character, so one escaper cannot serve all three.
    assert_eq!(render::text("a<b>&c"), "a&lt;b&gt;&amp;c");
    assert_eq!(
        render::attr("say \"hi\" & 'bye'"),
        "say &quot;hi&quot; &amp; &#39;bye&#39;"
    );
    // The search index sits inside <script type="application/json">, where a
    // literal </script> would close the element early.
    let escaped = render::json("</script><img src=x>");
    assert!(!escaped.contains('<'), "raw < survived: {escaped}");
    assert!(escaped.contains("\\u003c"), "not escaped: {escaped}");
    assert_eq!(render::json("tab\there\n\"q\""), "tab\\there\\n\\\"q\\\"");
}

#[test]
fn raw_html_in_documentation_is_shown_not_executed() {
    // A doc comment is untrusted text. Raw HTML must arrive as visible
    // characters, never as markup that could inject script into the site.
    let out = render::markdown("<script>alert(1)</script>\n\nplain");
    assert!(!out.contains("<script>"), "script survived: {out}");
    assert!(out.contains("&lt;script&gt;"), "not escaped: {out}");
    let inline = render::markdown("text <img src=x onerror=y> more");
    assert!(!inline.contains("<img"), "inline html survived: {inline}");
}

#[test]
fn markdown_features_the_reference_needs_are_enabled() {
    let table = render::markdown("| a | b |\n|---|---|\n| 1 | 2 |\n");
    assert!(table.contains("<table>"), "tables disabled: {table}");
    let code = render::markdown("```osprey\nfn f() = 1\n```\n");
    assert!(code.contains("<pre><code"), "code fence lost: {code}");
    assert!(code.contains("fn f() = 1"), "code body lost: {code}");
}

#[test]
fn internal_links_point_at_files_the_export_actually_writes() {
    // The Markdown sources are shared with the 11ty site, which serves pretty
    // URLs and `.md` paths. Left alone those links 404 in a static export.
    let pretty = render::markdown("[Channel](channel/)");
    assert!(pretty.contains("href=\"channel.html\""), "{pretty}");
    let source = render::markdown("[Map](api/map.md)");
    assert!(source.contains("href=\"api/map.html\""), "{source}");
    let anchored = render::markdown("[Part](guide/#usage)");
    assert!(anchored.contains("href=\"guide.html#usage\""), "{anchored}");
    // External links and bare fragments are left exactly as written.
    let external = render::markdown("[Site](https://example.com/a/)");
    assert!(
        external.contains("href=\"https://example.com/a/\""),
        "{external}"
    );
    let fragment = render::markdown("[Here](#section)");
    assert!(fragment.contains("href=\"#section\""), "{fragment}");
}

#[test]
fn generated_paths_stay_inside_the_trees_the_exporter_owns() {
    assert!(validate_path("api/math.html").is_ok());
    assert!(validate_path("assets/theme.css").is_ok());
    assert!(validate_path("index.html").is_ok());
    // Traversal, absolute paths and foreign roots are refused: a slug reaches
    // this from user input.
    assert!(validate_path("../escape.html").is_err());
    assert!(validate_path("api/../../escape.html").is_err());
    assert!(validate_path("/etc/passwd").is_err());
    assert!(validate_path("src/main.rs").is_err());
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
    assert_ne!(sheets[0], sheets[1], "osprey and midnight are identical");
    assert_ne!(sheets[1], sheets[2], "midnight and paper are identical");
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
        "assets/search-index.json",
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

    let index: serde_json::Value =
        serde_json::from_str(&read(&dir, "assets/search-index.json")).expect("valid JSON");
    assert_eq!(index.as_array().map(Vec::len), Some(2));
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
    // The index stays parseable even with `</script>` in the text.
    let raw = read(&dir, "assets/search-index.json");
    assert!(!raw.contains("</script>"), "index can close its element");
    let parsed: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");
    assert_eq!(parsed.as_array().map(Vec::len), Some(1));
}

#[test]
fn an_empty_export_still_produces_a_usable_landing_page() {
    let dir = fresh_dir("empty");
    generate(&dir, &[], "osprey", &[]).expect("generates");
    let index = read(&dir, "index.html");
    assert!(index.contains("Osprey documentation"), "no landing page");
    assert!(dir.join("assets/search-index.json").is_file());
    assert_eq!(read(&dir, "assets/search-index.json"), "[]");
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
    let index = read(&dir, "assets/search-index.json");
    assert!(
        !index.contains("**Bold**"),
        "raw markdown in index: {index}"
    );
}
