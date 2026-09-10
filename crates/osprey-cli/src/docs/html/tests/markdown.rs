//! What the Markdown renderer produces: the escaping context each output
//! position needs, the raw-HTML policy, link retargeting and refusal, and the
//! heading anchors that let a link reach a section rather than a page.

use super::md;
use crate::docs::html::render;

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
    let out = md("<script>alert(1)</script>\n\nplain");
    assert!(!out.contains("<script>"), "script survived: {out}");
    assert!(out.contains("&lt;script&gt;"), "not escaped: {out}");
    let inline = md("text <img src=x onerror=y> more");
    assert!(!inline.contains("<img"), "inline html survived: {inline}");
}

#[test]
fn markdown_features_the_reference_needs_are_enabled() {
    let table = md("| a | b |\n|---|---|\n| 1 | 2 |\n");
    assert!(table.contains("<table>"), "tables disabled: {table}");
    let code = md("```osprey\nfn f() = 1\n```\n");
    assert!(code.contains("<pre><code"), "code fence lost: {code}");
    assert!(code.contains("fn f() = 1"), "code body lost: {code}");
}

#[test]
fn internal_links_point_at_files_the_export_actually_writes() {
    // The Markdown sources are shared with the 11ty site, which serves pretty
    // URLs and `.md` paths. Left alone those links 404 in a static export.
    let pretty = md("[Channel](channel/)");
    assert!(pretty.contains("href=\"channel.html\""), "{pretty}");
    let source = md("[Map](api/map.md)");
    assert!(source.contains("href=\"api/map.html\""), "{source}");
    let anchored = md("[Part](guide/#usage)");
    assert!(anchored.contains("href=\"guide.html#usage\""), "{anchored}");
    // External links and bare fragments are left exactly as written.
    let external = md("[Site](https://example.com/a/)");
    assert!(
        external.contains("href=\"https://example.com/a/\""),
        "{external}"
    );
    let fragment = md("[Here](#section)");
    assert!(fragment.contains("href=\"#section\""), "{fragment}");
}

#[test]
fn link_schemes_that_execute_in_the_reader_are_refused() {
    // Documentation is untrusted text. A link that runs code when clicked is
    // an injection whether or not the markup around it was escaped.
    for hostile in [
        "javascript:alert(1)",
        "JaVaScRiPt:alert(1)",
        "java&#9;script:alert(1)",
        "vbscript:msgbox(1)",
        "data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==",
    ] {
        let out = md(&format!("[click]({hostile})")).to_lowercase();
        assert!(!out.contains("javascript:"), "{hostile} survived: {out}");
        assert!(!out.contains("vbscript:"), "{hostile} survived: {out}");
        assert!(!out.contains("href=\"data:"), "{hostile} survived: {out}");
        assert!(out.contains("click"), "{hostile} lost its text: {out}");
    }
    // An image source is the same hazard and gets the same treatment.
    let image = md("![x](javascript:alert(1))").to_lowercase();
    assert!(
        !image.contains("javascript:"),
        "image scheme survived: {image}"
    );
}

#[test]
fn headings_carry_anchors_so_a_link_can_reach_a_section() {
    // CommonMark defines no heading anchors, so `guide.md#examples` landed on
    // the page and then sat at the top of it — a link that looks like it
    // worked. Anchors come from the heading text.
    let out = md("# Examples\n\nfirst\n\n## Deeper Section\n\n## Examples\n\n## Examples\n");
    assert!(out.contains("<h1 id=\"examples\">"), "no h1 anchor: {out}");
    assert!(
        out.contains("<h2 id=\"deeper-section\">"),
        "text not slugged: {out}"
    );
    // A repeated heading takes a suffix. Two elements sharing an id makes the
    // second one unreachable — the same silent half-failure, one step on.
    assert!(out.contains("<h2 id=\"examples-2\">"), "no suffix: {out}");
    assert!(
        out.contains("<h2 id=\"examples-3\">"),
        "suffix stuck: {out}"
    );
    // Code spans are part of the heading a reader sees, so part of its anchor.
    let code = md("## The `map` function\n");
    assert!(
        code.contains("id=\"the-map-function\""),
        "code span dropped: {code}"
    );
    // An anchor an author wrote by hand is theirs, not ours to renumber.
    let explicit = md("## Custom {#mine}\n");
    assert!(
        explicit.contains("id=\"mine\""),
        "explicit id lost: {explicit}"
    );
}

#[test]
fn heading_ids_avoid_suffix_explicit_and_page_landmark_collisions() {
    let out = md("## Examples\n## Examples-2\n## Examples\n## Content\n## q\n## Custom\n## Pinned {#custom}\n");
    for id in [
        "examples",
        "examples-2",
        "examples-3",
        "content-2",
        "q-2",
        "custom-2",
        "custom",
    ] {
        assert_eq!(out.matches(&format!("id=\"{id}\"")).count(), 1, "{out}");
    }
}

#[test]
fn heading_attributes_allow_styling_but_never_event_handlers() {
    let out = md("## Safe {#safe .wide onclick=alert(1) onmouseover=alert(2)}\n");
    assert!(
        out.contains("id=\"safe\"") && out.contains("class=\"wide\""),
        "{out}"
    );
    assert!(
        !out.contains("onclick=") && !out.contains("onmouseover="),
        "{out}"
    );
}

#[test]
fn footnotes_keep_distinct_targets_and_cannot_steal_a_landmark_id() {
    let out = md("## Content\n\nSee [^content], [^α] and [^β].\n\n[^content]: First.\n[^α]: Second.\n[^β]: Third.\n");
    assert!(out.contains("<h2 id=\"content-2\">"), "{out}");
    for id in ["footnote-content", "footnote-page", "footnote-page-2"] {
        assert_eq!(out.matches(&format!("id=\"{id}\"")).count(), 1, "{out}");
        assert!(out.contains(&format!("href=\"#{id}\"")), "{out}");
    }
}
